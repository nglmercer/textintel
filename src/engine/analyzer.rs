//! Engine analysis: text → [`MessageFingerprint`] plus rebus decoding.
//! The staged pipeline ([`TextIntelligence::analyze`]) stays here as one
//! cohesive unit; construction, comparison, patterns, search, and
//! diagnostics live in their own modules.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use crate::cache::{CacheDiagnostics, rebus_cache_key};
use crate::core::error::{ProviderError, TextIntelError};
use crate::core::providers::LexiconProvider;
use crate::core::types::{
    ChannelAvailability, DecodedCandidate, LexicalFeatures, MessageFingerprint, PhoneticCandidate,
    SpokenCandidate, StageTimings, Transformation,
};
use crate::language::segmentation::segment_message_with_provider;
use crate::lexical::character::char_features;
use crate::lexical::minhash::{minhash_signature, simhash};
use crate::lexical::ngrams::word_ngrams;
use crate::lexical::tokenizer::{simple_lemmas_with_provider, stop_words_with_provider, tokenize};
use crate::normalization::leetspeak::apply_leet;
use crate::normalization::repetition::collapse_repetition;
use crate::normalization::unicode::{casefold_text, nfkc};
use crate::normalization::whitespace::normalize_whitespace;
use crate::obfuscation::features::obfuscation_features;
use crate::rebus::decoder::RebusDecoder;
use crate::symbols::resolver::resolve_symbols_with_provider;
use crate::visual::unicode_features::analyze_unicode;

// Historic path stability: the type moved to the engine root (`mod.rs`) so
// every engine submodule can reach it; the old path keeps resolving.
pub use super::TextIntelligence;

#[derive(Debug, Clone)]
struct EmbeddingInput {
    key: String,
    text: String,
}

/// Fraction of words present in the lexicon (any language), measuring the
/// *intended* reading: when the top rebus candidate differs from the raw
/// text, coverage runs over the candidate's words (`h3llo` counts through
/// `hello`); otherwise it runs over the raw tokens. Segments without a
/// letter (URLs, emoji, pure numbers) are not words and are excluded from
/// both numerator and denominator; texts without words report 0.0. Each
/// word also counts through its alphanumeric fold, so zero-width and
/// punctuation noise (`co\u{200b}de`) does not fake invalidity.
fn lexicon_coverage(
    raw: &str,
    tokens: &[String],
    decoded_top: Option<&str>,
    provider: &dyn LexiconProvider,
) -> f64 {
    let decoded_words: Vec<&str>;
    let words: Vec<&str> = match decoded_top {
        Some(top) if alphanumeric_fold(top) != alphanumeric_fold(raw) => {
            decoded_words = top
                .split_whitespace()
                .filter(|token| token.chars().any(|ch| ch.is_alphabetic()))
                .collect();
            decoded_words
        }
        _ => tokens
            .iter()
            .map(String::as_str)
            .filter(|token| token.chars().any(|ch| ch.is_alphabetic()))
            .collect(),
    };
    if words.is_empty() {
        return 0.0;
    }
    let known = words
        .iter()
        .filter(|token| {
            provider.contains(token, None)
                || (!is_fold_stable(token) && provider.contains(&alphanumeric_fold(token), None))
        })
        .count();
    (known as f64 / words.len() as f64).clamp(0.0, 1.0)
}

/// True when [`alphanumeric_fold`] is the identity: ASCII lowercase
/// alphanumerics casefold to themselves and nothing is filtered, so a
/// second lookup on the fold would repeat the first.
fn is_fold_stable(token: &str) -> bool {
    token
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn alphanumeric_fold(text: &str) -> String {
    crate::normalization::unicode::casefold_text(text)
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

/// Truncate to `max_chars` characters on a char boundary (deterministic
/// preprocessing for bounded embedding inputs).
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

impl TextIntelligence {
    /// Semantic-rescoring marker for rebus cache keys: `sem:off` when
    /// rescoring is disabled, otherwise the embedding model identity so model
    /// swaps key separately.
    fn rebus_semantic_marker(&self) -> String {
        if !self.config.semantic {
            return "sem:off".to_string();
        }
        if let Some(metadata) = self.embedding_provider.model_metadata() {
            return format!(
                "sem:{}@{}#{}",
                metadata.model_id,
                metadata.revision.as_deref().unwrap_or("-"),
                metadata.dimensions
            );
        }
        let capabilities = self.embedding_provider.capabilities();
        format!(
            "sem:{}@{}#{}",
            capabilities.provider,
            capabilities.model_revision.as_deref().unwrap_or("-"),
            capabilities.dimensions.unwrap_or(0)
        )
    }

    fn rebus_cache_lookup(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
        semantic: &str,
    ) -> Option<Vec<DecodedCandidate>> {
        let cache = self.rebus_cache.as_ref()?;
        let key = rebus_cache_key(
            text,
            languages,
            max_candidates,
            &self.config.rebus_weights,
            semantic,
        );
        cache.lock().ok()?.get(&key)
    }

    fn rebus_cache_store(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
        semantic: &str,
        candidates: Vec<DecodedCandidate>,
    ) {
        let Some(cache) = self.rebus_cache.as_ref() else {
            return;
        };
        let key = rebus_cache_key(
            text,
            languages,
            max_candidates,
            &self.config.rebus_weights,
            semantic,
        );
        if let Ok(mut guard) = cache.lock() {
            guard.put(key, candidates);
        }
    }

    /// Observable rebus-cache state (counts only, never cached texts).
    pub(super) fn rebus_cache_diagnostics(&self) -> CacheDiagnostics {
        self.rebus_cache
            .as_ref()
            .and_then(|cache| cache.lock().ok().map(|guard| guard.diagnostics()))
            .unwrap_or_else(CacheDiagnostics::disabled)
    }

    /// Analyze with the exact-text decision cache when enabled. Hits
    /// return cloned fingerprints (analysis is deterministic); misses
    /// analyze and store. A poisoned lock degrades to uncached analysis
    /// rather than failing the request.
    pub fn analyze_cached(&self, text: &str) -> Result<MessageFingerprint, TextIntelError> {
        if let Some(cache) = self.decision_fp_cache.as_ref()
            && let Ok(mut guard) = cache.lock()
            && let Some(hit) = guard.get(text)
        {
            return Ok(hit);
        }
        let fingerprint = self.analyze(text)?;
        if let Some(cache) = self.decision_fp_cache.as_ref()
            && let Ok(mut guard) = cache.lock()
        {
            guard.put(text.to_string(), fingerprint.clone());
        }
        Ok(fingerprint)
    }

    /// Observable decision-cache state (counts only, never cached texts).
    pub(super) fn decision_cache_diagnostics(&self) -> CacheDiagnostics {
        self.decision_fp_cache
            .as_ref()
            .and_then(|cache| cache.lock().ok().map(|guard| guard.diagnostics()))
            .unwrap_or_else(CacheDiagnostics::disabled)
    }

    fn check_length(&self, text: &str) -> Result<usize, TextIntelError> {
        let length = text.chars().count();
        if length > self.config.max_input_length {
            return Err(TextIntelError::InputTooLong {
                length,
                maximum: self.config.max_input_length,
            });
        }
        Ok(length)
    }

    fn detect(
        &self,
        text: &str,
    ) -> Result<Vec<crate::core::types::LanguageCandidate>, TextIntelError> {
        self.language_provider
            .detect(text)
            .map_err(TextIntelError::from)
    }

    fn build_phonetic_candidates(
        &self,
        raw: &str,
        languages: &[crate::core::types::LanguageCandidate],
        rebus: &[DecodedCandidate],
    ) -> Result<Vec<PhoneticCandidate>, TextIntelError> {
        if !self.config.phonetic {
            return Ok(Vec::new());
        }
        let mut values = Vec::new();
        let mut seen = BTreeSet::new();
        // Configured hints win over detection for G2P voice selection, then
        // detected languages fill the remaining slots.
        let mut language_list: Vec<String> = self
            .config
            .language_hints
            .iter()
            .filter(|hint| hint.as_str() != "unknown")
            .cloned()
            .collect();
        language_list.extend(
            languages
                .iter()
                .filter(|candidate| candidate.language != "unknown")
                .map(|candidate| candidate.language.clone()),
        );
        language_list.dedup();
        language_list.truncate(3);
        let language_list = if language_list.is_empty() {
            vec!["und".to_string()]
        } else {
            language_list
        };
        for language in &language_list {
            let mut texts = vec![raw.to_string()];
            texts.extend(rebus.iter().take(3).map(|decoded| decoded.text.clone()));
            let candidates = self
                .g2p_provider
                .phonemize_batch(&texts, language)
                .map_err(TextIntelError::from)?;
            for candidate in candidates {
                if seen.insert((candidate.source.clone(), candidate.language.clone())) {
                    values.push(candidate);
                }
            }
        }
        Ok(values)
    }

    fn analyze_base(
        &self,
        text: &str,
    ) -> Result<(MessageFingerprint, Vec<EmbeddingInput>), TextIntelError> {
        let (fingerprint, inputs, _) = self.analyze_stages(text)?;
        Ok((fingerprint, inputs))
    }

    /// [`analyze_base`](Self::analyze) plus per-stage timings. Durations only;
    /// no text or vectors ever enter [`StageTimings`].
    fn analyze_stages(
        &self,
        text: &str,
    ) -> Result<(MessageFingerprint, Vec<EmbeddingInput>, StageTimings), TextIntelError> {
        let input_length = self.check_length(text)?;
        let total_started = Instant::now();
        let mut timings = StageTimings::default();
        let elapsed = |started: Instant| started.elapsed().as_secs_f64() * 1_000_000.0;

        let started = Instant::now();
        let unicode = analyze_unicode(text);
        timings.normalization_micros += elapsed(started);

        let started = Instant::now();
        let languages = self.detect(text)?;
        let language_names: Vec<String> = languages
            .iter()
            .map(|candidate| candidate.language.clone())
            .collect();
        let segments = segment_message_with_provider(
            text,
            self.config.max_segments,
            self.language_provider.as_ref(),
        )
        .map_err(TextIntelError::from)?;
        // Piece splits do not depend on the detector, so when segmentation
        // was not truncated the segments already hold every token and the
        // second segmentation pass is skipped; truncated runs fall back.
        let tokens = if segments.len() < self.config.max_segments {
            segments
                .iter()
                .map(|segment| segment.text.clone())
                .collect()
        } else {
            tokenize(text)
        };
        let lemmas = match &self.lemmatizer_provider {
            Some(provider) => provider
                .lemmatize(&tokens, None)
                .map_err(TextIntelError::from)?,
            None => simple_lemmas_with_provider(
                &tokens,
                Some(&language_names),
                self.lexicon_provider.as_ref(),
            ),
        };
        let lexical = LexicalFeatures {
            tokens: tokens.clone(),
            lemmas: lemmas.clone(),
            stop_words: stop_words_with_provider(
                &tokens,
                Some(&language_names),
                self.lexicon_provider.as_ref(),
            ),
            word_ngrams: word_ngrams(&tokens, 2),
            token_ngrams: word_ngrams(&tokens, 3),
            jaccard_ready: lemmas.iter().cloned().collect(),
            simhash: simhash(&lemmas, 64),
            minhash: minhash_signature(&lemmas, 32),
        };
        timings.language_micros += elapsed(started);

        let started = Instant::now();
        let symbols = resolve_symbols_with_provider(
            text,
            self.config.max_symbol_readings,
            self.symbol_provider.as_ref(),
        );
        timings.symbols_micros += elapsed(started);

        let started = Instant::now();
        let obfuscation = obfuscation_features(text, &unicode);
        timings.normalization_micros += elapsed(started);

        // Bounded entity evidence, keyed by the top detected language. The
        // provider enforces its own bounds; the configured limits apply on
        // top so deployments can tighten without swapping providers.
        let entity_language = languages
            .first()
            .map(|candidate| candidate.language.clone());
        let mut entities = match &self.entity_provider {
            Some(provider) => provider.extract(text, entity_language.as_deref()),
            None => Vec::new(),
        };
        entities
            .retain(|mention| mention.value.chars().count() <= self.config.max_entity_span_chars);
        entities.truncate(self.config.max_entities);

        // Configured hints override detected languages for decoding; hints
        // never change detection itself. Otherwise scope follows detection,
        // ranked best-first: the long tail is noise that crowds the true
        // language's readings out of the beam (`I ❤ NY` decoded to Hindi
        // before English because eight tail languages outranked it), so
        // scope stops at the three strongest substantive hypotheses, which
        // covers monolingual and code-switched text. Two honest fallbacks:
        // when nothing substantive was detected the remainder is empty and
        // un-scopes decoding (the tokenizer treats empty as "all allowed"),
        // and a single segment topped by `unknown` also decodes globally —
        // with no confident detection and no surrounding context, scoping by
        // the tail is pure gamble (short slang like `luv` detects as
        // [unknown, fr, es, ...], cutting the true language's readings),
        // while globally the true reading wins on its own probability.
        // NOTE: multi-segment unknown-topped texts (obfuscated `ch34p`,
        // digit-heavy `2nite`) keep tail scoping: neutral decoding lets
        // high-prior foreign number readings (`2` → `dos`) crowd out the
        // intended reading, which scores worse than the noisy scope. The
        // principled fix (validity-gated scoping plus calibrated
        // cross-language priors) is future work; see the gap analysis.
        let decode_languages: Vec<String> = if self.config.language_hints.is_empty() {
            let top_unknown = language_names
                .first()
                .is_some_and(|top| top.eq_ignore_ascii_case("unknown"));
            if top_unknown && segments.len() == 1 {
                Vec::new()
            } else {
                language_names
                    .iter()
                    .filter(|name| !name.eq_ignore_ascii_case("unknown"))
                    .take(3)
                    .cloned()
                    .collect()
            }
        } else {
            self.config.language_hints.clone()
        };
        let started = Instant::now();
        // Fingerprint rebus decoding never uses semantic rescoring (the
        // `sem:off` key marker); `decode_with_languages` may, and keys
        // separately. Fast decision serving over clean text opts out via
        // `config.rebus = false` (see `EngineConfig::rebus`): no decode
        // runs, and coverage below falls back to the raw tokens.
        let rebus = if !self.config.rebus {
            Vec::new()
        } else {
            let decoded = match self.rebus_cache_lookup(
                text,
                Some(&decode_languages),
                Some(self.config.max_candidates),
                "sem:off",
            ) {
                Some(hit) => hit,
                None => {
                    let decoder = RebusDecoder::new(self.config.clone());
                    let abbreviations = self.abbreviation_provider.as_deref();
                    let decoded = decoder.decode_with_abbreviations(
                        text,
                        Some(&decode_languages),
                        Some(self.config.max_candidates),
                        self.symbol_provider.as_ref(),
                        self.lexicon_provider.as_ref(),
                        self.g2p_provider.as_ref(),
                        None,
                        abbreviations,
                    );
                    self.rebus_cache_store(
                        text,
                        Some(&decode_languages),
                        Some(self.config.max_candidates),
                        "sem:off",
                        decoded.clone(),
                    );
                    decoded
                }
            };
            timings.rebus_micros += elapsed(started);
            decoded
        };
        let spoken_candidates = rebus
            .iter()
            .map(|candidate| SpokenCandidate {
                text: candidate.text.clone(),
                language: candidate.language.clone(),
                probability: candidate.score,
                source_transformations: candidate.transformations.clone(),
                confidence: candidate.score,
                source: "rebus".to_string(),
            })
            .collect::<Vec<_>>();

        let semantic_embeddings = BTreeMap::new();
        let started = Instant::now();
        let phonetic_candidates = self.build_phonetic_candidates(text, &languages, &rebus)?;
        timings.phonetic_micros += elapsed(started);

        let started = Instant::now();
        let normalized = normalize_whitespace(&collapse_repetition(
            &apply_leet(&casefold_text(&nfkc(text))),
            self.config.repetition_keep,
        ));
        let mut normalization_views = BTreeMap::new();
        normalization_views.insert("nfc".to_string(), unicode.nfc.clone());
        normalization_views.insert("nfkc".to_string(), unicode.nfkc.clone());
        normalization_views.insert("casefold".to_string(), unicode.casefolded.clone());
        normalization_views.insert("leet".to_string(), apply_leet(&unicode.casefolded));
        normalization_views.insert(
            "repetition_collapsed".to_string(),
            collapse_repetition(&unicode.casefolded, self.config.repetition_keep),
        );
        normalization_views.insert("normalized".to_string(), normalized.clone());
        // Transliteration views are additive: `raw` is never replaced. Each
        // view records its provider confidence so scoring can weight the
        // conversion instead of trusting it unconditionally.
        let mut transliteration_confidence = BTreeMap::new();
        if let Some(provider) = &self.transliteration_provider {
            for view in provider.transliterate(text) {
                let name = format!("transliteration:{}", view.target_script.to_lowercase());
                transliteration_confidence.insert(name.clone(), view.confidence);
                normalization_views.insert(name, view.text);
            }
        }
        let transformations = normalization_views
            .iter()
            .filter(|(name, value)| value.as_str() != text && name.as_str() != "normalized")
            .map(|(name, value)| {
                let mut step =
                    Transformation::new(text, value.clone(), format!("normalization:{name}"))
                        .with_span(0, text.len(), text);
                if name.starts_with("transliteration:") {
                    step = step.with_provider("transliteration");
                    if let Some(confidence) = transliteration_confidence.get(name) {
                        step = step.with_confidence(*confidence);
                    }
                } else {
                    step = step.with_provider("normalization");
                }
                step
            })
            .collect();
        timings.normalization_micros += elapsed(started);
        let mut channel_availability = BTreeMap::new();
        channel_availability.insert(
            "lexical".to_string(),
            ChannelAvailability::available("resource_index", 0.85),
        );
        channel_availability.insert(
            "character".to_string(),
            ChannelAvailability::available("deterministic", 1.0),
        );
        channel_availability.insert(
            "visual".to_string(),
            ChannelAvailability::available("unicode_analysis", 0.9),
        );
        channel_availability.insert(
            "symbolic".to_string(),
            if symbols.is_empty() {
                ChannelAvailability::unavailable("symbol_index")
            } else {
                ChannelAvailability::available("symbol_index", 0.75)
            },
        );
        channel_availability.insert(
            "decoded".to_string(),
            if self.config.rebus {
                ChannelAvailability::available("bounded_beam_search", 0.7)
            } else {
                ChannelAvailability::unavailable("bounded_beam_search")
            },
        );
        channel_availability.insert(
            "obfuscation".to_string(),
            ChannelAvailability::available("deterministic", 0.9),
        );
        channel_availability.insert(
            "semantic".to_string(),
            if semantic_embeddings.is_empty() {
                ChannelAvailability::unavailable("embedding_provider")
            } else {
                ChannelAvailability::available("embedding_provider", 0.7)
            },
        );
        channel_availability.insert(
            "phonetic".to_string(),
            if phonetic_candidates.is_empty() {
                ChannelAvailability::unavailable("g2p_provider")
            } else {
                ChannelAvailability::available("g2p_provider", 0.65)
            },
        );
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "fingerprint_schema_version".to_string(),
            crate::core::types::FINGERPRINT_SCHEMA_VERSION.to_string(),
        );
        metadata.insert(
            "semantic_enabled".to_string(),
            self.config.semantic.to_string(),
        );
        metadata.insert(
            "phonetic_enabled".to_string(),
            self.config.phonetic.to_string(),
        );
        metadata.insert("rebus_enabled".to_string(), self.config.rebus.to_string());
        metadata.insert("input_length".to_string(), input_length.to_string());
        // Coverage is computed after decoding so validity reflects the
        // intended reading (`h3llo` counts through `hello`), not the raw
        // obfuscation. See `lexicon_coverage`.
        let lexicon_coverage = lexicon_coverage(
            text,
            &tokens,
            rebus.first().map(|candidate| candidate.text.as_str()),
            self.lexicon_provider.as_ref(),
        );
        let embedding_inputs = if self.config.semantic {
            let bound = self.config.embedding_max_chars;
            let mut values = vec![EmbeddingInput {
                key: "default".to_string(),
                text: truncate_chars(text, bound),
            }];
            values.extend(rebus.iter().take(3).enumerate().map(|(index, candidate)| {
                EmbeddingInput {
                    key: format!("decoded:{}", index + 1),
                    text: truncate_chars(&candidate.text, bound),
                }
            }));
            values.extend(
                segments
                    .iter()
                    .filter(|segment| segment.segment_type == "text")
                    .take(16)
                    .enumerate()
                    .map(|(index, segment)| EmbeddingInput {
                        key: format!("segment:{index}"),
                        text: truncate_chars(&segment.text, bound),
                    }),
            );
            values
        } else {
            Vec::new()
        };
        let fingerprint = MessageFingerprint {
            schema_version: crate::core::types::FINGERPRINT_SCHEMA_VERSION,
            raw: text.to_string(),
            normalized: Some(normalized),
            normalization_views,
            transliteration_confidence,
            lexicon_coverage,
            transformations,
            language_candidates: languages,
            segments,
            tokens,
            lemmas,
            char_features: char_features(text),
            unicode_features: unicode,
            symbols,
            lexical_features: lexical,
            semantic_embeddings,
            spoken_candidates,
            phonetic_candidates,
            rebus_candidates: rebus,
            obfuscation_features: obfuscation,
            channel_availability,
            entities,
            metadata,
        };
        timings.total_micros = total_started.elapsed().as_secs_f64() * 1_000_000.0;
        Ok((fingerprint, embedding_inputs, timings))
    }

    fn attach_embeddings(
        &self,
        fingerprint: &mut MessageFingerprint,
        embedding_inputs: &[EmbeddingInput],
        values: Vec<Vec<f32>>,
    ) -> Result<(), TextIntelError> {
        if values.len() != embedding_inputs.len() {
            return Err(TextIntelError::Provider(ProviderError::new(
                "embedding",
                format!(
                    "returned {} vectors for {} inputs",
                    values.len(),
                    embedding_inputs.len()
                ),
            )));
        }
        for (input, vector) in embedding_inputs.iter().zip(values) {
            if vector.iter().any(|value| !value.is_finite()) {
                return Err(TextIntelError::Provider(ProviderError::new(
                    "embedding",
                    "returned a non-finite vector",
                )));
            }
            if let Some(metadata) = self.embedding_provider.model_metadata() {
                metadata.validate_vector(&vector).map_err(|message| {
                    TextIntelError::Provider(ProviderError::new("embedding", message))
                })?;
            }
            if !vector.is_empty() {
                fingerprint
                    .semantic_embeddings
                    .insert(input.key.clone(), vector);
            }
        }
        let availability = fingerprint
            .channel_availability
            .entry("semantic".to_string())
            .or_insert_with(|| ChannelAvailability::unavailable("embedding_provider"));
        if !fingerprint.semantic_embeddings.is_empty() {
            *availability = ChannelAvailability::available("embedding_provider", 0.7);
        }
        fingerprint.metadata.insert(
            "semantic_vectors".to_string(),
            fingerprint.semantic_embeddings.len().to_string(),
        );
        if let Some(metadata) = self.embedding_provider.model_metadata() {
            fingerprint
                .metadata
                .insert("semantic_model".to_string(), metadata.model_id.clone());
            fingerprint.metadata.insert(
                "semantic_dimensions".to_string(),
                metadata.dimensions.to_string(),
            );
            if let Some(revision) = metadata.revision {
                fingerprint
                    .metadata
                    .insert("semantic_revision".to_string(), revision);
            }
        }
        // Quality tier behind the vectors (`production` for transformer
        // backends, `basic` for the feature-hash fallback). The scorer uses
        // it to gate `contextual_semantic` on trustworthy evidence.
        let quality = match self.embedding_provider.capabilities().quality {
            crate::core::capabilities::CapabilityLevel::Production => "production",
            crate::core::capabilities::CapabilityLevel::Basic => "basic",
            crate::core::capabilities::CapabilityLevel::Unavailable => "unavailable",
        };
        fingerprint
            .metadata
            .insert("semantic_quality".to_string(), quality.to_string());
        Ok(())
    }

    /// Provider fan-out bounded by `config.embedding_batch_size`: large
    /// batches are chunked into sequential provider calls and concatenated
    /// in order.
    fn embed_chunked(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, TextIntelError> {
        let bound = self.config.embedding_batch_size.max(1);
        let mut output = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(bound) {
            let mut vectors = self
                .embedding_provider
                .embed_batch(chunk)
                .map_err(TextIntelError::from)?;
            output.append(&mut vectors);
        }
        Ok(output)
    }

    pub fn analyze(&self, text: &str) -> Result<MessageFingerprint, TextIntelError> {
        Ok(self.analyze_with_timing(text)?.0)
    }

    /// [`analyze`](Self::analyze) plus per-stage timings. The timings carry
    /// durations only — no input text, embeddings, or user data — so they are
    /// safe to log and export by default.
    pub fn analyze_with_timing(
        &self,
        text: &str,
    ) -> Result<(MessageFingerprint, StageTimings), TextIntelError> {
        let total_started = Instant::now();
        let (mut fingerprint, embedding_inputs, mut timings) = self.analyze_stages(text)?;
        if self.config.semantic {
            let started = Instant::now();
            let values = self.embed_chunked(
                &embedding_inputs
                    .iter()
                    .map(|input| input.text.clone())
                    .collect::<Vec<_>>(),
            )?;
            self.attach_embeddings(&mut fingerprint, &embedding_inputs, values)?;
            timings.semantic_micros = started.elapsed().as_secs_f64() * 1_000_000.0;
        }
        timings.total_micros = total_started.elapsed().as_secs_f64() * 1_000_000.0;
        Ok((fingerprint, timings))
    }

    pub fn analyze_batch(
        &self,
        texts: &[String],
    ) -> Result<Vec<MessageFingerprint>, TextIntelError> {
        if texts.len() > self.config.max_batch_size {
            return Err(TextIntelError::InvalidConfiguration(format!(
                "batch size {} exceeds max_batch_size={}",
                texts.len(),
                self.config.max_batch_size
            )));
        }
        let mut bases = Vec::with_capacity(texts.len());
        let mut all_embedding_inputs = Vec::new();
        for text in texts {
            let (fingerprint, embedding_texts) = self.analyze_base(text)?;
            all_embedding_inputs.extend(embedding_texts);
            bases.push((fingerprint, all_embedding_inputs.len()));
        }
        if !self.config.semantic {
            return Ok(bases
                .into_iter()
                .map(|(fingerprint, _)| fingerprint)
                .collect());
        }
        let all_embedding_texts = all_embedding_inputs
            .iter()
            .map(|input| input.text.clone())
            .collect::<Vec<_>>();
        let values = self.embed_chunked(&all_embedding_texts)?;
        if values.len() != all_embedding_texts.len() {
            return Err(TextIntelError::Provider(ProviderError::new(
                "embedding",
                format!(
                    "returned {} vectors for {} batched inputs",
                    values.len(),
                    all_embedding_texts.len()
                ),
            )));
        }
        let mut output = Vec::with_capacity(bases.len());
        let mut offset = 0usize;
        for (mut fingerprint, end) in bases {
            let count = end.saturating_sub(offset);
            let inputs = &all_embedding_inputs[offset..end];
            let vectors = values[offset..end].to_vec();
            self.attach_embeddings(&mut fingerprint, inputs, vectors)?;
            output.push(fingerprint);
            offset += count;
        }
        Ok(output)
    }

    pub fn decode(&self, text: &str) -> Result<Vec<DecodedCandidate>, TextIntelError> {
        self.decode_with_languages(text, None, None)
    }

    pub fn decode_with_languages(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
    ) -> Result<Vec<DecodedCandidate>, TextIntelError> {
        self.check_length(text)?;
        let decoder = RebusDecoder::new(self.config.clone());
        // Semantic rescoring only when an embedding backend is configured;
        // the default null backend yields no vectors and stays free.
        let semantic;
        let semantic_ref: Option<&crate::rebus::SemanticEvidence> = if self.config.semantic {
            let provider = self.embedding_provider.clone();
            semantic = move |surface: &str, source: &str| -> Option<f64> {
                let vectors = provider
                    .embed(&[surface.to_string(), source.to_string()])
                    .ok()?;
                let (left, right) = (vectors.first()?, vectors.get(1)?);
                if left.is_empty() || right.is_empty() {
                    return None;
                }
                Some(crate::semantic::similarity::cosine(left, right))
            };
            Some(&semantic)
        } else {
            None
        };
        // Explicit languages win; configured hints fill in when the caller
        // passes none; otherwise the decoder runs language-neutral.
        let effective = languages.or(if self.config.language_hints.is_empty() {
            None
        } else {
            Some(self.config.language_hints.as_slice())
        });
        let marker = self.rebus_semantic_marker();
        if let Some(hit) = self.rebus_cache_lookup(text, effective, max_candidates, &marker) {
            return Ok(hit);
        }
        let decoded = decoder.decode_with_abbreviations(
            text,
            effective,
            max_candidates,
            self.symbol_provider.as_ref(),
            self.lexicon_provider.as_ref(),
            self.g2p_provider.as_ref(),
            semantic_ref,
            self.abbreviation_provider.as_deref(),
        );
        self.rebus_cache_store(text, effective, max_candidates, &marker, decoded.clone());
        Ok(decoded)
    }
}
