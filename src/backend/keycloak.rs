//! Keycloak extraction: driving the adapter through the core pipeline.
//!
//! Builds the adapter's extractor, runs `detect` → `extract`, binds the
//! version-specific manifest, verifies completeness, and renders the
//! importable realm-export artifact. Everything Keycloak-specific lives here
//! behind [`super::extract`].

use keystate_adapter_keycloak::{KeycloakExtractor, realm_export};
use keystate_core::{
    CanonicalRealm, ExtractScope, Extractor, ManifestVerifier, VerificationReport,
};

use crate::error::{Error, Result};
use crate::progress::Progress;

/// Detect, extract, verify, and render one Keycloak realm.
pub async fn extract(
    realm: &str,
    db_url: &str,
    progress: &Progress,
) -> Result<(CanonicalRealm, VerificationReport, serde_json::Value)> {
    let extractor = KeycloakExtractor::new(db_url)?;

    let info = extractor.detect().await?;
    progress.detecting(&info);

    progress.extracting(realm);
    let canonical = extractor.extract(&ExtractScope::new(realm)).await?;

    let manifest = keystate_adapter_keycloak::keycloak_manifest(&info.detected_version)
        .ok_or_else(|| Error::UnsupportedVersion(info.detected_version))?;
    let report = ManifestVerifier.verify(&canonical, &manifest)?;
    progress.verifying(&report);

    let export = realm_export(&canonical);
    progress.rendering();

    Ok((canonical, report, export))
}
