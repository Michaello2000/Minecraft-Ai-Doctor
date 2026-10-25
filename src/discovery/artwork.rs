//! Only launcher profile metadata and explicitly named local artwork are read here.
use super::{Instance, safe_path};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

fn text(path: &Path) -> Option<String> {
    safe_path(path).ok()?;
    let file = File::open(path).ok()?;
    if file.metadata().ok()?.len() > 256 * 1024 {
        return None;
    }
    let mut text = String::new();
    file.take(256 * 1024 + 1).read_to_string(&mut text).ok()?;
    (text.len() <= 256 * 1024).then_some(text)
}
fn safe_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 160
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || "-_. ".contains(c))
        && !key.contains("..")
}
fn image(path: PathBuf) -> Option<PathBuf> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    if !["png", "webp", "jpg", "jpeg", "ico"].contains(&ext.as_str())
        || !path.is_file()
        || safe_path(&path).is_err()
    {
        return None;
    }
    (std::fs::metadata(&path).ok()?.len() <= 512 * 1024).then_some(path)
}
pub(super) fn decorate(instance: &mut Instance) {
    let mut key = None;
    if let Some(cfg) = text(&instance.path.join("instance.cfg")) {
        for line in cfg.lines() {
            if let Some((k, v)) = line.split_once('=') {
                let v = v.trim();
                if k.trim() == "name" && !v.is_empty() && v.len() <= 160 {
                    instance.name = v.into();
                }
                if k.trim() == "iconKey" && safe_key(v) {
                    key = Some(v.to_string());
                }
            }
        }
    }
    for name in [
        "icon.png",
        "icon.webp",
        "icon.jpg",
        "icon.ico",
        ".dawn/icon.png",
        ".dawn/icon.webp",
    ] {
        if let Some(icon) = image(instance.path.join(name)) {
            instance.icon = Some(icon);
            break;
        }
    }
    if instance.icon.is_none()
        && let Some(key) = key
        && let Some(root) = instance.path.parent().and_then(Path::parent)
    {
        for extension in ["png", "webp", "jpg", "jpeg", "ico"] {
            if let Some(icon) = image(root.join("icons").join(format!("{key}.{extension}"))) {
                instance.icon = Some(icon);
                break;
            }
        }
    }
    if instance.launcher == "Dawn"
        && let Some(raw) = text(&instance.path.join("profile.json"))
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw)
    {
        let profile = &json["profile"];
        if let Some(name) = profile["name"]
            .as_str()
            .filter(|n| !n.is_empty() && n.len() <= 160)
        {
            instance.name = name.into();
        }
        if instance.icon.is_none() {
            for field in ["iconPath", "icon"] {
                if let Some(name) = profile[field].as_str() {
                    let relative = Path::new(name);
                    // Metadata must never point us at arbitrary user files or another profile.
                    if relative.is_relative()
                        && relative
                            .components()
                            .all(|c| matches!(c, std::path::Component::Normal(_)))
                        && let Some(icon) = image(instance.path.join(relative))
                    {
                        instance.icon = Some(icon);
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_icons_names_and_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("instances/one")).unwrap();
        std::fs::create_dir(root.join("icons")).unwrap();
        std::fs::write(
            root.join("instances/one/instance.cfg"),
            "name=My pack\niconKey=custom\n",
        )
        .unwrap();
        std::fs::write(root.join("icons/custom.webp"), b"artwork").unwrap();
        let mut i = Instance {
            name: "one".into(),
            launcher: "Prism".into(),
            path: root.join("instances/one"),
            icon: None,
        };
        decorate(&mut i);
        assert_eq!(i.name, "My pack");
        assert_eq!(i.icon, Some(root.join("icons/custom.webp")));
        std::fs::write(
            root.join("instances/one/instance.cfg"),
            "iconKey=../../outside",
        )
        .unwrap();
        i.icon = None;
        decorate(&mut i);
        assert!(i.icon.is_none());
        std::fs::write(
            i.path.join("profile.json"),
            r#"{"profile":{"name":"Dawn pack","iconPath":"../outside.png"}}"#,
        )
        .unwrap();
        i.launcher = "Dawn".into();
        decorate(&mut i);
        assert_eq!(i.name, "Dawn pack");
        assert!(i.icon.is_none());
    }
}
