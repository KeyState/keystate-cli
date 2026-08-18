//! The command-line surface: argument definitions and usage documentation.
//!
//! This module owns *what* the tool accepts; the `config::resolve` module owns
//! *how* those values combine with config files, environment variables, and
//! prompts. Keeping the two apart means precedence logic is testable without
//! spawning the binary.

pub mod extract;

use clap::{Parser, Subcommand};

use crate::cli::extract::ExtractArgs;

/// Shown after every `--help`. Leading with the usage patterns keeps the
/// fastest path to "I trust this" inside the tool itself.
const AFTER_HELP: &str = "\
USAGE PATTERNS

  # 1. Everything on the command line
  keystate extract --backend keycloak --realm master \
      --db-url postgres://keycloak:keycloak@localhost:5432/keycloak

  # 2. Configuration file (flags override the file)
  keystate extract --config keystate.toml
  keystate extract --config keystate.toml --realm myrealm

  # 3. Environment variable (preferred for credentials)
  KEYSTATE_DB_URL=postgres://keycloak:keycloak@localhost:5432/keycloak \
      keystate extract --backend keycloak --realm master

CREDENTIALS

  Prefer KEYSTATE_DB_URL or the interactive prompt over --db-url / a
  plaintext db_url in the config file: command-line and config-file
  credentials are visible to other users (ps, shell history, git).

  A TOML config file may reference an environment variable without
  embedding the secret:  db_url = \"${DATABASE_URL}\"

  Precedence: config file < KEYSTATE_DB_URL < --db-url < interactive prompt.

EXIT CODES

  0  success
  1  runtime error
  2  usage error
  3  --check detected drift (the output would change)";

/// The top-level command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "keystate",
    version,
    about = "Extract and verify IAM configuration state from backend databases",
    after_help = AFTER_HELP
)]
pub struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The subcommands this tool offers.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Extract one realm's configuration state from a backend and write it out.
    Extract(ExtractArgs),
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn help_documents_all_usage_patterns() {
        let help = Cli::command().render_help().to_string();
        assert!(help.contains("USAGE PATTERNS"));
        assert!(help.contains("KEYSTATE_DB_URL"));
        assert!(help.contains("keystate extract --config keystate.toml"));
        assert!(help.contains("EXIT CODES"));
        assert!(help.contains("3  --check detected drift"));
    }
}
