//! Read-only experiment planning. No downloads, mod changes or game launches.
use crate::{Result, discovery};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

#[derive(Serialize)]
pub struct Snapshot {
    pub experimental: bool,
    pub files: Vec<(String, String)>,
    pub steps: Vec<String>,
}

pub fn plan(path: &Path) -> Result<Snapshot> {
    let inputs = discovery::inputs(path)?;
    let mods = inputs
        .mods
        .ok_or("Select a folder containing mods, or a mods directory")?;
    let mut files = Vec::new();
    let mut total = 0u64;
    for path in discovery::children(&mods)? {
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("jar"))
        {
            continue;
        }
        discovery::safe_path(&path)?;
        let file = File::open(&path).map_err(|e| e.to_string())?;
        let size = file.metadata().map_err(|e| e.to_string())?.len();
        total = total.saturating_add(size);
        if size > 128 * 1024 * 1024 || total > 1024 * 1024 * 1024 || files.len() >= 512 {
            return Err("Snapshot exceeds 128 MiB/file, 1 GiB total or 512 JAR limit".into());
        }
        let mut hash = Sha256::new();
        let mut reader = file.take(size + 1);
        let mut buffer = [0; 64 * 1024];
        let mut count = 0u64;
        loop {
            let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            count += n as u64;
            hash.update(&buffer[..n]);
        }
        if count != size {
            return Err("Mod changed during snapshot; retry after launcher exits".into());
        }
        files.push((
            crate::privacy::redact(&path.file_name().unwrap_or_default().to_string_lossy()),
            format!("{:x}", hash.finalize()),
        ));
    }
    Ok(Snapshot { experimental: true, files, steps: vec![
        "This is a read-only inventory and experiment plan, not a verified disposable instance.".into(),
        "Manually create a separate disposable instance; exclude saves, accounts, tokens, server lists and personal data.".into(),
        "Record Minecraft, loader, pack versions and hashes; run a known-good baseline only after explicit launch approval.".into(),
        "After separate approval, add exactly one named mod/version from a trusted permitted source. Record source URL and expected hash.".into(),
        "Import baseline and experiment logs into the Doctor. Compare evidence and restore the disposable copy to baseline.".into(),
    ] })
}

#[cfg(test)]
mod tests {
    #[test]
    fn snapshot_is_repeatable_and_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        std::fs::create_dir(&mods).unwrap();
        let path = mods.join("a.jar");
        std::fs::write(&path, b"test bytes").unwrap();
        let first = super::plan(&mods).unwrap();
        let second = super::plan(&mods).unwrap();
        assert_eq!(first.files, second.files);
        assert_eq!(std::fs::read(&path).unwrap(), b"test bytes");
        assert_eq!(first.files[0].1.len(), 64);
    }
}
