//! Fingerprint schema migration.
//!
//! [`migrate_fingerprint_bytes`] is the single entry point for reading
//! stored fingerprints: it inspects the version *before* deserializing,
//! rejects newer schemas outright, migrates supported older ones through
//! explicit per-version steps, and never silently accepts incompatible data.
//!
//! History: fingerprints predating schema 2 carry no `schema_version` field
//! (implicit version 1). Every field added since is `#[serde(default)]`, so
//! the v1→v2 step is an explicit re-stamp after default-filled
//! deserialization — recorded in [`MigratedFingerprint::migrated_from`]
//! rather than hidden by serde.

use crate::core::types::{FINGERPRINT_SCHEMA_VERSION, MessageFingerprint};

const PROVIDER: &str = "fingerprint_migration";

/// Oldest fingerprint schema this build can migrate from.
pub const OLDEST_SUPPORTED_FINGERPRINT_VERSION: u32 = 1;

/// A fingerprint plus its provenance. `migrated_from` is `None` for payloads
/// already at the current schema.
#[derive(Debug, Clone, PartialEq)]
pub struct MigratedFingerprint {
    pub fingerprint: MessageFingerprint,
    pub migrated_from: Option<u32>,
}

/// Read one stored fingerprint, migrating when supported.
///
/// - missing `schema_version` reads as version 1 (pre-versioned payloads);
/// - versions newer than the build are rejected;
/// - versions older than [`OLDEST_SUPPORTED_FINGERPRINT_VERSION`] are
///   rejected as too old to migrate safely.
pub fn migrate_fingerprint_bytes(bytes: &[u8]) -> Result<MigratedFingerprint, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("{PROVIDER}: payload is not JSON: {error}"))?;
    let version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1);
    let version = u32::try_from(version)
        .map_err(|_| format!("{PROVIDER}: schema_version {version} exceeds u32 range"))?;
    if version > FINGERPRINT_SCHEMA_VERSION {
        return Err(format!(
            "{PROVIDER}: schema_version={version} is newer than supported {FINGERPRINT_SCHEMA_VERSION}; refusing to guess"
        ));
    }
    if version < OLDEST_SUPPORTED_FINGERPRINT_VERSION {
        return Err(format!(
            "{PROVIDER}: schema_version={version} is older than oldest supported {OLDEST_SUPPORTED_FINGERPRINT_VERSION}"
        ));
    }
    if version == FINGERPRINT_SCHEMA_VERSION {
        let fingerprint: MessageFingerprint = serde_json::from_value(value)
            .map_err(|error| format!("{PROVIDER}: current-schema payload is invalid: {error}"))?;
        return Ok(MigratedFingerprint {
            fingerprint,
            migrated_from: None,
        });
    }
    migrate_older(value, version)
}

/// Step-wise migration for older payloads. Each arm documents the exact
/// delta it bridges; unknown versions fall through to rejection.
fn migrate_older(value: serde_json::Value, version: u32) -> Result<MigratedFingerprint, String> {
    match version {
        // v1 → v2: every field added since is `#[serde(default)]`, so a
        // default-filled deserialization is the complete step. The version
        // is then stamped explicitly rather than inherited from the default.
        1 => {
            let mut fingerprint: MessageFingerprint =
                serde_json::from_value(value).map_err(|error| {
                    format!("{PROVIDER}: v1 payload does not fit the v2 shape: {error}")
                })?;
            fingerprint.schema_version = FINGERPRINT_SCHEMA_VERSION;
            Ok(MigratedFingerprint {
                fingerprint,
                migrated_from: Some(1),
            })
        }
        other => Err(format!(
            "{PROVIDER}: no migration path from schema_version={other}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineConfig, TextIntelligence};

    fn current_value() -> serde_json::Value {
        let engine = TextIntelligence::new(EngineConfig::default());
        let fingerprint = engine.analyze("compra ahora").unwrap();
        serde_json::to_value(&fingerprint).unwrap()
    }

    #[test]
    fn current_schema_passes_through_unmigrated() {
        let bytes = serde_json::to_vec(&current_value()).unwrap();
        let migrated = migrate_fingerprint_bytes(&bytes).unwrap();
        assert_eq!(migrated.migrated_from, None);
        assert_eq!(
            migrated.fingerprint.schema_version,
            FINGERPRINT_SCHEMA_VERSION
        );
    }

    #[test]
    fn missing_version_migrates_from_v1() {
        let mut value = current_value();
        value.as_object_mut().unwrap().remove("schema_version");
        // Also drop a defaulted v2 field to prove defaults fill the gap.
        value
            .as_object_mut()
            .unwrap()
            .remove("channel_availability");
        let bytes = serde_json::to_vec(&value).unwrap();
        let migrated = migrate_fingerprint_bytes(&bytes).unwrap();
        assert_eq!(migrated.migrated_from, Some(1));
        assert_eq!(
            migrated.fingerprint.schema_version,
            FINGERPRINT_SCHEMA_VERSION
        );
        assert!(migrated.fingerprint.channel_availability.is_empty());
    }

    #[test]
    fn newer_and_ancient_versions_are_rejected() {
        let mut value = current_value();
        value["schema_version"] = serde_json::json!(FINGERPRINT_SCHEMA_VERSION + 1);
        let error = migrate_fingerprint_bytes(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(error.contains("newer than supported"));

        value["schema_version"] = serde_json::json!(0);
        let error = migrate_fingerprint_bytes(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(error.contains("older than oldest supported"));

        assert!(migrate_fingerprint_bytes(b"not json").is_err());
    }
}
