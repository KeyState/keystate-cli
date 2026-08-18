//! Unified CLI error type.
//!
//! Every failure path in the CLI funnels into this one type so that
//! `main` can map it to a single, documented exit code (1) and render it
//! consistently — and so that error construction stays free of I/O noise
//! that would obscure the actual cause.

use std::path::PathBuf;

/// Errors surfaced by the CLI. Mapped to exit code 1 by `main`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A config file could not be read from disk.
    #[error("config file `{path}`: {source}")]
    ConfigIo {
        /// Path to the config file.
        path: PathBuf,
        /// The underlying read error.
        #[source]
        source: std::io::Error,
    },
    /// A config file failed to parse as TOML.
    #[error("config file `{path}`: {source}")]
    ConfigParse {
        /// Path to the config file.
        path: PathBuf,
        /// The underlying TOML parse error.
        #[source]
        source: toml::de::Error,
    },
    /// A generic I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// A failure from the `keystate-core` layer.
    #[error("keystate-core: {0}")]
    Core(#[from] keystate_core::Error),
    /// A failure from a backend adapter.
    #[error("keycloak adapter: {0}")]
    Adapter(#[from] keystate_adapter_keycloak::Error),
    /// The backend named on the command line or in config is not known.
    #[error("unknown backend `{0}` — supported backends: keycloak")]
    UnknownBackend(String),
    /// The detected Keycloak version has no manifest in the adapter.
    #[error("unsupported Keycloak version {0}")]
    UnsupportedVersion(keystate_core::Version),
    /// A realm name is not safe to use as a filesystem path component.
    #[error("realm `{realm}` is not a valid output path component: {reason}")]
    InvalidRealm {
        /// The realm name as given.
        realm: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// No credentials were available anywhere in the precedence chain.
    #[error("missing database credentials: {0}")]
    MissingCredentials(&'static str),
    /// A config value references an environment variable that is not set.
    #[error("config value `${{{0}}}` references an environment variable that is not set")]
    UnsetEnvVariable(String),
    /// Any other failure, with a plain message.
    #[error("{0}")]
    Message(String),
}

/// Convenience alias for the CLI's result type.
pub type Result<T> = std::result::Result<T, Error>;
