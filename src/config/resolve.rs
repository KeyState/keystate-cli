//! Precedence resolution: defaults < config file < KEYSTATE_DB_URL < flag < prompt.
//!
//! This module is the single place that decides what a run actually does —
//! command-line flags, config-file values, environment variables, and the
//! interactive prompt are combined *here*, and nowhere else. It is fully
//! unit-testable: the environment and the prompt are injected behind the
//! [`CredentialSource`] trait rather than read from the process directly.

use std::path::PathBuf;

use super::{Config, ExtractSection};
use crate::backend::Backend;
use crate::cli::extract::ExtractArgs;
use crate::error::{Error, Result};
use crate::progress::OutputFormat;

/// The name of the environment variable that holds the backend database URL.
pub const ENV_DB_URL: &str = "KEYSTATE_DB_URL";

/// Fully resolved options for one extraction run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractOptions {
    /// The backend to extract from.
    pub backend: Backend,
    /// The realm to extract.
    pub realm: String,
    /// The resolved database URL.
    pub db_url: String,
    /// The output root directory.
    pub output_dir: PathBuf,
    /// Run the pipeline without writing, reporting drift.
    pub check: bool,
    /// Suppress human-readable progress output.
    pub quiet: bool,
    /// Reporting format for progress and status.
    pub output_format: OutputFormat,
}

/// Where a resolved credential came from — used to warn on discouraged paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialOrigin {
    /// `KEYSTATE_DB_URL`.
    Env,
    /// A plaintext `db_url` in the config file.
    ConfigPlain,
    /// A `db_url` in the config file that references `${{VAR}}`.
    ConfigInterpolated,
    /// The `--db-url` flag.
    Flag,
    /// The interactive prompt.
    Prompt,
}

impl CredentialOrigin {
    /// Whether this path is the discouraged plaintext one.
    pub fn is_discouraged(self) -> bool {
        matches!(self, CredentialOrigin::ConfigPlain | CredentialOrigin::Flag)
    }
}

/// Where the process can obtain credentials: the environment and an
/// interactive prompt.
///
/// Abstracted so precedence logic can be tested without touching the real
/// process environment, stdin, or a terminal.
pub trait CredentialSource {
    /// Look up an environment variable.
    fn env(&self, name: &str) -> Option<String>;
    /// Prompt the user without echoing input.
    fn prompt(&self, message: &str) -> std::io::Result<String>;
    /// Whether stdin is a terminal (the prompt only runs on a TTY).
    fn stdin_is_tty(&self) -> bool;
}

/// The real process environment and terminal.
pub struct System;

impl CredentialSource for System {
    fn env(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn prompt(&self, message: &str) -> std::io::Result<String> {
        rpassword::prompt_password(message)
    }

    fn stdin_is_tty(&self) -> bool {
        use std::io::IsTerminal;
        std::io::stdin().is_terminal()
    }
}

/// Combine command-line args, an optional config file, and the environment
/// into the effective options for one run.
pub fn resolve(
    args: &ExtractArgs,
    config: Option<&Config>,
    creds: &impl CredentialSource,
) -> Result<ExtractOptions> {
    let default_config = Config::default();
    let cfg = config.unwrap_or(&default_config);

    let backend = Backend::parse(flag_or_config(
        args.backend.as_deref(),
        cfg.backend.name.as_deref(),
    ))?;

    let realm = flag_or_config(args.realm.as_deref(), cfg.extract.realm.as_deref())
        .ok_or_else(|| {
            Error::Message(
                "no realm given: pass --realm or set [extract] realm in the config file".into(),
            )
        })?
        .to_string();

    let output_dir = match &args.output {
        Some(dir) => dir.clone(),
        None => cfg
            .output
            .directory
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./keystate-out")),
    };

    let (db_url, source) = resolve_db_url(args, &cfg.extract, creds)?;
    if source.is_discouraged() {
        eprintln!(
            "warning: passing the database URL via {} is discouraged; prefer {} or the interactive prompt",
            match source {
                CredentialOrigin::Flag => "--db-url",
                _ => "a plaintext db_url in the config file",
            },
            ENV_DB_URL
        );
    }

    Ok(ExtractOptions {
        backend,
        realm,
        db_url,
        output_dir,
        check: args.check,
        quiet: args.quiet,
        output_format: args.output_format,
    })
}

/// `db_url` precedence: config < `KEYSTATE_DB_URL` < `--db-url` < prompt.
fn resolve_db_url(
    args: &ExtractArgs,
    cfg: &ExtractSection,
    creds: &impl CredentialSource,
) -> Result<(String, CredentialOrigin)> {
    if let Some(url) = &args.db_url {
        return Ok((url.clone(), CredentialOrigin::Flag));
    }
    if let Some(url) = creds.env(ENV_DB_URL) {
        return Ok((url, CredentialOrigin::Env));
    }
    if let Some(url) = &cfg.db_url {
        let (interpolated, used_var) = interpolate_env(url, creds)?;
        let source = if used_var {
            CredentialOrigin::ConfigInterpolated
        } else {
            CredentialOrigin::ConfigPlain
        };
        return Ok((interpolated, source));
    }
    if creds.stdin_is_tty() {
        let url = creds.prompt("Database URL (postgres://...): ")?;
        return Ok((url, CredentialOrigin::Prompt));
    }
    Err(Error::MissingCredentials(
        "set KEYSTATE_DB_URL, --db-url, a config db_url, or run with a terminal for the interactive prompt",
    ))
}

/// Resolve every `${NAME}` reference in a config value against the
/// environment. Returns the resolved string and whether any variable was
/// substituted.
fn interpolate_env(value: &str, creds: &impl CredentialSource) -> Result<(String, bool)> {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    let mut substituted = false;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            return Err(Error::Message(format!(
                "unterminated ${{...}} reference in config value `{value}`"
            )));
        };
        let name = &after[..end];
        let replacement = creds
            .env(name)
            .ok_or_else(|| Error::UnsetEnvVariable(name.to_string()))?;
        out.push_str(&replacement);
        substituted = true;
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok((out, substituted))
}

/// A flag overrides a config value; whichever is present wins.
fn flag_or_config<'a>(flag: Option<&'a str>, config: Option<&'a str>) -> Option<&'a str> {
    flag.or(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A scripted credential source for tests: a fixed env map, an optional
    /// prompt answer, and a fixed TTY flag.
    struct Fake {
        env: HashMap<&'static str, String>,
        prompt_answer: Option<String>,
        tty: bool,
    }

    impl CredentialSource for Fake {
        fn env(&self, name: &str) -> Option<String> {
            self.env.get(name).cloned()
        }
        fn prompt(&self, _message: &str) -> std::io::Result<String> {
            self.prompt_answer
                .clone()
                .ok_or_else(|| std::io::Error::other("no prompt in test"))
        }
        fn stdin_is_tty(&self) -> bool {
            self.tty
        }
    }

    fn args() -> ExtractArgs {
        ExtractArgs {
            config: None,
            backend: None,
            realm: None,
            db_url: None,
            output: None,
            check: false,
            quiet: false,
            output_format: OutputFormat::Text,
        }
    }

    fn config() -> Config {
        Config::default()
    }

    #[test]
    fn env_var_wins_over_config_file() {
        let mut args = args();
        args.realm = Some("master".into());
        let mut cfg = config();
        cfg.extract.db_url = Some("postgres://config@localhost/db".into());
        let fake = Fake {
            env: HashMap::from([(ENV_DB_URL, "postgres://env@localhost/db".into())]),
            prompt_answer: None,
            tty: false,
        };
        let resolved = resolve(&args, Some(&cfg), &fake).unwrap();
        assert_eq!(resolved.db_url, "postgres://env@localhost/db");
        assert_eq!(resolved.backend, Backend::Keycloak);
        assert_eq!(resolved.realm, "master");
        assert_eq!(resolved.output_dir, PathBuf::from("./keystate-out"));
    }

    #[test]
    fn flag_wins_over_env_var() {
        let mut args = args();
        args.realm = Some("master".into());
        args.db_url = Some("postgres://flag@localhost/db".into());
        let fake = Fake {
            env: HashMap::from([(ENV_DB_URL, "postgres://env@localhost/db".into())]),
            prompt_answer: None,
            tty: false,
        };
        let resolved = resolve(&args, None, &fake).unwrap();
        assert_eq!(resolved.db_url, "postgres://flag@localhost/db");
    }

    #[test]
    fn prompt_is_last_resort_on_a_tty() {
        let mut args = args();
        args.realm = Some("master".into());
        let fake = Fake {
            env: HashMap::new(),
            prompt_answer: Some("postgres://prompt@localhost/db".into()),
            tty: true,
        };
        let resolved = resolve(&args, None, &fake).unwrap();
        assert_eq!(resolved.db_url, "postgres://prompt@localhost/db");
    }

    #[test]
    fn no_credentials_anywhere_is_a_clear_error() {
        let mut args = args();
        args.realm = Some("master".into());
        let fake = Fake {
            env: HashMap::new(),
            prompt_answer: None,
            tty: false,
        };
        let error = resolve(&args, None, &fake).unwrap_err();
        assert!(
            matches!(error, Error::MissingCredentials(_)),
            "expected MissingCredentials, got {error}"
        );
    }

    #[test]
    fn missing_realm_is_a_clear_error() {
        let fake = Fake {
            env: HashMap::from([(ENV_DB_URL, "postgres://env@localhost/db".into())]),
            prompt_answer: None,
            tty: false,
        };
        let error = resolve(&args(), None, &fake).unwrap_err();
        assert!(error.to_string().contains("no realm given"), "{error}");
    }

    #[test]
    fn unknown_backend_is_rejected() {
        let mut args = args();
        args.backend = Some("ferriskey".into());
        args.realm = Some("master".into());
        let fake = Fake {
            env: HashMap::from([(ENV_DB_URL, "postgres://env@localhost/db".into())]),
            prompt_answer: None,
            tty: false,
        };
        let error = resolve(&args, None, &fake).unwrap_err();
        assert!(matches!(error, Error::UnknownBackend(_)), "{error}");
    }

    #[test]
    fn config_backend_and_realm_are_used() {
        let mut cfg = config();
        cfg.backend.name = Some("keycloak".into());
        cfg.extract.realm = Some("master".into());
        cfg.extract.db_url = Some("postgres://config@localhost/db".into());
        cfg.output.directory = Some("./custom-out".into());
        let fake = Fake {
            env: HashMap::new(),
            prompt_answer: None,
            tty: false,
        };
        let resolved = resolve(&args(), Some(&cfg), &fake).unwrap();
        assert_eq!(resolved.backend, Backend::Keycloak);
        assert_eq!(resolved.realm, "master");
        assert_eq!(resolved.db_url, "postgres://config@localhost/db");
        assert_eq!(resolved.output_dir, PathBuf::from("./custom-out"));
    }

    #[test]
    fn flag_realm_overrides_config_realm() {
        let mut args = args();
        args.realm = Some("flagrealm".into());
        let mut cfg = config();
        cfg.extract.realm = Some("configrealm".into());
        cfg.extract.db_url = Some("postgres://config@localhost/db".into());
        let fake = Fake {
            env: HashMap::new(),
            prompt_answer: None,
            tty: false,
        };
        let resolved = resolve(&args, Some(&cfg), &fake).unwrap();
        assert_eq!(resolved.realm, "flagrealm");
    }

    #[test]
    fn interpolation_resolves_variables() {
        let fake = Fake {
            env: HashMap::from([("DATABASE_URL", "postgres://interp@localhost/db".into())]),
            prompt_answer: None,
            tty: false,
        };
        let (value, substituted) = interpolate_env("${DATABASE_URL}", &fake).unwrap();
        assert_eq!(value, "postgres://interp@localhost/db");
        assert!(substituted);

        let (value, substituted) =
            interpolate_env("postgres://u@h/d?pw=${DATABASE_URL}", &fake).unwrap();
        assert_eq!(value, "postgres://u@h/d?pw=postgres://interp@localhost/db");
        assert!(substituted);
    }

    #[test]
    fn interpolation_without_variables_is_passthrough() {
        let fake = Fake {
            env: HashMap::new(),
            prompt_answer: None,
            tty: false,
        };
        let (value, substituted) = interpolate_env("postgres://plain@localhost/db", &fake).unwrap();
        assert_eq!(value, "postgres://plain@localhost/db");
        assert!(!substituted);
    }

    #[test]
    fn interpolation_of_unset_variable_is_a_clear_error() {
        let fake = Fake {
            env: HashMap::new(),
            prompt_answer: None,
            tty: false,
        };
        let error = interpolate_env("${DATABASE_URL}", &fake).unwrap_err();
        assert!(matches!(error, Error::UnsetEnvVariable(_)), "{error}");
    }

    #[test]
    fn interpolation_of_unterminated_reference_is_a_clear_error() {
        let fake = Fake {
            env: HashMap::new(),
            prompt_answer: None,
            tty: false,
        };
        let error = interpolate_env("${DATABASE_URL", &fake).unwrap_err();
        assert!(error.to_string().contains("unterminated"), "{error}");
    }
}
