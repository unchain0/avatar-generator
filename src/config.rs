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
        }
    }
}

fn env_path(key: &str, default: &str) -> PathBuf {
    PathBuf::from(std::env::var(key).unwrap_or_else(|_| default.to_string()))
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
