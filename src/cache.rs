//! Bounded session reports and persistent redacted API results. Never persist raw logs.
use crate::{Report, Result, discovery};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn key(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
const MAX_ENTRY: usize = 2 * 1024 * 1024;
pub const AI_TTL: u64 = 7 * 24 * 3600;
pub const METADATA_TTL: u64 = 6 * 3600;

#[derive(Clone)]
pub struct Disk {
    pub root: PathBuf,
}
#[derive(Serialize, serde::Deserialize)]
struct Entry {
    created: u64,
    value: serde_json::Value,
}

impl Disk {
    pub fn user() -> Self {
        let root = if cfg!(test) {
            std::env::temp_dir().join(format!("prism-doctor-tests-{}", std::process::id()))
        } else {
            std::env::var_os("LOCALAPPDATA")
                .or_else(|| std::env::var_os("APPDATA"))
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                // Keep existing caches and repair-copy backups accessible after the rebrand.
                .join("PrismAIDoctor/cache-v1")
        };
        Self { root }
    }
    fn path(&self, namespace: &str, hash: &str) -> Result<PathBuf> {
        if !["ai", "metadata", "artwork"].contains(&namespace)
            || hash.len() != 64
            || !hash.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err("Invalid cache key".into());
        }
        Ok(self.root.join(format!("{namespace}-{hash}.json")))
    }
    pub fn get<T: DeserializeOwned>(
        &self,
        namespace: &str,
        hash: &str,
        ttl: u64,
    ) -> Result<Option<T>> {
        let path = self.path(namespace, hash)?;
        if !path.exists() {
            return Ok(None);
        }
        discovery::safe_path(&path)?;
        let file = File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > MAX_ENTRY as u64 {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        file.take((MAX_ENTRY + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > MAX_ENTRY {
            return Ok(None);
        }
        let Ok(entry) = serde_json::from_slice::<Entry>(&bytes) else {
            return Ok(None);
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        if entry.created > now || now - entry.created > ttl {
            return Ok(None);
        }
        Ok(serde_json::from_value(entry.value).ok())
    }
    pub fn put<T: Serialize>(&self, namespace: &str, hash: &str, value: &T) -> Result<()> {
        let path = self.path(namespace, hash)?;
        let mut existing = self.root.as_path();
        while !existing.exists() {
            existing = existing
                .parent()
                .ok_or("Cache root has no existing parent")?;
        }
        discovery::safe_path(existing)?;
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        discovery::safe_path(&self.root)?;
        if path.exists() {
            discovery::safe_path(&path)?;
        }
        let entry = Entry {
            created: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs(),
            value: serde_json::to_value(value).map_err(|e| e.to_string())?,
        };
        let bytes = serde_json::to_vec(&entry).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_ENTRY {
            return Err("Cache entry exceeds 2 MiB limit".into());
        }
        let temp = self
            .root
            .join(format!("{namespace}-{hash}-{}.tmp", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        if let Err(error) = fs::rename(&temp, &path) {
            let cleanup = fs::remove_file(&temp);
            return Err(format!(
                "Cache update failed: {error}; temporary cleanup: {cleanup:?}"
            ));
        }
        self.prune()
    }
    pub fn clear(&self) -> Result<()> {
        if !self.root.exists() {
            return Ok(());
        }
        discovery::safe_path(&self.root)?;
        for item in fs::read_dir(&self.root)
            .map_err(|e| e.to_string())?
            .take(256)
        {
            let item = item.map_err(|e| e.to_string())?;
            let name = item.file_name().to_string_lossy().into_owned();
            if name
                .strip_prefix("ai-")
                .or_else(|| name.strip_prefix("metadata-"))
                .or_else(|| name.strip_prefix("artwork-"))
                .and_then(|s| s.strip_suffix(".json"))
                .is_some_and(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
            {
                discovery::safe_path(&item.path())?;
                fs::remove_file(item.path()).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    fn prune(&self) -> Result<()> {
        let mut files = Vec::new();
        for item in fs::read_dir(&self.root)
            .map_err(|e| e.to_string())?
            .take(256)
        {
            let item = item.map_err(|e| e.to_string())?;
            let path = item.path();
            let name = item.file_name().to_string_lossy().into_owned();
            let valid = name
                .strip_prefix("ai-")
                .or_else(|| name.strip_prefix("metadata-"))
                .or_else(|| name.strip_prefix("artwork-"))
                .and_then(|s| s.strip_suffix(".json"))
                .is_some_and(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()));
            if valid {
                discovery::safe_path(&path)?;
                let m = item.metadata().map_err(|e| e.to_string())?;
                files.push((m.modified().ok(), m.len(), path));
            }
        }
        files.sort_by_key(|f| f.0);
        let mut total = files.iter().map(|f| f.1).sum::<u64>();
        let mut count = files.len();
        for (_, size, path) in files {
            if total <= 32 * 1024 * 1024 && count <= 96 {
                break;
            }
            fs::remove_file(path).map_err(|e| e.to_string())?;
            total = total.saturating_sub(size);
            count -= 1;
        }
        Ok(())
    }
}

pub fn report_key(path: &Path) -> Result<String> {
    discovery::safe_path(path)?;
    let inputs = discovery::inputs(path)?;
    let mut hasher = Sha256::new();
    hasher.update(b"doctor-report-v4");
    hasher.update(
        path.canonicalize()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .as_bytes(),
    );
    hasher.update(serde_json::to_vec(&inputs.versions).map_err(|e| e.to_string())?);
    for log in inputs.logs {
        discovery::safe_path(&log)?;
        hasher.update(log.to_string_lossy().as_bytes());
        let mut bytes = Vec::new();
        File::open(log)
            .map_err(|e| e.to_string())?
            .take((crate::logs::MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        hasher.update(bytes);
    }
    if let Some(folder) = inputs.mods {
        for file in discovery::children(&folder)? {
            if file
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("jar"))
            {
                discovery::safe_path(&file)?;
                let m = fs::metadata(&file).map_err(|e| e.to_string())?;
                hasher.update(file.to_string_lossy().as_bytes());
                hasher.update(m.len().to_le_bytes());
                hasher.update(
                    m.modified()
                        .map_err(|e| e.to_string())?
                        .duration_since(UNIX_EPOCH)
                        .map_err(|e| e.to_string())?
                        .as_nanos()
                        .to_le_bytes(),
                );
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Default)]
pub struct Reports {
    entries: VecDeque<(String, Report)>,
}
impl Reports {
    pub fn analyze(&mut self, path: &Path, cancel: &AtomicBool) -> Result<(Report, bool)> {
        let hash = report_key(path)?;
        if let Some((_, report)) = self.entries.iter().find(|(k, _)| k == &hash) {
            return Ok((report.clone(), true));
        }
        let report = crate::analyze_cancellable(path, cancel)?;
        if report_key(path)? == hash {
            let size = report
                .logs
                .iter()
                .flat_map(|l| &l.lines)
                .map(|l| l.text.len())
                .sum::<usize>()
                + report
                    .mods
                    .iter()
                    .flat_map(|m| &m.descriptor_lines)
                    .map(|l| l.text.len())
                    .sum::<usize>();
            if size <= 16 * 1024 * 1024 {
                if self.entries.len() == 4 {
                    self.entries.pop_front();
                }
                self.entries.push_back((hash, report.clone()));
            }
        }
        Ok((report, false))
    }
    pub fn text(&mut self, text: &str) -> Result<(Report, bool)> {
        let hash = key(text.as_bytes());
        if let Some((_, report)) = self.entries.iter().find(|(k, _)| k == &hash) {
            return Ok((report.clone(), true));
        }
        let report = crate::analyze_text(text)?;
        if self.entries.len() == 4 {
            self.entries.pop_front();
        }
        self.entries.push_back((hash, report.clone()));
        Ok((report, false))
    }
}

pub fn warning(error: &str) {
    eprintln!(
        "{}",
        crate::privacy::redact(&format!(
            "Cache unavailable; current results are still usable: {error}"
        ))
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeats_hit_but_changed_log_invalidates_and_disk_is_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("latest.log");
        fs::write(&path, b"normal").unwrap();
        let mut reports = Reports::default();
        let cancel = AtomicBool::new(false);
        assert!(!reports.analyze(&path, &cancel).unwrap().1);
        assert!(reports.analyze(&path, &cancel).unwrap().1);
        fs::write(&path, b"changed").unwrap();
        assert!(!reports.analyze(&path, &cancel).unwrap().1);
        let disk = Disk {
            root: temp.path().join("cache"),
        };
        let hash = key(b"payload");
        disk.put("ai", &hash, &vec!["safe"]).unwrap();
        assert_eq!(
            disk.get::<Vec<String>>("ai", &hash, 100).unwrap().unwrap(),
            vec!["safe"]
        );
        assert!(disk.get::<String>("ai", "../escape", 100).is_err());
        fs::write(disk.path("ai", &hash).unwrap(), b"malformed").unwrap();
        assert!(disk.get::<String>("ai", &hash, 100).unwrap().is_none());
        let stale = Entry {
            created: 1,
            value: serde_json::json!("stale"),
        };
        fs::write(
            disk.path("ai", &hash).unwrap(),
            serde_json::to_vec(&stale).unwrap(),
        )
        .unwrap();
        assert!(disk.get::<String>("ai", &hash, 100).unwrap().is_none());
        fs::write(disk.root.join("unrelated.txt"), b"keep").unwrap();
        disk.clear().unwrap();
        assert!(disk.root.join("unrelated.txt").exists());
        assert!(!disk.path("ai", &hash).unwrap().exists());
    }
}
