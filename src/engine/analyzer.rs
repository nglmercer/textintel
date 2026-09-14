use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

use crate::comparison::model::{score_fingerprints_with_profile, SimilarityProfile};
use crate::comparison::scorer::score_fingerprints as weighted_score_fingerprints;
use crate::core::capabilities::ProviderCapabilities;
use crate::core::config::EngineConfig;
use crate::core::error::{ProviderError, TextIntelError};
use crate::core::providers::{
    EmbeddingProvider, G2PProvider, LanguageDetectionProvider, LemmatizerProvider, LexiconProvider,
    RerankerProvider, SimilarityScorer, SpamPredictor, SymbolKnowledgeProvider, VectorStore,
};
use crate::core::types::{
    ChannelAvailability, ComparisonResult, DecodedCandidate, DuplicateMode, DuplicateResult,
    LexicalFeatures, MessageFingerprint, Pattern, PatternMatch, PhoneticCandidate, SearchResult,
    SpokenCandidate, Transformation,
};
use crate::detection::duplicates::{duplicate_result, duplicate_result_with_mode};
use crate::detection::patterns::match_pattern_fingerprint;
use crate::detection::spam::HeuristicSpamPredictor;
use crate::language::segmentation::segment_message_with_provider;
use crate::language::NgramLanguageDetector;
use crate::lexical::character::char_features;
use crate::lexical::minhash::{minhash_signature, simhash};
use crate::lexical::ngrams::word_ngrams;
use crate::lexical::tokenizer::{simple_lemmas_with_provider, stop_words_with_provider, tokenize};
use crate::normalization::leetspeak::apply_leet;
use crate::normalization::repetition::collapse_repetition;
use crate::normalization::unicode::{casefold_text, nfkc};
use crate::normalization::whitespace::normalize_whitespace;
use crate::obfuscation::features::obfuscation_features;
use crate::phonetic::g2p::RuleBasedG2PProvider;
use crate::rebus::decoder::RebusDecoder;
use crate::resources::ResourceLoader;
use crate::semantic::embeddings::NullEmbeddingProvider;
use crate::storage::{JsonFileStore, MemoryStore};
use crate::symbols::resolver::resolve_symbols_with_provider;
use crate::visual::unicode_features::analyze_unicode;

struct RegisteredPattern {
    pattern: Pattern,
    examples: Vec<(String, MessageFingerprint)>,
}

#[derive(Debug, Clone)]
struct EmbeddingInput {
    key: String,
    text: String,
}

/// Main high-level API.  The default instance is local-only and model-free;
/// optional providers can be injected through the `with_*` methods.
pub struct TextIntelligence {
    config: EngineConfig,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    g2p_provider: Arc<dyn G2PProvider>,
    language_provider: Arc<dyn LanguageDetectionProvider>,
    lexicon_provider: Arc<dyn LexiconProvider>,
    lemmatizer_provider: Option<Arc<dyn LemmatizerProvider>>,
    symbol_provider: Arc<dyn SymbolKnowledgeProvider>,
    reranker_provider: Option<Arc<dyn RerankerProvider>>,
    spam_predictor: Arc<dyn SpamPredictor>,
    similarity_scorer: Option<Arc<dyn SimilarityScorer>>,
    similarity_profile: Option<SimilarityProfile>,
    store: RwLock<Box<dyn VectorStore>>,
    patterns: RwLock<BTreeMap<String, RegisteredPattern>>,
}

impl Default for TextIntelligence {
    fn default() -> Self {
        Self::new(EngineConfig::default())
    }
}

impl TextIntelligence {
    /// Construct an engine.  Invalid limits or weights are programmer errors;
    /// use [`Self::try_new`] when configuration comes from an untrusted file.
    pub fn new(config: EngineConfig) -> Self {
        Self::try_new(config).expect("invalid TextIntelligence configuration")
    }

    pub fn try_new(config: EngineConfig) -> Result<Self, TextIntelError> {
        config
            .validate()
            .map_err(TextIntelError::InvalidConfiguration)?;
        let resources = Arc::new(
            ResourceLoader::common()
                .map_err(|error| TextIntelError::Serialization(error.to_string()))?,
        );
        let language_detector = NgramLanguageDetector::from_resources(&resources);
        Ok(Self {
            config,
            embedding_provider: Arc::new(NullEmbeddingProvider),
            g2p_provider: Arc::new(RuleBasedG2PProvider),
            language_provider: Arc::new(language_detector),
            lexicon_provider: resources.clone(),
            lemmatizer_provider: None,
            symbol_provider: resources,
            reranker_provider: None,
            spam_predictor: Arc::new(HeuristicSpamPredictor),
            similarity_scorer: None,
            similarity_profile: None,
            store: RwLock::new(Box::new(MemoryStore::default())),
            patterns: RwLock::new(BTreeMap::new()),
        })
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Stable diagnostics for deployments. Capability inspection is local and
    /// never performs network or model loading work.
    pub fn provider_capabilities(&self) -> BTreeMap<String, ProviderCapabilities> {
        let mut capabilities = BTreeMap::new();
        capabilities.insert(
            "embedding".to_string(),
            self.embedding_provider.capabilities(),
        );
        capabilities.insert("g2p".to_string(), self.g2p_provider.capabilities());
        capabilities.insert(
            "language".to_string(),
            self.language_provider.capabilities(),
        );
        capabilities.insert("lexicon".to_string(), self.lexicon_provider.capabilities());
        capabilities.insert("symbols".to_string(), self.symbol_provider.capabilities());
        capabilities.insert("spam".to_string(), self.spam_predictor.capabilities());
        capabilities.insert(
            "store".to_string(),
            self.store
                .read()
                .map(|store| store.capabilities())
                .unwrap_or_else(|_| ProviderCapabilities::new("store:unavailable")),
        );
        if let Some(reranker) = &self.reranker_provider {
            capabilities.insert("reranker".to_string(), reranker.capabilities());
        }
        if let Some(scorer) = &self.similarity_scorer {
            capabilities.insert("similarity".to_string(), scorer.capabilities());
        }
        capabilities
    }

    pub fn health_check(&self) -> Result<(), TextIntelError> {
        self.embedding_provider
            .health_check()
            .map_err(TextIntelError::from)
    }

    pub fn with_embedding_provider<P: EmbeddingProvider + 'static>(mut self, provider: P) -> Self {
        self.embedding_provider = Arc::new(provider);
        self
    }

    pub fn with_g2p_provider<P: G2PProvider + 'static>(mut self, provider: P) -> Self {
        self.g2p_provider = Arc::new(provider);
        self
    }

    pub fn with_language_provider<P: LanguageDetectionProvider + 'static>(
        mut self,
        provider: P,
    ) -> Self {
        self.language_provider = Arc::new(provider);
        self
    }

    pub fn with_lexicon_provider<P: LexiconProvider + 'static>(mut self, provider: P) -> Self {
        self.lexicon_provider = Arc::new(provider);
        self
    }

    /// Replace the language, lexicon, and symbol indexes with one coherent
    /// resource set. This is the normal entry point for application packs.
    pub fn with_resources(mut self, resources: ResourceLoader) -> Self {
        let resources = Arc::new(resources);
        self.language_provider = Arc::new(NgramLanguageDetector::from_resources(&resources));
        self.lexicon_provider = resources.clone();
        self.symbol_provider = resources;
        self
    }

    pub fn with_lemmatizer_provider<P: LemmatizerProvider + 'static>(
        mut self,
        provider: P,
    ) -> Self {
        self.lemmatizer_provider = Some(Arc::new(provider));
        self
    }

    pub fn with_symbol_knowledge_provider<P: SymbolKnowledgeProvider + 'static>(
        mut self,
        provider: P,
    ) -> Self {
        self.symbol_provider = Arc::new(provider);
        self
    }

    pub fn with_reranker_provider<P: RerankerProvider + 'static>(mut self, provider: P) -> Self {
        self.reranker_provider = Some(Arc::new(provider));
        self
    }

    pub fn with_spam_predictor<P: SpamPredictor + 'static>(mut self, predictor: P) -> Self {
        self.spam_predictor = Arc::new(predictor);
        self
    }

    pub fn with_similarity_scorer<P: SimilarityScorer + 'static>(mut self, scorer: P) -> Self {
        self.similarity_scorer = Some(Arc::new(scorer));
        self
    }

    pub fn with_similarity_profile(mut self, profile: SimilarityProfile) -> Self {
        self.similarity_profile = Some(profile);
        self
    }

    fn score_pair(
        &self,
        left: &MessageFingerprint,
        right: &MessageFingerprint,
    ) -> ComparisonResult {
        if let Some(scorer) = &self.similarity_scorer {
            scorer.score(left, right)
        } else if let Some(profile) = &self.similarity_profile {
            score_fingerprints_with_profile(left, right, profile)
        } else {
            weighted_score_fingerprints(left, right, &self.config.similarity_weights)
        }
    }

    pub fn with_store<S: VectorStore + 'static>(mut self, store: S) -> Self {
        self.store = RwLock::new(Box::new(store));
        self
    }

    pub fn with_json_store(
        self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Self, TextIntelError> {
        let store = JsonFileStore::open(path).map_err(TextIntelError::Storage)?;
        Ok(self.with_store(store))
    }

    fn check_length(&self, text: &str) -> Result<(), TextIntelError> {
        let length = text.chars().count();
        if length > self.config.max_input_length {
            return Err(TextIntelError::InputTooLong {
                length,
                maximum: self.config.max_input_length,
            });
        }
        Ok(())
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
        let language_list: Vec<String> = languages
            .iter()
            .filter(|candidate| candidate.language != "unknown")
            .take(3)
            .map(|candidate| candidate.language.clone())
            .collect();
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
        self.check_length(text)?;
        let unicode = analyze_unicode(text);
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
        let tokens = tokenize(text);
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
        let symbols = resolve_symbols_with_provider(
            text,
            self.config.max_symbol_readings,
            self.symbol_provider.as_ref(),
        );
        let obfuscation = obfuscation_features(text, &unicode);
        let decoder = RebusDecoder::new(self.config.clone());
        let rebus = decoder.decode_with_all_providers(
            text,
            Some(&language_names),
            Some(self.config.max_candidates),
            self.symbol_provider.as_ref(),
            self.lexicon_provider.as_ref(),
            self.g2p_provider.as_ref(),
        );
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
            .collect();

        let semantic_embeddings = BTreeMap::new();
        let phonetic_candidates = self.build_phonetic_candidates(text, &languages, &rebus)?;
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
        let transformations = normalization_views
            .iter()
            .filter(|(name, value)| value.as_str() != text && name.as_str() != "normalized")
            .map(|(name, value)| Transformation {
                source: text.to_string(),
                replacement: value.clone(),
                transformation_type: format!("normalization:{name}"),
            })
            .collect();
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
            ChannelAvailability::available("bounded_beam_search", 0.7),
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
        metadata.insert("input_length".to_string(), text.chars().count().to_string());
        let embedding_inputs = if self.config.semantic {
            let mut values = vec![EmbeddingInput {
                key: "default".to_string(),
                text: text.to_string(),
            }];
            values.extend(rebus.iter().take(3).enumerate().map(|(index, candidate)| {
                EmbeddingInput {
                    key: format!("decoded:{}", index + 1),
                    text: candidate.text.clone(),
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
                        text: segment.text.clone(),
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
            metadata,
        };
        Ok((fingerprint, embedding_inputs))
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
        Ok(())
    }

    pub fn analyze(&self, text: &str) -> Result<MessageFingerprint, TextIntelError> {
        let (mut fingerprint, embedding_inputs) = self.analyze_base(text)?;
        if self.config.semantic {
            let values = self
                .embedding_provider
                .embed_batch(
                    &embedding_inputs
                        .iter()
                        .map(|input| input.text.clone())
                        .collect::<Vec<_>>(),
                )
                .map_err(TextIntelError::from)?;
            self.attach_embeddings(&mut fingerprint, &embedding_inputs, values)?;
        }
        Ok(fingerprint)
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
        let values = self
            .embedding_provider
            .embed_batch(&all_embedding_texts)
            .map_err(TextIntelError::from)?;
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

    pub fn compare_batch(
        &self,
        pairs: &[(String, String)],
    ) -> Result<Vec<ComparisonResult>, TextIntelError> {
        if pairs.len() > self.config.max_batch_size {
            return Err(TextIntelError::InvalidConfiguration(format!(
                "batch size {} exceeds max_batch_size={}",
                pairs.len(),
                self.config.max_batch_size
            )));
        }
        let texts = pairs
            .iter()
            .flat_map(|(left, right)| [left.clone(), right.clone()])
            .collect::<Vec<_>>();
        let fingerprints = self.analyze_batch(&texts)?;
        pairs
            .iter()
            .enumerate()
            .map(|(index, _)| {
                Ok(self.score_pair(&fingerprints[index * 2], &fingerprints[index * 2 + 1]))
            })
            .collect()
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
        Ok(decoder.decode_with_all_providers(
            text,
            languages,
            max_candidates,
            self.symbol_provider.as_ref(),
            self.lexicon_provider.as_ref(),
            self.g2p_provider.as_ref(),
        ))
    }

    pub fn compare(&self, left: &str, right: &str) -> Result<ComparisonResult, TextIntelError> {
        let left = self.analyze(left)?;
        let right = self.analyze(right)?;
        Ok(self.score_pair(&left, &right))
    }

    pub fn compare_fingerprints(
        &self,
        left: &MessageFingerprint,
        right: &MessageFingerprint,
    ) -> ComparisonResult {
        self.score_pair(left, right)
    }

    pub fn add_pattern(
        &self,
        id: impl Into<String>,
        examples: Vec<String>,
    ) -> Result<(), TextIntelError> {
        let id = id.into();
        self.add_pattern_with_options(Pattern {
            id,
            examples,
            negative_examples: Vec::new(),
            threshold: 0.75,
            languages: Vec::new(),
            tags: Vec::new(),
            enabled_channels: Vec::new(),
        })
    }

    pub fn add_pattern_with_options(&self, pattern: Pattern) -> Result<(), TextIntelError> {
        if pattern.id.trim().is_empty() || pattern.examples.is_empty() {
            return Err(TextIntelError::InvalidConfiguration(
                "pattern id and examples are required".to_string(),
            ));
        }
        if !pattern.threshold.is_finite() || !(0.0..=1.0).contains(&pattern.threshold) {
            return Err(TextIntelError::InvalidConfiguration(
                "pattern threshold must be between 0 and 1".to_string(),
            ));
        }
        let id = pattern.id.clone();
        let examples = pattern.examples.clone();
        let mut analyzed = Vec::with_capacity(examples.len());
        for example in &examples {
            analyzed.push((example.clone(), self.analyze(example)?));
        }
        let pattern = RegisteredPattern {
            pattern,
            examples: analyzed,
        };
        self.patterns
            .write()
            .map_err(|_| TextIntelError::Storage("pattern lock poisoned".to_string()))?
            .insert(id, pattern);
        Ok(())
    }

    pub fn remove_pattern(&self, id: &str) -> Result<bool, TextIntelError> {
        Ok(self
            .patterns
            .write()
            .map_err(|_| TextIntelError::Storage("pattern lock poisoned".to_string()))?
            .remove(id)
            .is_some())
    }

    pub fn match_patterns(&self, text: &str) -> Result<Vec<PatternMatch>, TextIntelError> {
        let query = self.analyze(text)?;
        let patterns = self
            .patterns
            .read()
            .map_err(|_| TextIntelError::Storage("pattern lock poisoned".to_string()))?;
        let mut matches = patterns
            .values()
            .filter_map(|registered| {
                match_pattern_fingerprint(
                    &query,
                    &registered.pattern,
                    &registered.examples,
                    &self.config.similarity_weights,
                )
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| right.score.total_cmp(&left.score));
        Ok(matches)
    }

    pub fn detect_spam(
        &self,
        text: &str,
    ) -> Result<crate::core::types::SpamResult, TextIntelError> {
        let fingerprint = self.analyze(text)?;
        let patterns = self.match_patterns(text)?;
        self.spam_predictor
            .predict(&fingerprint, &patterns)
            .map_err(TextIntelError::from)
    }

    pub fn duplicate(
        &self,
        left: &str,
        right: &str,
        threshold: f64,
    ) -> Result<DuplicateResult, TextIntelError> {
        let left = self.analyze(left)?;
        let right = self.analyze(right)?;
        Ok(duplicate_result(
            &left,
            &right,
            threshold.clamp(0.0, 1.0),
            &self.config.similarity_weights,
        ))
    }

    pub fn duplicate_with_mode(
        &self,
        left: &str,
        right: &str,
        threshold: f64,
        mode: DuplicateMode,
    ) -> Result<DuplicateResult, TextIntelError> {
        let left = self.analyze(left)?;
        let right = self.analyze(right)?;
        Ok(duplicate_result_with_mode(
            &left,
            &right,
            threshold.clamp(0.0, 1.0),
            &self.config.similarity_weights,
            mode,
        ))
    }

    pub fn add_document(&self, id: impl Into<String>, text: &str) -> Result<(), TextIntelError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(TextIntelError::InvalidConfiguration(
                "document id cannot be empty".to_string(),
            ));
        }
        let fingerprint = self.analyze(text)?;
        let mut store = self
            .store
            .write()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?;
        if store.len() >= self.config.max_documents
            && store.records().iter().all(|(current, _)| current != &id)
        {
            return Err(TextIntelError::Storage(format!(
                "max_documents={} reached",
                self.config.max_documents
            )));
        }
        store
            .upsert(id, fingerprint)
            .map_err(TextIntelError::Storage)
    }

    pub fn remove_document(&self, id: &str) -> Result<bool, TextIntelError> {
        self.store
            .write()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .remove(id)
            .map_err(TextIntelError::Storage)
    }

    pub fn document_count(&self) -> Result<usize, TextIntelError> {
        Ok(self
            .store
            .read()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .len())
    }

    pub fn find_similar(
        &self,
        text: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, TextIntelError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let query = self.analyze(text)?;
        let retrieved = self
            .store
            .read()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .search_candidates_with_metadata(&query, self.config.max_search_candidates.max(limit))
            .map_err(TextIntelError::Storage)?;
        let retrieval_channels = retrieved.channels;
        let records = retrieved.records;
        let mut candidates = records
            .into_iter()
            .map(|(id, fingerprint)| {
                let comparison = self.score_pair(&query, &fingerprint);
                (id, fingerprint, comparison)
            })
            .collect::<Vec<_>>();
        let candidate_count = candidates.len();
        candidates.sort_by(|left, right| right.2.score.total_cmp(&left.2.score));
        let results = if let Some(reranker) = &self.reranker_provider {
            let input = candidates
                .iter()
                .map(|(id, fingerprint, comparison)| {
                    (id.clone(), fingerprint.clone(), comparison.score)
                })
                .collect();
            let reranked = reranker
                .rerank(&query, input)
                .map_err(TextIntelError::from)?;
            let mut by_id = candidates
                .into_iter()
                .map(|(id, _, comparison)| (id, comparison))
                .collect::<BTreeMap<_, _>>();
            let mut reranked_results: Vec<(String, ComparisonResult)> = reranked
                .into_iter()
                .filter_map(|(id, _, score)| {
                    by_id.remove(&id).map(|mut comparison| {
                        comparison.score = score.clamp(0.0, 1.0);
                        (id, comparison)
                    })
                })
                .collect();
            reranked_results.sort_by(|left, right| right.1.score.total_cmp(&left.1.score));
            reranked_results
        } else {
            candidates
                .into_iter()
                .map(|(id, _, comparison)| (id, comparison))
                .collect()
        };
        Ok(results
            .into_iter()
            .take(limit)
            .map(|(id, comparison)| SearchResult {
                id,
                score: comparison.score,
                comparison,
                candidate_count,
                retrieval_channels: retrieval_channels.clone(),
            })
            .collect())
    }

    pub fn find_duplicates(
        &self,
        text: &str,
        threshold: f64,
    ) -> Result<Vec<(String, DuplicateResult)>, TextIntelError> {
        self.find_duplicates_with_mode(text, threshold, DuplicateMode::Combined)
    }

    pub fn find_duplicates_with_mode(
        &self,
        text: &str,
        threshold: f64,
        mode: DuplicateMode,
    ) -> Result<Vec<(String, DuplicateResult)>, TextIntelError> {
        let query = self.analyze(text)?;
        let records = self
            .store
            .read()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .search_candidates(&query, self.config.max_search_candidates)
            .map_err(TextIntelError::Storage)?;
        Ok(records
            .into_iter()
            .map(|(id, fingerprint)| {
                (
                    id,
                    duplicate_result_with_mode(
                        &query,
                        &fingerprint,
                        threshold.clamp(0.0, 1.0),
                        &self.config.similarity_weights,
                        mode,
                    ),
                )
            })
            .filter(|(_, result)| result.duplicate)
            .collect())
    }
}
