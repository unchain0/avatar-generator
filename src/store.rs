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
        let mut records = load_index(&index_path)?;
        let store = Self {
            data_dir,
            index_path,
            records: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
        };
        store.reconcile_on_startup(&mut records)?;
        *store.lock() = records;
        Ok(store)
    }

    /// Repair crash windows left by interrupted saves: finish pending renames,
    /// drop index entries whose PNG never landed, and scrub orphan temps.
    fn reconcile_on_startup(&self, records: &mut HashMap<String, AvatarRecord>) -> io::Result<()> {
        let mut changed = false;
        let mut drop_ids = Vec::new();

        for id in records.keys().cloned().collect::<Vec<_>>() {
            let Some(final_path) = self.image_path(&id) else {
                drop_ids.push(id);
                continue;
            };
            if final_path.exists() {
                continue;
            }
            let temp_path = self.temp_image_path(&id);
            if temp_path.exists() {
                match std::fs::rename(&temp_path, &final_path) {
                    Ok(()) => {}
                    Err(_) => {
                        let _ = std::fs::remove_file(&temp_path);
                        drop_ids.push(id);
                        changed = true;
                    }
                }
            } else {
                drop_ids.push(id);
                changed = true;
            }
        }

        for id in drop_ids {
            records.remove(&id);
            changed = true;
        }

        // Orphan temps that are not claimed by any remaining record.
        if let Ok(entries) = std::fs::read_dir(&self.data_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                let Some(id) = name
                    .strip_prefix(".tmp_")
                    .and_then(|s| s.strip_suffix(".png"))
                else {
                    continue;
                };
                if records.contains_key(id) {
                    // Record kept and still missing final PNG would have been
                    // repaired above; leftover temp with a live final is junk.
                    let Some(final_path) = self.image_path(id) else {
                        let _ = std::fs::remove_file(entry.path());
                        continue;
                    };
                    if final_path.exists() {
                        let _ = std::fs::remove_file(entry.path());
                    }
                    continue;
                }
                let _ = std::fs::remove_file(entry.path());
            }
        }

        if changed {
            self.persist(records)?;
        }
        Ok(())
    }

    fn temp_image_path(&self, id: &str) -> PathBuf {
        self.data_dir.join(format!(".tmp_{id}.png"))
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

        let temp_path = self.temp_image_path(&record.id);

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

        std::fs::write(&temp_path, bytes)?;

        if self.persist(&next).is_err() {
            let _ = std::fs::remove_file(&temp_path);
            return Err(io::Error::other("index persist failed; PNG removed"));
        }

        match std::fs::rename(&temp_path, &path) {
            Ok(()) => {
                *guard = next;
                Ok(())
            }
            Err(_e) => {
                // Rename failed after index persisted — roll back the index
                // on disk and in memory so the gallery does not render a broken card.
                next.remove(&avatar_id);
                let _ = self.persist(&next);
                let _ = std::fs::remove_file(&temp_path);
                Err(io::Error::other(
                    "rename failed after index persist; index rolled back",
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

        let mut next = guard.clone();
        next.remove(id);

        if self.persist(&next).is_err() {
            return Ok(false);
        }

        // Index and memory stay aligned even if the file unlink fails; an
        // orphan PNG is preferred over resurrecting a deleted record later.
        *guard = next;
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(self.temp_image_path(id));

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

        if self.persist(&next).is_err() {
            return Err(io::Error::other("index persist failed; no files deleted"));
        }

        // Commit memory as soon as the index is durable so a later PNG unlink
        // failure cannot resurrect a cleared session via a subsequent save.
        *guard = next;
        self.remove_image_files(&ids);
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

        if self.persist(&next).is_err() {
            return Err(io::Error::other("index persist failed; no files deleted"));
        }

        *guard = next;
        self.remove_image_files(&expired);
        Ok(expired.len())
    }

    fn remove_image_files(&self, ids: &[String]) {
        for id in ids {
            if let Some(path) = self.image_path(id) {
                let _ = std::fs::remove_file(path);
            }
            let _ = std::fs::remove_file(self.temp_image_path(id));
        }
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

    /// After the cleared index is durable, memory must match it even if a PNG
    /// unlink fails — otherwise a later save rewrites the stale map and
    /// resurrects a partial gallery.
    #[test]
    fn clear_session_commits_memory_before_file_unlink() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf()).unwrap();
        let future = Utc::now() + chrono::Duration::days(1);

        store
            .save(record(ID_A, "session-a", future), b"a1")
            .unwrap();
        store
            .save(record(ID_B, "session-a", future), b"a2")
            .unwrap();

        // Replace PNGs with non-empty directories so unlink fails.
        for id in [ID_A, ID_B] {
            let path = store.image_path(id).unwrap();
            std::fs::remove_file(&path).unwrap();
            std::fs::create_dir(&path).unwrap();
            std::fs::write(path.join("keep"), b"x").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o555)).unwrap();
            }
        }

        let removed = store.clear_session("session-a").unwrap();
        assert_eq!(removed, 2);
        assert_eq!(store.list_session("session-a").len(), 0);

        store
            .save(record(ID_C, "session-b", future), b"b1")
            .unwrap();
        assert!(store.get(ID_A).is_none());
        assert!(store.get(ID_B).is_none());
        assert!(store.get(ID_C).is_some());

        let reopened = Store::new(dir.path().to_path_buf()).unwrap();
        assert!(reopened.get(ID_A).is_none());
        assert!(reopened.get(ID_B).is_none());
        assert!(reopened.get(ID_C).is_some());

        for id in [ID_A, ID_B] {
            let path = store.image_path(id).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
            }
            let _ = std::fs::remove_dir_all(&path);
        }
    }

    /// Crash between index persist and final PNG rename: on reopen the temp
    /// file is promoted so the gallery does not serve a broken card.
    #[test]
    fn startup_recovers_pending_temp_png() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let future = Utc::now() + chrono::Duration::days(1);

        {
            let store = Store::new(path.clone()).unwrap();
            store.save(record(ID_A, "s1", future), b"png-a").unwrap();
        }

        let final_png = path.join(format!("{ID_A}.png"));
        let temp_png = path.join(format!(".tmp_{ID_A}.png"));
        std::fs::rename(&final_png, &temp_png).unwrap();
        assert!(!final_png.exists());
        assert!(temp_png.exists());

        let recovered = Store::new(path).unwrap();
        assert!(recovered.get(ID_A).is_some());
        assert_eq!(recovered.read_image(ID_A).unwrap().unwrap(), b"png-a");
        assert!(final_png.exists());
        assert!(!temp_png.exists());
    }

    /// Crash with index entry but neither final nor temp PNG drops the record.
    #[test]
    fn startup_drops_index_entry_without_png() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let future = Utc::now() + chrono::Duration::days(1);

        {
            let store = Store::new(path.clone()).unwrap();
            store.save(record(ID_A, "s1", future), b"png-a").unwrap();
        }

        std::fs::remove_file(path.join(format!("{ID_A}.png"))).unwrap();

        let recovered = Store::new(path.clone()).unwrap();
        assert!(recovered.get(ID_A).is_none());

        std::fs::write(path.join(format!(".tmp_{ID_B}.png")), b"orphan").unwrap();
        let scrubbed = Store::new(path).unwrap();
        assert!(!scrubbed
            .data_dir()
            .join(format!(".tmp_{ID_B}.png"))
            .exists());
    }
}
