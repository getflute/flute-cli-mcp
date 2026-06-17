use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Sandbox,
    Production,
}

impl Profile {
    pub fn as_cli_str(self) -> &'static str {
        match self {
            Profile::Sandbox => "sandbox",
            Profile::Production => "production",
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid FLUTE_PROFILE value `{0}` (expected `sandbox` or `production`)")]
    InvalidProfile(String),
    #[error("invalid FLUTE_MCP_TIMEOUT_SECS value `{0}` (expected positive integer)")]
    InvalidTimeout(String),
    #[error("could not locate `flute` on PATH and FLUTE_BIN is unset")]
    BinaryNotFound,
    #[error("FLUTE_BIN=`{0}` does not exist or is not executable")]
    BinaryUnusable(String),
}

#[derive(Debug, Clone)]
pub struct Config {
    pub profile: Profile,
    pub binary: PathBuf,
    pub merchant_id: Option<String>,
    pub timeout: Duration,
    pub debug: bool,
    pub allow_prod_writes: bool,
}

/// Parse a boolean env flag strictly: only `1`/`true`/`yes`/`on` (case-insensitive,
/// trimmed) enable it. `0`, `false`, `no`, empty, or anything else are false — so a
/// value like `FLUTE_MCP_ALLOW_PROD_WRITES=false` does NOT enable production writes.
fn truthy(value: Option<String>) -> bool {
    match value {
        Some(s) => matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        None => false,
    }
}

/// Whether `path` is a file we can execute. On Unix this checks the executable bit so
/// a non-executable file is rejected at startup rather than failing on the first call.
#[cfg(unix)]
fn is_executable_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &std::path::Path) -> bool {
    path.is_file()
}

impl Config {
    /// Build a Config from a closure that returns env vars (so tests can inject).
    pub fn from_env<F>(getenv: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let profile = match getenv("FLUTE_PROFILE").as_deref() {
            None | Some("") | Some("sandbox") => Profile::Sandbox,
            Some("production") | Some("prod") => Profile::Production,
            Some(other) => return Err(ConfigError::InvalidProfile(other.to_string())),
        };

        let timeout = match getenv("FLUTE_MCP_TIMEOUT_SECS") {
            None => Duration::from_secs(30),
            Some(s) => {
                let n: u64 = s
                    .parse()
                    .map_err(|_| ConfigError::InvalidTimeout(s.clone()))?;
                if n == 0 {
                    return Err(ConfigError::InvalidTimeout(s));
                }
                Duration::from_secs(n)
            }
        };

        let binary = match getenv("FLUTE_BIN") {
            Some(p) if !p.is_empty() => {
                let path = PathBuf::from(&p);
                if !is_executable_file(&path) {
                    return Err(ConfigError::BinaryUnusable(p));
                }
                path
            }
            _ => which::which("flute").map_err(|_| ConfigError::BinaryNotFound)?,
        };

        let merchant_id = getenv("FLUTE_MERCHANT_ID").filter(|s| !s.is_empty());
        let debug = truthy(getenv("FLUTE_MCP_DEBUG"));
        let allow_prod_writes = truthy(getenv("FLUTE_MCP_ALLOW_PROD_WRITES"));

        Ok(Self {
            profile,
            binary,
            merchant_id,
            timeout,
            debug,
            allow_prod_writes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn make_env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    fn fake_binary(dir: &TempDir) -> PathBuf {
        let path = dir.path().join("flute");
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    #[test]
    fn defaults_to_sandbox_30s_no_prod_writes() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        let pairs = [("FLUTE_BIN", bin.to_str().unwrap())];
        let env = make_env(&pairs);
        let cfg = Config::from_env(env).unwrap();
        assert_eq!(cfg.profile, Profile::Sandbox);
        assert_eq!(cfg.timeout, Duration::from_secs(30));
        assert!(!cfg.debug);
        assert!(!cfg.allow_prod_writes);
        assert_eq!(cfg.merchant_id, None);
    }

    #[test]
    fn accepts_production_and_prod_alias() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        for value in ["production", "prod"] {
            let pairs = [
                ("FLUTE_BIN", bin.to_str().unwrap()),
                ("FLUTE_PROFILE", value),
            ];
            let env = make_env(&pairs);
            assert_eq!(Config::from_env(env).unwrap().profile, Profile::Production);
        }
    }

    #[test]
    fn reads_merchant_id_and_prod_write_override() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        let pairs = [
            ("FLUTE_BIN", bin.to_str().unwrap()),
            ("FLUTE_MERCHANT_ID", "m-123"),
            ("FLUTE_MCP_ALLOW_PROD_WRITES", "1"),
        ];
        let env = make_env(&pairs);
        let cfg = Config::from_env(env).unwrap();
        assert_eq!(cfg.merchant_id.as_deref(), Some("m-123"));
        assert!(cfg.allow_prod_writes);
    }

    #[test]
    fn false_like_values_do_not_enable_prod_writes() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        for value in ["false", "0", "no", "off", ""] {
            let pairs = [
                ("FLUTE_BIN", bin.to_str().unwrap()),
                ("FLUTE_MCP_ALLOW_PROD_WRITES", value),
            ];
            let env = make_env(&pairs);
            assert!(
                !Config::from_env(env).unwrap().allow_prod_writes,
                "value {value:?} must NOT enable production writes"
            );
        }
    }

    #[test]
    fn truthy_values_enable_prod_writes() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        for value in ["1", "true", "TRUE", "yes", "on"] {
            let pairs = [
                ("FLUTE_BIN", bin.to_str().unwrap()),
                ("FLUTE_MCP_ALLOW_PROD_WRITES", value),
            ];
            let env = make_env(&pairs);
            assert!(
                Config::from_env(env).unwrap().allow_prod_writes,
                "value {value:?} must enable production writes"
            );
        }
    }

    #[test]
    fn non_executable_binary_errors() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("flute");
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644); // present but not executable
        std::fs::set_permissions(&path, perms).unwrap();
        let pairs = [("FLUTE_BIN", path.to_str().unwrap())];
        let env = make_env(&pairs);
        assert!(matches!(
            Config::from_env(env),
            Err(ConfigError::BinaryUnusable(_))
        ));
    }

    #[test]
    fn rejects_unknown_profile() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        let pairs = [
            ("FLUTE_BIN", bin.to_str().unwrap()),
            ("FLUTE_PROFILE", "staging"),
        ];
        let env = make_env(&pairs);
        assert!(
            matches!(Config::from_env(env), Err(ConfigError::InvalidProfile(s)) if s == "staging")
        );
    }

    #[test]
    fn missing_binary_errors() {
        let env = make_env(&[("FLUTE_BIN", "/nope/does/not/exist")]);
        assert!(matches!(
            Config::from_env(env),
            Err(ConfigError::BinaryUnusable(_))
        ));
    }
}
