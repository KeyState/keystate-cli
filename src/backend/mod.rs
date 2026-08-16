//! Backend selection and concrete per-backend extraction.
//!
//! Dispatch is a concrete `match` over the [`Backend`] enum — deliberately
//! *not* trait objects, because core's [`Extractor`] port is not object-safe
//! (see `keystate-core`'s `ARCHITECTURE.md` §2.1). A new backend adds a
//! variant here, a module like [`keycloak`], and a match arm — nothing else.

pub mod keycloak;

use keystate_core::{CanonicalRealm, VerificationReport};

use crate::error::{Error, Result};
use crate::progress::Progress;

/// The backends this CLI can extract from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Keycloak, via `keystate-adapter-keycloak`.
    Keycloak,
}

impl Backend {
    /// Parse a backend name from CLI/config input; defaults to Keycloak.
    pub fn parse(name: Option<&str>) -> Result<Self> {
        match name.unwrap_or("keycloak") {
            "keycloak" => Ok(Backend::Keycloak),
            other => Err(Error::UnknownBackend(other.to_string())),
        }
    }

    /// The stable, URL-safe identifier used in output paths.
    pub fn name(self) -> &'static str {
        match self {
            Backend::Keycloak => "keycloak",
        }
    }
}

/// Extract and verify one realm from the selected backend.
///
/// Monomorphized per backend via the concrete `match`; the returned future is
/// `Send` (core's `Extractor` contract), so the CLI drives it on tokio.
pub async fn extract(
    backend: Backend,
    realm: &str,
    db_url: &str,
    progress: &Progress,
) -> Result<(CanonicalRealm, VerificationReport)> {
    match backend {
        Backend::Keycloak => keycloak::extract(realm, db_url, progress).await,
    }
}
