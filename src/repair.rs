//! Reviewed, journaled changes in an app-owned mod-only copy. Source instances stay read-only.
use crate::{Report, Result, discovery, mods};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
mod download;

const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_TOTAL: u64 = 1024 * 1024 * 1024;
const JOURNAL: &str = "doctor-repair.json";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceFile {
    pub name: String,
    pub sha256: String,
    pub size: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub file: String,
    pub version: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Duplicate {
    pub mod_id: String,
    pub keep: String,
    pub choices: Vec<Candidate>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Download {
    pub url: String,
    pub filename: String,
    pub size: u64,
    pub sha512: String,
    pub project_id: String,
    pub release_id: String,
    pub mod_id: String,
    pub version: String,
    pub minecraft: String,
    pub loader: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Replacement {
    pub old_files: Vec<String>,
    pub download: Download,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub source_mods: PathBuf,
    pub files: Vec<SourceFile>,
    pub duplicates: Vec<Duplicate>,
    pub replacement: Option<Replacement>,
    pub notes: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Change {
    file: String,
    before: Option<String>,
    after: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Journal {
    format: u32,
    id: String,
    state: String,
    changes: Vec<Change>,
}
#[derive(Clone, Debug)]
pub struct Receipt {
    pub directory: PathBuf,
    pub changes: usize,
    pub rolled_back: bool,
    pub interrupted: bool,
}

pub fn workspace() -> PathBuf {
    let root = crate::cache::Disk::user().root;
    root.parent().unwrap_or(&root).join("repair-copies-v1")
}
pub fn apply_reviewed(
    plan: &Plan,
    local: Option<&Path>,
    allow_download: bool,
    cancel: &AtomicBool,
) -> Result<Receipt> {
    let workspace = workspace();
    if allow_download {
        ensure_workspace(&workspace, &plan.source_mods)?;
        check_capacity(&workspace, plan)?;
        let replacement = plan
            .replacement
            .as_ref()
            .ok_or("No reviewed replacement to download")?;
        let staged = download::stage(&workspace, &replacement.download, cancel)?;
        apply(&workspace, plan, Some(staged.path()), cancel)
    } else {
        apply(&workspace, plan, local, cancel)
    }
}
pub fn safe_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 200
        || name.starts_with('.')
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
    {
        return false;
    }
    let base = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    !["CON", "PRN", "AUX", "NUL"].contains(&base.as_str())
        && !(base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.ends_with(['1', '2', '3', '4', '5', '6', '7', '8', '9']))
}
pub fn hash_file(path: &Path) -> Result<String> {
    discovery::safe_path(path)?;
    let file = File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Expected an ordinary JAR file".into());
    }
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size > MAX_FILE {
        return Err("Repair input exceeds 128 MiB".into());
    }
    let mut reader = file.take(MAX_FILE + 1);
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0; 65536];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > MAX_FILE {
            return Err("Repair file grew beyond limit".into());
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn inventory(folder: &Path) -> Result<Vec<SourceFile>> {
    let mut files = Vec::new();
    let mut total = 0;
    for path in discovery::children(folder)? {
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("jar"))
        {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if !safe_name(&name) {
            return Err("An unsafe JAR filename prevents automatic repair".into());
        }
        let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        total += size;
        if total > MAX_TOTAL || files.len() >= 512 {
            return Err("Repair copy exceeds 1 GiB or 512 JAR limit".into());
        }
        files.push(SourceFile {
            name,
            sha256: hash_file(&path)?,
            size,
        });
    }
    Ok(files)
}

pub fn plan(
    report: &Report,
    ai_indices: Option<&[usize]>,
    replacement: Option<Replacement>,
) -> Result<Plan> {
    let first=report.mods.first().ok_or("Import an instance or mods folder before proposing file changes. Log-only and pasted input remain advisory.")?;
    let folder = Path::new(&first.source)
        .parent()
        .ok_or("Missing mods folder")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if report.mods.iter().any(|m| {
        Path::new(&m.source)
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .as_ref()
            != Some(&folder)
    }) {
        return Err("Mod sources are not one ordinary directory".into());
    }
    let files = inventory(&folder)?;
    let (fresh, _) = mods::scan(&folder, &AtomicBool::new(false))?;
    if metadata_snapshot(&fresh)? != metadata_snapshot(&report.mods)? {
        return Err(
            "Mod metadata changed since analysis. Analyze again before reviewing a repair.".into(),
        );
    }
    let mut duplicates = Vec::new();
    let mut notes = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for (index, finding) in report.findings.iter().enumerate() {
        if ai_indices.is_some_and(|indices| !indices.contains(&index)) {
            continue;
        }
        if finding.code != "duplicate" {
            continue;
        }
        let Some(id) = finding.title.strip_prefix("Duplicate mod ID: ") else {
            continue;
        };
        if !ids.insert(id) {
            continue;
        }
        let matching = report
            .mods
            .iter()
            .filter(|m| m.id == id)
            .collect::<Vec<_>>();
        if matching.len() < 2 {
            continue;
        }
        if matching.iter().any(|m| {
            report
                .mods
                .iter()
                .any(|other| other.source == m.source && other.id != m.id)
        }) {
            notes.push(format!("{id}: shared multi-mod JAR needs manual review."));
            continue;
        }
        let mut choices = matching
            .iter()
            .map(|m| Candidate {
                file: Path::new(&m.source)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                version: m.version.clone(),
            })
            .collect::<Vec<_>>();
        choices.sort_by(|a, b| {
            semver::Version::parse(&b.version)
                .ok()
                .cmp(&semver::Version::parse(&a.version).ok())
                .then(a.file.cmp(&b.file))
        });
        choices.dedup_by(|a, b| a.file == b.file);
        if choices.len() > 1 {
            duplicates.push(Duplicate {
                mod_id: id.into(),
                keep: choices[0].file.clone(),
                choices,
            });
        }
    }
    if let Some(replacement) = &replacement {
        validate_download(&replacement.download)?;
        if replacement
            .old_files
            .iter()
            .any(|name| !files.iter().any(|f| &f.name == name))
        {
            return Err("Replacement references a JAR not in this analysis".into());
        }
        duplicates.retain(|d| d.mod_id != replacement.download.mod_id);
    }
    notes.push("Only JARs are copied. Worlds, accounts, launcher hooks and personal configuration are excluded. This is a repaired mod inventory, not a launched or complete cloned launcher instance.".into());
    notes.push("Keep-version choices are proposals. Review dependency requirements; applying them does not prove the game will launch.".into());
    Ok(Plan {
        source_mods: folder,
        files,
        duplicates,
        replacement,
        notes,
    })
}

fn metadata_snapshot(mods: &[mods::Mod]) -> Result<Vec<String>> {
    let mut snapshot = Vec::new();
    for m in mods {
        let mut m = m.clone();
        m.source = Path::new(&m.source)
            .file_name()
            .ok_or("Invalid mod source filename")?
            .to_string_lossy()
            .into_owned();
        snapshot.push(serde_json::to_string(&m).map_err(|e| e.to_string())?);
    }
    snapshot.sort();
    Ok(snapshot)
}

pub fn validate_download(download: &Download) -> Result<()> {
    let url =
        reqwest::Url::parse(&download.url).map_err(|_| "Invalid mod download URL".to_string())?;
    if url.scheme() != "https"
        || url.host_str() != Some("cdn.modrinth.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Automatic downloads require the direct HTTPS Modrinth CDN URL".into());
    }
    if !safe_name(&download.filename)
        || !download.filename.to_ascii_lowercase().ends_with(".jar")
        || download.size > MAX_FILE
        || download.size == 0
        || download.sha512.len() != 128
        || !download.sha512.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err("Download metadata lacks a safe filename, size or SHA-512".into());
    }
    if download.project_id.is_empty()
        || download.release_id.is_empty()
        || download.mod_id.is_empty()
        || download.version.is_empty()
        || download.minecraft.is_empty()
        || !["fabric", "quilt", "forge", "neoforge"].contains(&download.loader.as_str())
    {
        return Err(
            "Download needs a confirmed project, release, mod identity, game and loader".into(),
        );
    }
    Ok(())
}

fn ensure_workspace(workspace: &Path, source: &Path) -> Result<()> {
    if std::path::absolute(workspace)
        .map_err(|e| e.to_string())?
        .starts_with(source)
    {
        return Err("Repair workspace must be outside the source instance".into());
    }
    let mut ancestor = workspace;
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or("Repair workspace has no parent")?;
    }
    discovery::safe_path(ancestor)?;
    fs::create_dir_all(workspace).map_err(|e| e.to_string())?;
    discovery::safe_path(workspace)?;
    if workspace
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(source)
    {
        return Err("Repair workspace must be outside the source instance".into());
    }
    Ok(())
}
fn write_journal(root: &Path, journal: &Journal) -> Result<()> {
    discovery::safe_path(root)?;
    let path = root.join(JOURNAL);
    if path.exists() {
        discovery::safe_path(&path)?;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temp = root.join(format!("journal-next-{}-{stamp}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(journal).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(temp, path).map_err(|e| e.to_string())
}
fn validate_copy(workspace: &Path, root: &Path) -> Result<Journal> {
    discovery::safe_path(workspace)?;
    discovery::safe_path(root)?;
    let workspace = workspace.canonicalize().map_err(|e| e.to_string())?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if root.parent() != Some(workspace.as_path())
        || !root
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("repair-"))
    {
        return Err("Rollback is restricted to this app's repair-copy workspace".into());
    }
    let path = root.join(JOURNAL);
    discovery::safe_path(&path)?;
    let file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 1024 * 1024 {
        return Err("Repair journal exceeds limit".into());
    }
    let journal: Journal = serde_json::from_reader(file.take(1024 * 1024 + 1))
        .map_err(|_| "Invalid repair journal".to_string())?;
    let mut names = std::collections::BTreeSet::new();
    if journal.format != 1
        || !["copying", "applying", "applied", "rolled_back"].contains(&journal.state.as_str())
        || journal.id != root.file_name().unwrap_or_default().to_string_lossy()
        || journal.changes.len() > 512
        || journal.changes.iter().any(|c| {
            !safe_name(&c.file)
                || !names.insert(c.file.to_ascii_lowercase())
                || !c.file.to_ascii_lowercase().ends_with(".jar")
                || c.before
                    .iter()
                    .chain(c.after.iter())
                    .any(|h| h.len() != 64 || !h.chars().all(|c| c.is_ascii_hexdigit()))
        })
    {
        return Err("Unsafe repair journal".into());
    }
    discovery::safe_path(&root.join("mods"))?;
    discovery::safe_path(&root.join("backup"))?;
    Ok(journal)
}

pub fn apply(
    workspace: &Path,
    plan: &Plan,
    staged: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<Receipt> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled before creating a repair copy".into());
    }
    ensure_workspace(workspace, &plan.source_mods)?;
    check_capacity(workspace, plan)?;
    let current = inventory(&plan.source_mods)?;
    if serde_json::to_string(&current).map_err(|e| e.to_string())?
        != serde_json::to_string(&plan.files).map_err(|e| e.to_string())?
    {
        return Err("Source mods changed since review. Analyze and review a new plan.".into());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let id = format!(
        "repair-{}",
        &crate::cache::key(format!("{now}:{}", std::process::id()).as_bytes())[..24]
    );
    let root = workspace.join(&id);
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    fs::create_dir(root.join("mods")).map_err(|e| e.to_string())?;
    fs::create_dir(root.join("backup")).map_err(|e| e.to_string())?;
    let mut changes = Vec::<Change>::new();
    for group in &plan.duplicates {
        if !group.choices.iter().any(|c| c.file == group.keep) {
            return Err("Invalid keep-version selection".into());
        }
        for candidate in &group.choices {
            if candidate.file != group.keep {
                let file = plan
                    .files
                    .iter()
                    .find(|f| f.name == candidate.file)
                    .ok_or("Unknown duplicate filename")?;
                if !changes.iter().any(|c| c.file == file.name) {
                    changes.push(Change {
                        file: file.name.clone(),
                        before: Some(file.sha256.clone()),
                        after: None,
                    });
                }
            }
        }
    }
    if let Some(replacement) = &plan.replacement {
        if plan.files.iter().any(|f| {
            f.name.eq_ignore_ascii_case(&replacement.download.filename)
                && f.name != replacement.download.filename
        }) {
            return Err("Case-only replacement filenames require manual review".into());
        }
        let staged = staged.ok_or("Verified replacement JAR was not staged")?;
        verify_replacement(staged, &replacement.download)?;
        for name in &replacement.old_files {
            let source = plan
                .files
                .iter()
                .find(|f| &f.name == name)
                .ok_or("Unknown replacement source")?;
            if changes.iter().any(|c| c.file == *name) {
                return Err("Two repair operations target one JAR".into());
            }
            changes.push(Change {
                file: name.clone(),
                before: Some(source.sha256.clone()),
                after: None,
            });
        }
        let filename = &replacement.download.filename;
        if let Some(change) = changes.iter_mut().find(|c| c.file == *filename) {
            change.after = Some(hash_file(staged)?);
        } else {
            if plan.files.iter().any(|f| f.name == *filename) {
                return Err("Replacement filename would overwrite an unrelated mod".into());
            }
            changes.push(Change {
                file: filename.clone(),
                before: None,
                after: Some(hash_file(staged)?),
            });
        }
    }
    if changes.is_empty() {
        return Err(
            "No verified automatic changes are selected. Unsupported findings need manual review."
                .into(),
        );
    }
    let mut journal = Journal {
        format: 1,
        id,
        state: "copying".into(),
        changes,
    };
    write_journal(&root, &journal)?;
    for file in &plan.files {
        if cancel.load(Ordering::Relaxed) {
            return Err(format!(
                "Cancelled before applying changes. Incomplete copy: {}",
                root.display()
            ));
        }
        let source = plan.source_mods.join(&file.name);
        let destination = root.join("mods").join(&file.name);
        let mut input = File::open(&source).map_err(|e| e.to_string())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|e| e.to_string())?;
        std::io::copy(
            &mut std::io::Read::by_ref(&mut input).take(MAX_FILE + 1),
            &mut output,
        )
        .map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        if hash_file(&destination)? != file.sha256 {
            return Err("Source changed during copy; no changes applied".into());
        }
    }
    journal.state = "applying".into();
    write_journal(&root, &journal)?;
    let operation = (|| -> Result<()> {
        for change in &journal.changes {
            discovery::safe_path(&root.join("mods"))?;
            discovery::safe_path(&root.join("backup"))?;
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled during repair".into());
            }
            if let Some(before) = &change.before {
                let file = root.join("mods").join(&change.file);
                if hash_file(&file)? != *before {
                    return Err("Repair copy changed during apply".into());
                }
                fs::rename(&file, root.join("backup").join(&change.file))
                    .map_err(|e| e.to_string())?;
            }
            if change.after.is_some() {
                let staged = staged.ok_or("Missing staged replacement")?;
                let mut input = File::open(staged).map_err(|e| e.to_string())?;
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(root.join("mods").join(&change.file))
                    .map_err(|e| e.to_string())?;
                std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
                output.sync_all().map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })();
    if let Err(error) = operation {
        let rollback = rollback(workspace, &root);
        return Err(format!("Repair stopped: {error}. Rollback: {rollback:?}"));
    }
    journal.state = "applied".into();
    write_journal(&root, &journal)?;
    Ok(Receipt {
        directory: root,
        changes: journal.changes.len(),
        rolled_back: false,
        interrupted: false,
    })
}

pub fn rollback(workspace: &Path, root: &Path) -> Result<Receipt> {
    let mut journal = validate_copy(workspace, root)?;
    if journal.state == "rolled_back" {
        return Ok(Receipt {
            directory: root.into(),
            changes: 0,
            rolled_back: true,
            interrupted: false,
        });
    }
    if !["applied", "applying"].contains(&journal.state.as_str()) {
        return Err("Copy did not finish staging; its source instance is unchanged".into());
    }
    // Preflight every file before changing any: never overwrite a user's later edits.
    for change in &journal.changes {
        let file = root.join("mods").join(&change.file);
        let backup = root.join("backup").join(&change.file);
        if backup.exists() && change.before.as_ref() != Some(&hash_file(&backup)?) {
            return Err("Backup integrity check failed".into());
        }
        if file.exists() {
            let hash = hash_file(&file)?;
            let expected = if backup.exists() || change.before.is_none() {
                change.after.as_ref()
            } else {
                change.before.as_ref()
            };
            if expected != Some(&hash) {
                return Err(
                    "A repaired file was changed after apply. Rollback refused to overwrite it."
                        .into(),
                );
            }
        } else if change.before.is_some() && !backup.exists() {
            return Err("A required original and its backup are both missing".into());
        }
    }
    for change in journal.changes.iter().rev() {
        discovery::safe_path(&root.join("mods"))?;
        discovery::safe_path(&root.join("backup"))?;
        let file = root.join("mods").join(&change.file);
        let backup = root.join("backup").join(&change.file);
        if backup.exists() {
            if file.exists() {
                fs::remove_file(&file).map_err(|e| e.to_string())?;
            }
            fs::rename(backup, file).map_err(|e| e.to_string())?;
        } else if change.before.is_none() && file.exists() {
            fs::remove_file(file).map_err(|e| e.to_string())?;
        }
    }
    journal.state = "rolled_back".into();
    write_journal(root, &journal)?;
    Ok(Receipt {
        directory: root.into(),
        changes: journal.changes.len(),
        rolled_back: true,
        interrupted: false,
    })
}

pub fn verify_replacement(path: &Path, download: &Download) -> Result<()> {
    validate_download(download)?;
    discovery::safe_path(path)?;
    let file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != download.size {
        return Err("Downloaded mod size does not match metadata".into());
    }
    let mut hash = Sha512::new();
    let mut reader = file.take(MAX_FILE + 1);
    let mut buffer = [0; 65536];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if format!("{:x}", hash.finalize()) != download.sha512.to_ascii_lowercase() {
        return Err("Downloaded mod SHA-512 verification failed".into());
    }
    let metadata = mods::read_jar(path)?;
    let m = metadata
        .iter()
        .find(|m| m.id == download.mod_id)
        .ok_or("Replacement JAR does not declare the expected mod ID")?;
    if m.version != download.version {
        return Err("Replacement descriptor version differs from the selected release; manual review required".into());
    }
    if m.loader.to_ascii_lowercase() != download.loader {
        return Err("Replacement uses a different loader".into());
    }
    for dep in &m.dependencies {
        if dep.kind == "required"
            && dep.id == "minecraft"
            && mods::matches(&dep.requirement, &download.minecraft) != Some(true)
        {
            return Err("Replacement does not verify the selected Minecraft version".into());
        }
    }
    Ok(())
}

pub fn latest(workspace: &Path) -> Result<Option<Receipt>> {
    if !workspace.exists() {
        return Ok(None);
    }
    let mut roots = discovery::children(workspace)?
        .into_iter()
        .filter(|p| p.is_dir())
        .collect::<Vec<_>>();
    roots.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
    for root in roots.into_iter().rev().take(64) {
        if let Ok(journal) = validate_copy(workspace, &root) {
            if journal.state == "copying" {
                continue;
            }
            return Ok(Some(Receipt {
                directory: root,
                changes: journal.changes.len(),
                rolled_back: journal.state == "rolled_back",
                interrupted: journal.state == "applying",
            }));
        }
    }
    Ok(None)
}

fn check_capacity(workspace: &Path, plan: &Plan) -> Result<()> {
    let roots = discovery::children(workspace)?
        .into_iter()
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("repair-"))
        })
        .collect::<Vec<_>>();
    if roots.len() >= 16 {
        return Err("Repair-copy limit reached (16). Review and remove old copies manually before creating another; backups are never deleted automatically.".into());
    }
    let mut size = plan.files.iter().map(|f| f.size).sum::<u64>()
        + plan.replacement.as_ref().map_or(0, |r| r.download.size);
    for root in roots {
        for name in ["mods", "backup"] {
            let folder = root.join(name);
            if !folder.exists() {
                continue;
            }
            for file in discovery::children(&folder)? {
                discovery::safe_path(&file)?;
                if file.is_file() {
                    size =
                        size.saturating_add(fs::metadata(file).map_err(|e| e.to_string())?.len());
                }
            }
        }
    }
    if size > 4 * 1024 * 1024 * 1024 {
        return Err("Repair workspace would exceed 4 GiB. Review old copies manually; backups are never deleted automatically.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn jar(path: &Path, id: &str, version: &str) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        zip.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        write!(
            zip,
            "{{\"schemaVersion\":1,\"id\":\"{id}\",\"version\":\"{version}\"}}"
        )
        .unwrap();
        zip.finish().unwrap();
    }
    #[test]
    fn duplicate_repair_copy_and_rollback_preserve_originals() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("mods");
        fs::create_dir(&source).unwrap();
        jar(&source.join("old.jar"), "example", "1.0.0");
        jar(&source.join("new.jar"), "example", "2.0.0");
        let report = crate::analyze(&source).unwrap();
        let plan = plan(&report, None, None).unwrap();
        assert_eq!(plan.duplicates[0].keep, "new.jar");
        let original = hash_file(&source.join("old.jar")).unwrap();
        let workspace = temp.path().join("repairs");
        let receipt = apply(&workspace, &plan, None, &AtomicBool::new(false)).unwrap();
        assert!(!receipt.directory.join("mods/old.jar").exists());
        assert!(source.join("old.jar").exists());
        assert_eq!(hash_file(&source.join("old.jar")).unwrap(), original);
        rollback(&workspace, &receipt.directory).unwrap();
        assert!(receipt.directory.join("mods/old.jar").exists());
        rollback(&workspace, &receipt.directory).unwrap();
    }
    #[test]
    fn changed_files_and_path_escape_are_refused() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("mods");
        fs::create_dir(&source).unwrap();
        jar(&source.join("a.jar"), "example", "1.0.0");
        jar(&source.join("b.jar"), "example", "2.0.0");
        let report = crate::analyze(&source).unwrap();
        let plan = plan(&report, None, None).unwrap();
        let workspace = temp.path().join("repairs");
        let receipt = apply(&workspace, &plan, None, &AtomicBool::new(false)).unwrap();
        fs::write(receipt.directory.join("mods/a.jar"), b"user edit").unwrap();
        assert!(rollback(&workspace, &receipt.directory).is_err());
        assert!(rollback(&workspace, &source).is_err());
        assert!(!safe_name("../outside.jar"));
        assert!(!safe_name("CON.jar"));
        jar(&source.join("a.jar"), "example", "9.0.0");
        assert!(apply(&workspace, &plan, None, &AtomicBool::new(false)).is_err());
        assert!(
            super::plan(&report, None, None)
                .unwrap_err()
                .contains("changed since analysis")
        );
    }
    #[test]
    fn verified_replacement_and_restart_rollback() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("mods");
        fs::create_dir(&source).unwrap();
        jar(&source.join("old.jar"), "example", "1.0.0");
        let staged = temp.path().join("new.jar");
        jar(&staged, "example", "2.0.0");
        let mut download = Download {
            url: "https://cdn.modrinth.com/data/fixture/versions/v2/new.jar".into(),
            filename: "new.jar".into(),
            size: fs::metadata(&staged).unwrap().len(),
            sha512: format!("{:x}", Sha512::digest(fs::read(&staged).unwrap())),
            project_id: "fixture".into(),
            release_id: "v2".into(),
            mod_id: "example".into(),
            version: "2.0.0".into(),
            minecraft: "1.21.1".into(),
            loader: "fabric".into(),
        };
        verify_replacement(&staged, &download).unwrap();
        download.mod_id = "unrelated".into();
        assert!(verify_replacement(&staged, &download).is_err());
        download.mod_id = "example".into();
        download.url = "https://untrusted.example/new.jar".into();
        assert!(validate_download(&download).is_err());
        download.url = "https://cdn.modrinth.com/data/fixture/versions/v2/new.jar".into();
        let report = crate::analyze(&source).unwrap();
        let plan = plan(
            &report,
            None,
            Some(Replacement {
                old_files: vec!["old.jar".into()],
                download,
            }),
        )
        .unwrap();
        let workspace = temp.path().join("repairs");
        let receipt = apply(&workspace, &plan, Some(&staged), &AtomicBool::new(false)).unwrap();
        assert!(source.join("old.jar").exists());
        assert!(!source.join("new.jar").exists());
        assert!(receipt.directory.join("mods/new.jar").exists());
        assert!(!receipt.directory.join("mods/old.jar").exists());
        let mut journal = validate_copy(&workspace, &receipt.directory).unwrap();
        journal.state = "applying".into();
        write_journal(&receipt.directory, &journal).unwrap();
        assert!(latest(&workspace).unwrap().unwrap().interrupted);
        // An abandoned pre-crash temporary journal must not block recovery.
        fs::write(receipt.directory.join("journal-next.tmp"), b"partial").unwrap();
        rollback(&workspace, &receipt.directory).unwrap();
        assert!(!receipt.directory.join("mods/new.jar").exists());
        assert!(receipt.directory.join("mods/old.jar").exists());
        assert!(latest(&workspace).unwrap().unwrap().rolled_back);
        fs::write(&staged, b"corrupted").unwrap();
        assert!(verify_replacement(&staged, &plan.replacement.unwrap().download).is_err());
    }
}
