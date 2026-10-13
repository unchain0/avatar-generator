use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MIN_COUNT: u32 = 1;
pub const MAX_COUNT: u32 = 50;
pub const VALID_SIZES: [u32; 9] = [50, 100, 200, 300, 400, 500, 640, 800, 1024];
pub const DEFAULT_TTL_DAYS: i64 = 1;
pub const MAX_TTL_DAYS: i64 = 30;
pub const MAX_SEED_LEN: usize = 128;
pub const MAX_SESSION_AVATARS: usize = 100;
pub const MAX_ARCHIVE_BYTES: usize = 100 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoboSet {
    #[serde(rename = "set1")]
    Set1,
    #[serde(rename = "set2")]
    Set2,
    #[serde(rename = "set3")]
    Set3,
    #[serde(rename = "set4")]
    Set4,
    #[serde(rename = "set5")]
    Set5,
}

impl RoboSet {
    pub fn as_str(self) -> &'static str {
        match self {
            RoboSet::Set1 => "set1",
            RoboSet::Set2 => "set2",
            RoboSet::Set3 => "set3",
            RoboSet::Set4 => "set4",
            RoboSet::Set5 => "set5",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "set1" | "1" => Ok(RoboSet::Set1),
            "set2" | "2" => Ok(RoboSet::Set2),
            "set3" | "3" => Ok(RoboSet::Set3),
            "set4" | "4" => Ok(RoboSet::Set4),
            "set5" | "5" => Ok(RoboSet::Set5),
            other => Err(format!("invalid set '{other}', expected set1..set5")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Background {
    #[serde(rename = "bg1")]
    Bg1,
    #[serde(rename = "bg2")]
    Bg2,
    #[serde(rename = "none")]
    None,
}

impl Background {
    pub fn as_str(self) -> &'static str {
        match self {
            Background::Bg1 => "bg1",
            Background::Bg2 => "bg2",
            Background::None => "none",
        }
    }

    pub fn upstream_param(self) -> Option<&'static str> {
        match self {
            Background::Bg1 => Some("bg1"),
            Background::Bg2 => Some("bg2"),
            Background::None => None,
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "bg1" => Ok(Background::Bg1),
            "bg2" => Ok(Background::Bg2),
            "none" | "" => Ok(Background::None),
            other => Err(format!(
                "invalid background '{other}', expected bg1, bg2 or none"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvatarOptions {
    pub set: RoboSet,
    pub background: Background,
    pub size: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenerateRequest {
    pub count: u32,
    #[serde(default)]
    pub seed: Option<String>,
    pub set: String,
    pub background: String,
    pub size: u32,
    #[serde(default)]
    pub ttl_days: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ValidatedRequest {
    pub count: u32,
    pub seed: Option<String>,
    pub options: AvatarOptions,
    pub ttl_days: i64,
}

pub fn validate_generate_request(
    req: &GenerateRequest,
    default_ttl_days: i64,
) -> Result<ValidatedRequest, String> {
    if !(MIN_COUNT..=MAX_COUNT).contains(&req.count) {
        return Err(format!("count must be between {MIN_COUNT} and {MAX_COUNT}"));
    }

    let set = RoboSet::parse(&req.set)?;
    let background = Background::parse(&req.background)?;

    if !VALID_SIZES.contains(&req.size) {
        return Err(format!(
            "invalid size {}, allowed: {VALID_SIZES:?}",
            req.size
        ));
    }

    let seed = req.seed.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(seed) = seed {
        if seed.chars().count() > MAX_SEED_LEN {
            return Err(format!("seed must be at most {MAX_SEED_LEN} characters"));
        }
    }

    let ttl_days = req.ttl_days.map(i64::from).unwrap_or(default_ttl_days);
    if !(1..=MAX_TTL_DAYS).contains(&ttl_days) {
        return Err(format!("ttl_days must be between 1 and {MAX_TTL_DAYS}"));
    }

    Ok(ValidatedRequest {
        count: req.count,
        seed: seed.map(str::to_owned),
        options: AvatarOptions {
            set,
            background,
            size: req.size,
        },
        ttl_days,
    })
}

pub fn random_base_seed() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn derive_seed(base: &str, index: u32) -> String {
    let mut hasher = Sha256::new();
    hasher.update(base.as_bytes());
    hasher.update(b"-");
    hasher.update(index.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarRecord {
    pub id: String,
    pub seed: String,
    pub options: AvatarOptions,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expires_at: DateTime<Utc>,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AvatarView {
    pub id: String,
    pub seed: String,
    pub seed_short: String,
    pub set: String,
    pub background: String,
    pub size: u32,
    pub expires_at: String,
}

impl AvatarRecord {
    pub fn view(&self) -> AvatarView {
        AvatarView {
            id: self.id.clone(),
            seed: self.seed.clone(),
            seed_short: self.seed.chars().take(16).collect(),
            set: self.options.set.as_str().to_string(),
            background: self.options.background.as_str().to_string(),
            size: self.options.size,
            expires_at: self.expires_at.to_rfc3339_opts(SecondsFormat::Secs, true),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct GenerateResponse {
    pub count: usize,
    pub avatars: Vec<AvatarView>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> GenerateRequest {
        GenerateRequest {
            count: 3,
            seed: Some("hello".into()),
            set: "set2".into(),
            background: "bg1".into(),
            size: 200,
            ttl_days: None,
        }
    }

    #[test]
    fn validate_accepts_well_formed_request() {
        let req = base_request();
        let validated = validate_generate_request(&req, DEFAULT_TTL_DAYS).unwrap();
        assert_eq!(validated.count, 3);
        assert_eq!(validated.options.set, RoboSet::Set2);
        assert_eq!(validated.options.background, Background::Bg1);
        assert_eq!(validated.options.size, 200);
        assert_eq!(validated.ttl_days, 1);
        assert_eq!(validated.seed.as_deref(), Some("hello"));
    }

    #[test]
    fn validate_applies_default_ttl_when_absent() {
        let mut req = base_request();
        req.ttl_days = None;
        let validated = validate_generate_request(&req, 7).unwrap();
        assert_eq!(validated.ttl_days, 7);
    }

    #[test]
    fn validate_rejects_out_of_range_count() {
        for count in [0, 51] {
            let mut req = base_request();
            req.count = count;
            assert!(validate_generate_request(&req, DEFAULT_TTL_DAYS).is_err());
        }
    }

    #[test]
    fn validate_rejects_unknown_set_and_background() {
        let mut req = base_request();
        req.set = "set9".into();
        assert!(validate_generate_request(&req, DEFAULT_TTL_DAYS).is_err());

        let mut req = base_request();
        req.background = "rainbow".into();
        assert!(validate_generate_request(&req, DEFAULT_TTL_DAYS).is_err());
    }

    #[test]
    fn validate_rejects_unknown_size() {
        let mut req = base_request();
        req.size = 123;
        assert!(validate_generate_request(&req, DEFAULT_TTL_DAYS).is_err());
    }

    #[test]
    fn validate_clamps_ttl_bounds() {
        let mut req = base_request();
        req.ttl_days = Some(0);
        assert!(validate_generate_request(&req, DEFAULT_TTL_DAYS).is_err());

        let mut req = base_request();
        req.ttl_days = Some(31);
        assert!(validate_generate_request(&req, DEFAULT_TTL_DAYS).is_err());
    }

    #[test]
    fn validate_treats_blank_seed_as_random() {
        let mut req = base_request();
        req.seed = Some("   ".into());
        let validated = validate_generate_request(&req, DEFAULT_TTL_DAYS).unwrap();
        assert!(validated.seed.is_none());
    }

    #[test]
    fn validate_counts_seed_characters_not_bytes() {
        let mut req = base_request();
        req.seed = Some("ç".repeat(MAX_SEED_LEN));
        let validated = validate_generate_request(&req, DEFAULT_TTL_DAYS).unwrap();
        assert_eq!(validated.seed.unwrap().chars().count(), MAX_SEED_LEN);

        let mut req = base_request();
        req.seed = Some("ç".repeat(MAX_SEED_LEN + 1));
        assert!(validate_generate_request(&req, DEFAULT_TTL_DAYS).is_err());
    }

    #[test]
    fn parse_set_accepts_numeric_and_prefixed_forms() {
        assert_eq!(RoboSet::parse("1").unwrap(), RoboSet::Set1);
        assert_eq!(RoboSet::parse("set3").unwrap(), RoboSet::Set3);
    }

    #[test]
    fn derive_seed_is_deterministic_and_index_sensitive() {
        let a = derive_seed("hello", 0);
        let b = derive_seed("hello", 0);
        let c = derive_seed("hello", 1);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn avatar_view_shortens_seed() {
        let record = AvatarRecord {
            id: "abc".into(),
            seed: "0123456789abcdefghij".into(),
            options: AvatarOptions {
                set: RoboSet::Set1,
                background: Background::None,
                size: 50,
            },
            created_at: Utc::now(),
            expires_at: Utc::now(),
            session_id: "s".into(),
        };
        assert_eq!(record.view().seed_short, "0123456789abcdef");
    }
}
