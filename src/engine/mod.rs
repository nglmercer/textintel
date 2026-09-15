pub mod analyzer;
pub mod production;

pub use analyzer::TextIntelligence;
pub use production::{
    preferred_similarity_artifact, preferred_similarity_artifact_in, DegradedCapability,
    EngineBuilder, EngineDiagnostics,
};
