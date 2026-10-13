use crate::models::AvatarRecord;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Store {
    data_dir: PathBuf,
    index_path: PathBuf,
    records: Mutex<HashMap<String, AvatarRecord>>,
}

impl Store {
    pub fn new(data_dir: PathBuf) -> io::Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let index_path = data_dir.join("index.json");
        let records = load_index(&index_path);
        Ok(Self {
            data_dir,
            index_path,
            records: Mutex::new(records),
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn image_path(&self, id: &str) -> PathBuf {
        self.data_dir.join(format!("{id}.png"))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, AvatarRecord>> {
        self.records.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn persist(&self, records: &HashMap<String, AvatarRecord>) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(records)?;
        let tmp = self.index_path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.index_path)?;
        Ok(())
    }

    pub fn save(&self, record: AvatarRecord, bytes: &[u8]) -> io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::write(self.image_path(&record.id), bytes)?;

        let mut records = self.lock();
        records.insert(record.id.clone(), record);
        self.persist(&records)
    }

    pub fn get(&self, id: &str) -> Option<AvatarRecord> {
        self.lock().get(id).cloned()
    }

    pub fn read_image(&self, id: &str) -> io::Result<Option<Vec<u8>>> {
        let path = self.image_path(id);
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn delete(&self, id: &str) -> io::Result<bool> {
        let removed_file = match std::fs::remove_file(self.image_path(id)) {
            Ok(()) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e),
        };

        let mut records = self.lock();
        let removed_record = records.remove(id).is_some();
        if removed_record {
            self.persist(&records)?;
        }
        Ok(removed_file || removed_record)
    }

    pub fn list_session(&self, session_id: &str) -> Vec<AvatarRecord> {
        let records = self.lock();
        let mut matched: Vec<AvatarRecord> = records
            .values()
            .filter(|r| r.session_id == session_id)
            .cloned()
            .collect();
        matched.sort_by_key(|r| std::cmp::Reverse(r.created_at));
        matched
    }

    pub fn clear_session(&self, session_id: &str) -> io::Result<usize> {
        let mut records = self.lock();
        let ids: Vec<String> = records
            .values()
            .filter(|r| r.session_id == session_id)
            .map(|r| r.id.clone())
            .collect();

        for id in &ids {
            match std::fs::remove_file(self.image_path(id)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }

        if !ids.is_empty() {
            records.retain(|_, r| r.session_id != session_id);
            self.persist(&records)?;
        }
        Ok(ids.len())
    }

    pub fn cleanup_expired(&self, now: DateTime<Utc>) -> io::Result<usize> {
        let mut records = self.lock();
        let expired: Vec<String> = records
            .values()
            .filter(|r| r.expires_at <= now)
            .map(|r| r.id.clone())
            .collect();

        for id in &expired {
            match std::fs::remove_file(self.image_path(id)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }

        if !expired.is_empty() {
            records.retain(|_, r| r.expires_at > now);
            self.persist(&records)?;
        }
        Ok(expired.len())
    }
}

fn load_index(path: &Path) -> HashMap<String, AvatarRecord> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AvatarOptions, Background, RoboSet};
    use tempfile::tempdir;

    fn record(id: &str, session: &str, expires_at: DateTime<Utc>) -> AvatarRecord {
        AvatarRecord {
            id: id.into(),
            seed: format!("seed-{id}"),
            options: AvatarOptions {
                set: RoboSet::Set1,
                background: Background::Bg1,
                size: 200,
            },
            created_at: Utc::now(),
            expires_at,
            session_id: session.into(),
        }
    }

    #[test]
    fn save_then_read_roundtrips_bytes() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf()).unwrap();

        store
            .save(
                record("abc", "s1", Utc::now() + chrono::Duration::days(1)),
                b"png-bytes",
            )
            .unwrap();

        assert_eq!(store.read_image("abc").unwrap().unwrap(), b"png-bytes");
        assert!(store.get("abc").is_some());
    }

    #[test]
    fn cleanup_removes_only_expired_records_and_files() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf()).unwrap();
        let now = Utc::now();

        store
            .save(
                record("old", "s1", now - chrono::Duration::hours(1)),
                b"old",
            )
            .unwrap();
        store
            .save(
                record("fresh", "s1", now + chrono::Duration::hours(1)),
                b"fresh",
            )
            .unwrap();

        let removed = store.cleanup_expired(now).unwrap();
        assert_eq!(removed, 1);
        assert!(store.get("old").is_none());
        assert!(store.get("fresh").is_some());
        assert!(!store.image_path("old").exists());
        assert!(store.image_path("fresh").exists());
    }

    #[test]
    fn clear_session_leaves_other_sessions_untouched() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf()).unwrap();
        let future = Utc::now() + chrono::Duration::days(1);

        store
            .save(record("a1", "session-a", future), b"a1")
            .unwrap();
        store
            .save(record("a2", "session-a", future), b"a2")
            .unwrap();
        store
            .save(record("b1", "session-b", future), b"b1")
            .unwrap();

        assert_eq!(store.clear_session("session-a").unwrap(), 2);
        assert_eq!(store.list_session("session-a").len(), 0);
        assert_eq!(store.list_session("session-b").len(), 1);
    }

    #[test]
    fn index_survives_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();
        {
            let store = Store::new(path.clone()).unwrap();
            store
                .save(
                    record("persisted", "s1", Utc::now() + chrono::Duration::days(1)),
                    b"x",
                )
                .unwrap();
        }

        let reopened = Store::new(path).unwrap();
        assert!(reopened.get("persisted").is_some());
    }

    #[test]
    fn delete_reports_missing_record() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf()).unwrap();
        assert!(!store.delete("nope").unwrap());
    }

    #[test]
    fn list_session_is_ordered_newest_first() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf()).unwrap();
        let future = Utc::now() + chrono::Duration::days(1);

        let mut older = record("older", "s", future);
        older.created_at = Utc::now() - chrono::Duration::minutes(5);
        let mut newer = record("newer", "s", future);
        newer.created_at = Utc::now();

        store.save(older, b"o").unwrap();
        store.save(newer, b"n").unwrap();

        let list = store.list_session("s");
        assert_eq!(list[0].id, "newer");
        assert_eq!(list[1].id, "older");
    }
}
