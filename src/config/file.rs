//! Loading and parsing the TOML configuration file.
//!
//! Just reading a file and deserializing it; all combination with flags and
//! environment happens in [`super::resolve`].

use std::path::Path;

use super::Config;
use crate::{Error, Result};

/// Read and parse a TOML config file at `path`.
pub fn load(path: &Path) -> Result<Config> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::ConfigIo {
        path: path.to_path_buf(),
        source,
    })?;
    let config = toml::from_str(&text).map_err(|source| Error::ConfigParse {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keystate.toml");
        std::fs::write(
            &path,
            r#"
[backend]
name = "keycloak"

[extract]
realm = "master"
db_url = "${DATABASE_URL}"

[output]
directory = "./out"
"#,
        )
        .unwrap();
        let config = load(&path).unwrap();
        assert_eq!(config.backend.name.as_deref(), Some("keycloak"));
        assert_eq!(config.extract.realm.as_deref(), Some("master"));
        assert_eq!(config.extract.db_url.as_deref(), Some("${DATABASE_URL}"));
        assert_eq!(config.output.directory.as_deref(), Some("./out"));
    }

    #[test]
    fn parses_an_empty_file_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keystate.toml");
        std::fs::write(&path, "").unwrap();
        let config = load(&path).unwrap();
        assert!(config.extract.realm.is_none());
        assert!(config.extract.db_url.is_none());
    }

    #[test]
    fn rejects_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keystate.toml");
        std::fs::write(&path, "[extract]\nrealm = \"master\"\nrealm_nme = \"x\"\n").unwrap();
        let error = load(&path).unwrap_err();
        assert!(error.to_string().contains("unknown field"), "{error}");
    }

    #[test]
    fn rejects_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keystate.toml");
        std::fs::write(&path, "[extract\nrealm = ").unwrap();
        let error = load(&path).unwrap_err();
        assert!(error.to_string().contains("config file"), "{error}");
    }

    #[test]
    fn missing_file_is_a_clear_error() {
        let error = load(Path::new("/nonexistent/keystate.toml")).unwrap_err();
        assert!(error.to_string().contains("config file"), "{error}");
    }
}
