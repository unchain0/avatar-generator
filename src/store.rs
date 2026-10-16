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
    pending: Mutex<HashMap<String, usize>>,
}

fn is_valid_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok()
}

impl Store {
    pub fn new(data_dir: PathBuf) -> io::Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let index_path = data_dir.join("index.json");
        let records = load_index(&index_path)?;
        Ok(Self {
            data_dir,
            index_path,
            records: Mutex::new(records),
            pending: Mutex::new(HashMap::new()),
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn image_path(&self, id: &str) -> Option<PathBuf> {
        is_valid_id(id).then(|| self.data_dir.join(format!("{id}.png")))
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
        let path = self
            .image_path(&record.id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid avatar id"))?;

        let temp_path = self.data_dir.join(format!(".tmp_{}.png", record.id));

        let mut guard = self.lock();
        let mut next = guard.clone();
        let is_new = !next.contains_key(&record.id);
        if is_new {
            let session_count = next
                .values()
                .filter(|r| r.session_id == record.session_id)
                .count();
            if session_count >= crate::models::MAX_SESSION_AVATARS {
                return Err(io::Error::other("session avatar cap exceeded"));
            }
        }
        let avatar_id = record.id.clone();
        next.insert(avatar_id.clone(), record);

        // Write the PNG to a temporary path first so we can clean up on index failure.
        std::fs::write(&temp_path, bytes)?;

        // Persist the index first. On failure, remove the temp PNG and return error.
        if self.persist(&next).is_err() {
            let _ = std::fs::remove_file(&temp_path);
            return Err(io::Error::other("index persist failed; PNG removed"));
        }

        // Index succeeded — move the temp PNG to its final location.
        match std::fs::rename(&temp_path, &path) {
            Ok(()) => {
                *guard = next;
                Ok(())
            }
            Err(_e) => {
                // Rename failed after index persisted — roll back the index
                // on disk and in memory so the gallery does not render a broken card.
                guard.remove(&avatar_id);
                let _ = self.persist(&guard);
                let _ = std::fs::remove_file(&temp_path);
                Err(io::Error::other(
                    "rename failed after index persist; index rolled back"
                ))
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<AvatarRecord> {
        self.lock().get(id).cloned()
    }

    pub fn read_image(&self, id: &str) -> io::Result<Option<Vec<u8>>> {
        let Some(path) = self.image_path(id) else {
            return Ok(None);
        };
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn delete(&self, id: &str) -> io::Result<bool> {
        let Some(path) = self.image_path(id) else {
            return Ok(false);
        };

        let mut guard = self.lock();
        if !guard.contains_key(id) {
            return Ok(false);
        }

        // Build the new index state (without the avatar) and persist it first,
        // so that if persistence fails the file and in-memory index remain
        // consistent (avatar still present in both). Only after the index is
        // successfully written do we remove the PNG file.
        let mut next = guard.clone();
        next.remove(id);

        if self.persist(&next).is_err() {
            // Persistence failed — leave in-memory and on-disk index unchanged,
            // and leave the PNG file in place. The operation is recoverable: the
            // next cleanup or delete will find the avatar still present.
            return Ok(false);
        }

        // Index persisted successfully — now remove the PNG file.
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }

        *guard = next;
        Ok(true)
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

    pub fn count_session(&self, session_id: &str) -> usize {
        self.lock()
            .values()
            .filter(|r| r.session_id == session_id)
            .count()
    }

    /// Atomically reserve capacity for a session. Returns Ok(()) if the session
    /// can accommodate `additional` more avatars, otherwise returns an error.
    /// The reservation is not persisted; it only ensures the check and the
    /// subsequent save happen atomically with respect to other reservations.
    pub fn reserve_capacity(&self, session_id: &str, additional: usize) -> io::Result<()> {
        let guard = self.lock();
        let mut pend = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let session_count = guard
            .values()
            .filter(|r| r.session_id == session_id)
            .count();
        let reserved = pend.get(session_id).copied().unwrap_or(0);
        if session_count + reserved + additional > crate::models::MAX_SESSION_AVATARS {
            return Err(io::Error::other(
                "session would exceed the maximum avatar cap",
            ));
        }
        *pend.entry(session_id.to_string()).or_insert(0) += additional;
        Ok(())
    }

    pub fn release_reservation(&self, session_id: &str, count: usize) {
        let mut pend = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = pend.get_mut(session_id) {
            *v = v.saturating_sub(count);
            if *v == 0 {
                pend.remove(session_id);
            }
        }
    }

    pub fn clear_session(&self, session_id: &str) -> io::Result<usize> {
        let mut guard = self.lock();
        let ids: Vec<String> = guard
            .values()
            .filter(|r| r.session_id == session_id)
            .map(|r| r.id.clone())
            .collect();

        if ids.is_empty() {
            return Ok(0);
        }

        let mut next = guard.clone();
        next.retain(|_, r| r.session_id != session_id);

        // Persist the proposed index first, so that if it fails the old index
        // still reflects the current on-disk state and no PNGs have been deleted yet.
        if self.persist(&next).is_err() {
            return Err(io::Error::other("index persist failed; no files deleted"));
        }

        // Index persisted successfully — now remove the PNG files.
        for id in &ids {
            if let Some(path) = self.image_path(id) {
                match std::fs::remove_file(path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
        }

        *guard = next;
        Ok(ids.len())
    }

    pub fn cleanup_expired(&self, now: DateTime<Utc>) -> io::Result<usize> {
        let mut guard = self.lock();
        let expired: Vec<String> = guard
            .values()
            .filter(|r| r.expires_at <= now)
            .map(|r| r.id.clone())
            .collect();

        if expired.is_empty() {
            return Ok(0);
        }

        let mut next = guard.clone();
        next.retain(|_, r| r.expires_at > now);

        // Persist the index first, so that if it fails the old index still reflects
        // the current on-disk state and no PNGs have been deleted yet.
        if self.persist(&next).is_err() {
            return Err(io::Error::other("index persist failed; no files deleted"));
        }

        // Index persisted successfully — now remove the PNG files.
        for id in &expired {
            if let Some(path) = self.image_path(id) {
                match std::fs::remove_file(path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
        }

        *guard = next;
        Ok(expired.len())
    }
}

fn load_index(path: &Path) -> io::Result<HashMap<String, AvatarRecord>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AvatarOptions, Background, RoboSet};
    use tempfile::tempdir;

    const ID_A: &str = "11111111-1111-1111-1111-111111111111";
    const ID_B: &str = "22222222-2222-2222-2222-222222222222";
    const ID_C: &str = "33333333-3333-3333-3333-333333333333";

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
    fn image_path_rejects_non_uuid_ids() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf()).unwrap();
        assert!(store.image_path(ID_A).is_some());
        for bad in ["abc", "../../etc/passwd", "a/b", ""] {
            assert!(store.image_path(bad).is_none(), "accepted {bad:?}");
        }
    }

    #[test]
    fn save_then_read_roundtrips_bytes() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf()).unwrap();

        store
            .save(
                record(ID_A, "s1", Utc::now() + chrono::Duration::days(1)),
                b"png-bytes",
            )
            .unwrap();

        assert_eq!(store.read_image(ID_A).unwrap().unwrap(), b"png-bytes");
        assert!(store.get(ID_A).is_some());
    }

    #[test]
    fn cleanup_removes_only_expired_records_and_files() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf()).unwrap();
        let now = Utc::now();

        store
            .save(record(ID_A, "s1", now - chrono::Duration::hours(1)), b"old")
            .unwrap();
        store
            .save(
                record(ID_B, "s1", now + chrono::Duration::hours(1)),
                b"fresh",
            )
            .unwrap();

        let removed = store.cleanup_expired(now).unwrap();
        assert_eq!(removed, 1);
        assert!(store.get(ID_A).is_none());
        assert!(store.get(ID_B).is_some());
        assert!(!store.image_path(ID_A).unwrap().exists());
        assert!(store.image_path(ID_B).unwrap().exists());
    }

    #[test]
    fn clear_session_leaves_other_sessions_untouched() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf()).unwrap();
        let future = Utc::now() + chrono::Duration::days(1);

        store
            .save(record(ID_A, "session-a", future), b"a1")
            .unwrap();
        store
            .save(record(ID_B, "session-a", future), b"a2")
            .unwrap();
        store
            .save(record(ID_C, "session-b", future), b"b1")
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
                    record(ID_A, "s1", Utc::now() + chrono::Duration::days(1)),
                    b"x",
                )
                .unwrap();
        }

        let reopened = Store::new(path).unwrap();
        assert!(reopened.get(ID_A).is_some());
    }

    #[test]
    fn corrupt_index_fails_to_open() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("index.json"), b"not json").unwrap();
        assert!(Store::new(dir.path().to_path_buf()).is_err());
    }

    #[test]
    fn delete_reports_missing_record() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf()).unwrap();
        assert!(!store.delete(ID_A).unwrap());
    }

    /// When persist() fails, clear_session must leave the in-memory index
    /// intact (guard unchanged) and not touch any PNG files. Otherwise a
    /// failed clear would empty the session in memory while the on-disk index
    /// and PNGs still reference them, permanently orphaning the avatars.
    #[test]
    fn clear_session_keeps_index_on_persist_failure() {
        let dir = tempdir().unwrap();
        let mut store = Store::new(dir.path().to_path_buf()).unwrap();
        let future = Utc::now() + chrono::Duration::days(1);

        store
            .save(record(ID_A, "session-a", future), b"a1")
            .unwrap();
        store
            .save(record(ID_B, "session-a", future), b"a2")
            .unwrap();

        store.index_path = dir.path().join("missing/index.json");

        let result = store.clear_session("session-a");
        assert!(
            result.is_err(),
            "clear_session must fail when persist fails"
        );

        // Guard must be unchanged: both records survive in memory and on disk.
        assert_eq!(store.list_session("session-a").len(), 2);
        assert!(store.get(ID_A).is_some());
        assert!(store.get(ID_B).is_some());
        assert!(store.image_path(ID_A).unwrap().exists());
        assert!(store.image_path(ID_B).unwrap().exists());
    }

    /// cleanup_expired must persist the proposed index BEFORE unlinking PNGs.
    /// If persistence fails, the expired record must remain in the index and
    /// its PNG must stay on disk (no broken-image state).
    #[test]
    fn cleanup_expired_keeps_index_on_persist_failure() {
        let dir = tempdir().unwrap();
        let mut store = Store::new(dir.path().to_path_buf()).unwrap();
        let now = Utc::now();

        store
            .save(record(ID_A, "s1", now - chrono::Duration::hours(1)), b"old")
            .unwrap();
        store
            .save(
                record(ID_B, "s1", now + chrono::Duration::hours(1)),
                b"fresh",
            )
            .unwrap();

        store.index_path = dir.path().join("missing/index.json");

        let result = store.cleanup_expired(now);
        assert!(
            result.is_err(),
            "cleanup_expired must fail when persist fails"
        );

        // The expired record must NOT be unlinked: still in index and on disk.
        assert!(store.get(ID_A).is_some());
        assert!(store.image_path(ID_A).unwrap().exists());
        assert!(store.get(ID_B).is_some());
    }

    #[test]
    fn list_session_is_ordered_newest_first() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf()).unwrap();
        let future = Utc::now() + chrono::Duration::days(1);

        let mut older = record(ID_A, "s", future);
        older.created_at = Utc::now() - chrono::Duration::minutes(5);
        let mut newer = record(ID_B, "s", future);
        newer.created_at = Utc::now();

        store.save(older, b"o").unwrap();
        store.save(newer, b"n").unwrap();

        let list = store.list_session("s");
        assert_eq!(list[0].id, ID_B);
        assert_eq!(list[1].id, ID_A);
        assert_eq!(store.count_session("s"), 2);
    }
}
