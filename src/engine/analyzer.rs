use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

use crate::comparison::scorer::score_fingerprints;
use crate::core::config::EngineConfig;
use crate::core::error::{ProviderError, TextIntelError};
use crate::core::providers::{
    EmbeddingProvider, G2PProvider, LanguageDetectionProvider, LemmatizerProvider,
    RerankerProvider, SymbolKnowledgeProvider, VectorStore,
};
use crate::core::types::{
    ComparisonResult, DecodedCandidate, DuplicateResult, LexicalFeatures, MessageFingerprint,
    Pattern, PatternMatch, PhoneticCandidate, SearchResult, SpokenCandidate,
};
use crate::detection::duplicates::duplicate_result;
use crate::detection::patterns::match_pattern_fingerprint;
use crate::detection::spam::predict_spam;
use crate::language::detector::DefaultLanguageDetector;
use crate::language::segmentation::segment_message;
use crate::lexical::character::char_features;
use crate::lexical::minhash::{minhash_signature, simhash};
use crate::lexical::ngrams::word_ngrams;
use crate::lexical::tokenizer::{simple_lemmas, stop_words, tokenize};
use crate::normalization::leetspeak::apply_leet;
use crate::normalization::repetition::collapse_repetition;
use crate::normalization::unicode::{casefold_text, nfkc};
use crate::normalization::whitespace::normalize_whitespace;
use crate::obfuscation::features::obfuscation_features;
use crate::phonetic::g2p::RuleBasedG2PProvider;
use crate::rebus::decoder::RebusDecoder;
use crate::semantic::embeddings::NullEmbeddingProvider;
use crate::storage::memory::MemoryStore;
use crate::symbols::knowledge::DefaultSymbolKnowledge;
use crate::symbols::resolver::resolve_symbols_with_provider;
use crate::visual::unicode_features::analyze_unicode;

struct RegisteredPattern {
    pattern: Pattern,
    examples: Vec<(String, MessageFingerprint)>,
}

/// Main high-level API.  The default instance is local-only and model-free;
/// optional providers can be injected through the `with_*` methods.
pub struct TextIntelligence {
    config: EngineConfig,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    g2p_provider: Arc<dyn G2PProvider>,
    language_provider: Arc<dyn LanguageDetectionProvider>,
    lemmatizer_provider: Option<Arc<dyn LemmatizerProvider>>,
    symbol_provider: Arc<dyn SymbolKnowledgeProvider>,
    reranker_provider: Option<Arc<dyn RerankerProvider>>,
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
        Ok(Self {
            config,
            embedding_provider: Arc::new(NullEmbeddingProvider),
            g2p_provider: Arc::new(RuleBasedG2PProvider),
            language_provider: Arc::new(DefaultLanguageDetector),
            lemmatizer_provider: None,
            symbol_provider: Arc::new(DefaultSymbolKnowledge),
            reranker_provider: None,
            store: RwLock::new(Box::new(MemoryStore::default())),
            patterns: RwLock::new(BTreeMap::new()),
        })
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
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

    pub fn with_store<S: VectorStore + 'static>(mut self, store: S) -> Self {
        self.store = RwLock::new(Box::new(store));
        self
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
            let candidate = self
                .g2p_provider
                .phonemize(raw, language)
                .map_err(TextIntelError::from)?;
            if seen.insert((candidate.source.clone(), candidate.language.clone())) {
                values.push(candidate);
            }
            for decoded in rebus.iter().take(3) {
                let candidate = self
                    .g2p_provider
                    .phonemize(&decoded.text, language)
                    .map_err(TextIntelError::from)?;
                if seen.insert((candidate.source.clone(), candidate.language.clone())) {
                    values.push(candidate);
                }
            }
        }
        Ok(values)
    }

    pub fn analyze(&self, text: &str) -> Result<MessageFingerprint, TextIntelError> {
        self.check_length(text)?;
        let unicode = analyze_unicode(text);
        let tokens = tokenize(text);
        let lemmas = match &self.lemmatizer_provider {
            Some(provider) => provider
                .lemmatize(&tokens, None)
                .map_err(TextIntelError::from)?,
            None => simple_lemmas(&tokens),
        };
        let lexical = LexicalFeatures {
            tokens: tokens.clone(),
            lemmas: lemmas.clone(),
            stop_words: stop_words(&tokens),
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
        let segments = segment_message(text, self.config.max_segments);
        let languages = self.detect(text)?;
        let obfuscation = obfuscation_features(text, &unicode);
        let normalized = normalize_whitespace(&collapse_repetition(
            &apply_leet(&casefold_text(&nfkc(text))),
            1,
        ));
        let language_names: Vec<String> = languages
            .iter()
            .map(|candidate| candidate.language.clone())
            .collect();
        let decoder = RebusDecoder::new(self.config.clone());
        let rebus = decoder.decode_with_provider(
            text,
            Some(&language_names),
            Some(self.config.max_candidates),
            self.symbol_provider.as_ref(),
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

        let mut semantic_embeddings = BTreeMap::new();
        if self.config.semantic {
            let mut embedding_texts = vec![text.to_string()];
            embedding_texts.extend(rebus.iter().take(3).map(|candidate| candidate.text.clone()));
            let values = self
                .embedding_provider
                .embed(&embedding_texts)
                .map_err(TextIntelError::from)?;
            if values.len() != embedding_texts.len() {
                return Err(TextIntelError::Provider(ProviderError::new(
                    "embedding",
                    format!(
                        "returned {} vectors for {} inputs",
                        values.len(),
                        embedding_texts.len()
                    ),
                )));
            }
            for (index, vector) in values.into_iter().enumerate() {
                if !vector.is_empty() {
                    let key = if index == 0 {
                        "default".to_string()
                    } else {
                        format!("decoded:{index}")
                    };
                    semantic_embeddings.insert(key, vector);
                }
            }
        }
        let phonetic_candidates = self.build_phonetic_candidates(text, &languages, &rebus)?;
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "semantic_enabled".to_string(),
            self.config.semantic.to_string(),
        );
        metadata.insert(
            "phonetic_enabled".to_string(),
            self.config.phonetic.to_string(),
        );
        metadata.insert("input_length".to_string(), text.chars().count().to_string());
        Ok(MessageFingerprint {
            raw: text.to_string(),
            normalized: Some(normalized),
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
            metadata,
        })
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
        Ok(decoder.decode_with_provider(
            text,
            languages,
            max_candidates,
            self.symbol_provider.as_ref(),
        ))
    }

    pub fn compare(&self, left: &str, right: &str) -> Result<ComparisonResult, TextIntelError> {
        let left = self.analyze(left)?;
        let right = self.analyze(right)?;
        Ok(score_fingerprints(
            &left,
            &right,
            &self.config.similarity_weights,
        ))
    }

    pub fn compare_fingerprints(
        &self,
        left: &MessageFingerprint,
        right: &MessageFingerprint,
    ) -> ComparisonResult {
        score_fingerprints(left, right, &self.config.similarity_weights)
    }

    pub fn add_pattern(
        &self,
        id: impl Into<String>,
        examples: Vec<String>,
    ) -> Result<(), TextIntelError> {
        let id = id.into();
        if id.trim().is_empty() || examples.is_empty() {
            return Err(TextIntelError::InvalidConfiguration(
                "pattern id and examples are required".to_string(),
            ));
        }
        let mut analyzed = Vec::with_capacity(examples.len());
        for example in &examples {
            analyzed.push((example.clone(), self.analyze(example)?));
        }
        let pattern = RegisteredPattern {
            pattern: Pattern {
                id: id.clone(),
                examples,
            },
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
        Ok(predict_spam(&fingerprint, &patterns))
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
        let records = self
            .store
            .read()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .records();
        let mut candidates = records
            .into_iter()
            .map(|(id, fingerprint)| {
                let comparison =
                    score_fingerprints(&query, &fingerprint, &self.config.similarity_weights);
                (id, fingerprint, comparison)
            })
            .collect::<Vec<_>>();
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
            })
            .collect())
    }

    pub fn find_duplicates(
        &self,
        text: &str,
        threshold: f64,
    ) -> Result<Vec<(String, DuplicateResult)>, TextIntelError> {
        let query = self.analyze(text)?;
        let records = self
            .store
            .read()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .records();
        Ok(records
            .into_iter()
            .map(|(id, fingerprint)| {
                (
                    id,
                    duplicate_result(
                        &query,
                        &fingerprint,
                        threshold.clamp(0.0, 1.0),
                        &self.config.similarity_weights,
                    ),
                )
            })
            .filter(|(_, result)| result.duplicate)
            .collect())
    }
}
