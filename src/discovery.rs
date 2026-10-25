use crate::Result;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const MAX_ENTRIES: usize = 2048;
mod artwork;
#[derive(Clone, Debug, Serialize)]
pub struct Instance {
    #[serde(skip)]
    pub icon: Option<PathBuf>,
    pub name: String,
    pub launcher: String,
    pub path: PathBuf,
}

pub struct Inputs {
    pub logs: Vec<PathBuf>,
    pub mods: Option<PathBuf>,
    pub versions: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}

// Check every ancestor: a plain file inside a junction can otherwise escape the selected tree.
pub fn safe_path(path: &Path) -> Result<()> {
    let absolute = std::path::absolute(path).map_err(|e| e.to_string())?;
    for ancestor in absolute.ancestors() {
        let metadata =
            fs::symlink_metadata(ancestor).map_err(|e| format!("{}: {e}", ancestor.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(
                "Symbolic links and junctions are not accepted; select an ordinary directory."
                    .into(),
            );
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(
                    "Reparse points are not accepted; select an ordinary directory.".into(),
                );
            }
        }
    }
    Ok(())
}

pub fn children(path: &Path) -> Result<Vec<PathBuf>> {
    safe_path(path)?;
    let mut files = Vec::new();
    for item in fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .take(MAX_ENTRIES + 1)
    {
        if files.len() == MAX_ENTRIES {
            return Err("Directory exceeds the 2048-entry safety limit.".into());
        }
        files.push(item.map_err(|e| e.to_string())?.path());
    }
    files.sort();
    Ok(files)
}

pub fn discover(custom: Option<&Path>) -> Vec<Instance> {
    let mut roots = Vec::new();
    if let Some(root) = custom {
        roots.push(("Custom", root.to_path_buf()));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push((
            "Prism",
            PathBuf::from(appdata).join("PrismLauncher/instances"),
        ));
        roots.push((
            "Dawn",
            PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join(".dawn/profiles"),
        ));
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        roots.push(("Lunar", PathBuf::from(home).join(".lunarclient/profiles")));
    }
    let mut instances = Vec::new();
    for (launcher, root) in roots {
        if let Ok(entries) = children(&root) {
            for path in entries.into_iter().filter(|p| {
                p.is_dir()
                    && !p
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            }) {
                if safe_path(&path).is_ok() {
                    let mut instance = Instance {
                        icon: None,
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        launcher: launcher.into(),
                        path,
                    };
                    artwork::decorate(&mut instance);
                    instances.push(instance);
                }
            }
        }
    }
    instances
}

pub const LAUNCHERS: [&str; 5] = ["All", "Prism", "Lunar", "Dawn", "Custom"];

pub fn hook_path() -> Result<PathBuf> {
    std::env::var_os("INST_MC_DIR")
        .or_else(|| std::env::var_os("INST_DIR"))
        .map(PathBuf::from)
        .ok_or_else(|| {
            "Prism hook environment is missing INST_MC_DIR and INST_DIR. Use manual import.".into()
        })
}

pub fn inputs(path: &Path) -> Result<Inputs> {
    let mut result = Inputs {
        logs: vec![],
        mods: None,
        versions: BTreeMap::new(),
        warnings: vec![],
    };
    if path.is_file() {
        let extension = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if !["log", "txt"].contains(&extension.as_str()) {
            return Err("Select a .log or .txt file.".into());
        }
        result.logs.push(path.into());
        return Ok(result);
    }
    if !path.is_dir() {
        return Err("Import path is not a file or directory.".into());
    }
    let root = [path.join(".minecraft"), path.join("minecraft")]
        .into_iter()
        .find(|p| p.is_dir())
        .unwrap_or_else(|| path.into());
    safe_path(&root)?;
    if path
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("mods"))
    {
        result.mods = Some(path.into());
    } else if root.join("mods").is_dir() {
        result.mods = Some(root.join("mods"));
    }
    for file in [
        root.join("logs/latest.log"),
        root.join("latest.log"),
        root.join("logs/ichor-boot.log"),
        root.join("launcher.log"),
    ] {
        if file.is_file() {
            result.logs.push(file);
        }
    }
    // Import only the newest crash report to avoid diagnosing every historical crash as current.
    let crash_dir = root.join("crash-reports");
    if crash_dir.is_dir() {
        match children(&crash_dir) {
            Ok(files) => {
                if let Some(file) = files
                    .into_iter()
                    .filter(|p| p.extension().is_some_and(|e| e == "txt"))
                    .max_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
                {
                    result.warnings.push("Newest crash report included; it may belong to an earlier launch. Compare timestamps with latest.log.".into());
                    result.logs.push(file);
                }
            }
            Err(e) => result.warnings.push(e),
        }
    }
    let metadata = if path.join("mmc-pack.json").is_file() {
        path.join("mmc-pack.json")
    } else if path
        .file_name()
        .is_some_and(|n| n == "minecraft" || n == ".minecraft")
    {
        path.parent().unwrap_or(path).join("mmc-pack.json")
    } else {
        path.join("mmc-pack.json")
    };
    if metadata.is_file() {
        match crate::logs::read(&metadata).and_then(|log| {
            if log.truncated {
                return Err("Prism version metadata was truncated".into());
            }
            serde_json::from_str::<serde_json::Value>(
                &log.lines
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
            .map_err(|e| e.to_string())
        }) {
            Ok(value) => {
                if let Some(components) = value["components"].as_array() {
                    for component in components {
                        if let (Some(uid), Some(version)) =
                            (component["uid"].as_str(), component["version"].as_str())
                        {
                            let id = match uid {
                                "net.minecraft" => "minecraft",
                                "net.fabricmc.fabric-loader" => "fabricloader",
                                "org.quiltmc.quilt-loader" => "quilt_loader",
                                "net.minecraftforge" => "forge",
                                "net.neoforged" => "neoforge",
                                _ => continue,
                            };
                            result.versions.insert(id.into(), version.into());
                        }
                    }
                }
            }
            Err(e) => result
                .warnings
                .push(format!("Cannot read Prism version metadata: {e}")),
        }
    }
    Ok(result)
}
