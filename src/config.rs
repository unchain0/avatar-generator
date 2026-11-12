use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub data_dir: PathBuf,
    pub static_dir: PathBuf,
    pub robohash_base: String,
    pub default_ttl_days: i64,
    pub cleanup_interval_secs: u64,
    pub max_upstream_concurrency: usize,
    pub rate_limit_per_minute: u32,
    pub preview_limit_per_minute: u32,
    pub trust_forwarded_headers: bool,
}

impl Config {
    pub fn from_env() -> Self {
        let port = env_parse("PORT", 8080);
        let default_ttl_days = env_parse("TTL_DAYS", crate::models::DEFAULT_TTL_DAYS)
            .clamp(1, crate::models::MAX_TTL_DAYS);

        Self {
            port,
            data_dir: env_path("DATA_DIR", "./data"),
            static_dir: env_path("STATIC_DIR", "./static"),
            robohash_base: std::env::var("ROBOHASH_BASE_URL")
                .unwrap_or_else(|_| "https://robohash.org".to_string()),
            default_ttl_days,
            cleanup_interval_secs: 600,
            max_upstream_concurrency: 10,
            rate_limit_per_minute: 30,
            preview_limit_per_minute: 240,
            trust_forwarded_headers: env_parse_bool("TRUST_PROXY", false),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 8080,
            data_dir: PathBuf::from("./data"),
            static_dir: PathBuf::from("./static"),
            robohash_base: "https://robohash.org".to_string(),
            default_ttl_days: crate::models::DEFAULT_TTL_DAYS,
            cleanup_interval_secs: 600,
            max_upstream_concurrency: 10,
            rate_limit_per_minute: 30,
            preview_limit_per_minute: 240,
            trust_forwarded_headers: false,
        }
    }
}

fn env_path(key: &str, default: &str) -> PathBuf {
    PathBuf::from(std::env::var(key).unwrap_or_else(|_| default.to_string()))
}

fn env_parse_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn env_parse<T>(key: &str, default: T) -> T
where
    T: std::str::FromStr,
{
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<T>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn default_config_has_expected_values() {
        let config = Config::default();
        assert_eq!(config.port, 8080);
        assert_eq!(config.data_dir, PathBuf::from("./data"));
        assert_eq!(config.static_dir, PathBuf::from("./static"));
        assert_eq!(config.robohash_base, "https://robohash.org");
        assert_eq!(config.default_ttl_days, crate::models::DEFAULT_TTL_DAYS);
        assert_eq!(config.cleanup_interval_secs, 600);
        assert_eq!(config.max_upstream_concurrency, 10);
        assert_eq!(config.rate_limit_per_minute, 30);
        assert_eq!(config.preview_limit_per_minute, 240);
        assert!(!config.trust_forwarded_headers);
    }

    // Test the parsing logic directly without relying on global env vars
    #[test]
    fn env_parse_returns_default_when_unset() {
        // Use a unique key that won't conflict with other tests
        let key = "TEST_ENV_PARSE_UNSET_12345";
        env::remove_var(key);
        let result: u16 = env_parse(key, 42);
        assert_eq!(result, 42);
    }

    #[test]
    fn env_parse_parses_valid_value() {
        // Use a unique key and set/remove it atomically within the test
        let key = "TEST_ENV_PARSE_VALID_12345";
        env::set_var(key, "123");
        let result: u16 = env_parse(key, 0);
        assert_eq!(result, 123);
        env::remove_var(key);
    }

    #[test]
    fn env_parse_returns_default_on_invalid_value() {
        let key = "TEST_ENV_PARSE_INVALID_12345";
        env::set_var(key, "not-a-number");
        let result: u16 = env_parse(key, 99);
        assert_eq!(result, 99);
        env::remove_var(key);
    }

    #[test]
    fn env_parse_bool_returns_default_when_unset() {
        let key = "TEST_ENV_PARSE_BOOL_UNSET_12345";
        env::remove_var(key);
        let result = env_parse_bool(key, true);
        assert!(result);
    }

    #[test]
    fn env_parse_bool_parses_true_values() {
        for val in ["1", "true", "yes", "on", "TRUE", "Yes", "ON"] {
            let key = "TEST_ENV_PARSE_BOOL_TRUE_12345";
            env::set_var(key, val);
            let result = env_parse_bool(key, false);
            assert!(result, "value: {val}");
            env::remove_var(key);
        }
    }

    #[test]
    fn env_parse_bool_parses_false_values() {
        for val in ["0", "false", "no", "off", "FALSE", "No", ""] {
            let key = "TEST_ENV_PARSE_BOOL_FALSE_12345";
            env::set_var(key, val);
            let result = env_parse_bool(key, true);
            assert!(!result, "value: {val}");
            env::remove_var(key);
        }
    }

    #[test]
    fn env_path_returns_default_when_unset() {
        let key = "TEST_ENV_PATH_UNSET_12345";
        env::remove_var(key);
        let result = env_path(key, "/default/path");
        assert_eq!(result, PathBuf::from("/default/path"));
    }

    #[test]
    fn from_env_reads_every_override_and_clamps_the_ttl() {
        let keys = [
            "PORT",
            "TTL_DAYS",
            "DATA_DIR",
            "STATIC_DIR",
            "ROBOHASH_BASE_URL",
            "TRUST_PROXY",
        ];
        let previous: Vec<(&str, Option<String>)> =
            keys.iter().map(|key| (*key, env::var(key).ok())).collect();

        env::set_var("PORT", "9091");
        env::set_var("TTL_DAYS", "999");
        env::set_var("DATA_DIR", "/tmp/avatar-data");
        env::set_var("STATIC_DIR", "/tmp/avatar-static");
        env::set_var("ROBOHASH_BASE_URL", "http://127.0.0.1:9");
        env::set_var("TRUST_PROXY", "yes");

        let config = Config::from_env();

        for (key, value) in previous {
            match value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }
        }

        assert_eq!(config.port, 9091);
        assert_eq!(config.default_ttl_days, crate::models::MAX_TTL_DAYS);
        assert_eq!(config.data_dir, PathBuf::from("/tmp/avatar-data"));
        assert_eq!(config.static_dir, PathBuf::from("/tmp/avatar-static"));
        assert_eq!(config.robohash_base, "http://127.0.0.1:9");
        assert!(config.trust_forwarded_headers);
    }

    #[test]
    fn from_env_falls_back_to_defaults_when_unset() {
        let keys = [
            "PORT",
            "TTL_DAYS",
            "DATA_DIR",
            "STATIC_DIR",
            "ROBOHASH_BASE_URL",
            "TRUST_PROXY",
        ];
        let previous: Vec<(&str, Option<String>)> =
            keys.iter().map(|key| (*key, env::var(key).ok())).collect();
        for key in keys {
            env::remove_var(key);
        }

        let config = Config::from_env();

        for (key, value) in previous {
            if let Some(value) = value {
                env::set_var(key, value);
            }
        }

        assert_eq!(config.port, 8080);
        assert_eq!(config.data_dir, PathBuf::from("./data"));
        assert_eq!(config.robohash_base, "https://robohash.org");
        assert_eq!(config.default_ttl_days, crate::models::DEFAULT_TTL_DAYS);
        assert!(!config.trust_forwarded_headers);
    }

    #[test]
    fn env_path_returns_custom_when_set() {
        let key = "TEST_ENV_PATH_CUSTOM_12345";
        env::set_var(key, "/custom/path");
        let result = env_path(key, "/default/path");
        assert_eq!(result, PathBuf::from("/custom/path"));
        env::remove_var(key);
    }
}
