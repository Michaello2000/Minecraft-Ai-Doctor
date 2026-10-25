use crate::{Evidence, Finding, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_JAR: u64 = 128 * 1024 * 1024;
const MAX_METADATA: u64 = 256 * 1024;
const MAX_ENTRIES: usize = 20_000;
const MAX_MODS: usize = 512;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dependency {
    pub id: String,
    pub requirement: String,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mod {
    pub id: String,
    pub version: String,
    pub loader: String,
    pub source: String,
    pub descriptor: String,
    pub line: usize,
    pub dependencies: Vec<Dependency>,
    pub descriptor_lines: Vec<crate::logs::Line>,
    #[serde(default)]
    pub icon: Option<String>,
}

fn requirement(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        s.into()
    } else if let Some(array) = value.as_array() {
        array
            .iter()
            .map(requirement)
            .collect::<Vec<_>>()
            .join(" || ")
    } else {
        "unknown".into()
    }
}

fn id_line(text: &str, id: &str) -> usize {
    text.lines()
        .position(|line| line.contains(&format!("\"{id}\"")) || line.contains(&format!("'{id}'")))
        .map_or(1, |i| i + 1)
}

fn json_mods(text: &str, source: &str, descriptor: &str) -> Result<Vec<Mod>> {
    let json: Value =
        serde_json::from_str(text).map_err(|e| format!("Invalid {descriptor}: {e}"))?;
    let quilt = descriptor == "quilt.mod.json";
    let root = if quilt { &json["quilt_loader"] } else { &json };
    let id = root["id"].as_str().ok_or("Missing mod ID")?;
    if id.len() > 128 || root["version"].as_str().is_some_and(|s| s.len() > 256) {
        return Err("Mod ID/version exceeds metadata limits".into());
    }
    let mut dependencies = Vec::new();
    if quilt {
        for (field, kind) in [("depends", "required"), ("breaks", "conflict")] {
            if let Some(items) = root[field].as_array() {
                for item in items {
                    if let Some(id) = item["id"].as_str() {
                        dependencies.push(Dependency {
                            id: id.into(),
                            requirement: item
                                .get("versions")
                                .map(requirement)
                                .unwrap_or("*".into()),
                            kind: if item["optional"].as_bool() == Some(true) {
                                "optional".into()
                            } else {
                                kind.into()
                            },
                        });
                    } else {
                        dependencies.push(Dependency {
                            id: "unknown quilt dependency group".into(),
                            requirement: "unknown".into(),
                            kind: "unknown".into(),
                        });
                    }
                }
            }
        }
    } else {
        for (field, kind) in [
            ("depends", "required"),
            ("recommends", "optional"),
            ("suggests", "optional"),
            ("breaks", "conflict"),
            ("conflicts", "warning"),
        ] {
            if let Some(items) = root[field].as_object() {
                for (id, value) in items {
                    dependencies.push(Dependency {
                        id: id.clone(),
                        requirement: requirement(value),
                        kind: kind.into(),
                    });
                }
            }
        }
    }
    if json.get("jars").is_some() || root.get("jars").is_some() || root.get("provides").is_some() {
        dependencies.push(Dependency {
            id: "nested jars / provided aliases".into(),
            requirement: "unknown".into(),
            kind: "unknown".into(),
        });
    }
    if dependencies.len() > 256
        || dependencies
            .iter()
            .any(|d| d.id.len() > 128 || d.requirement.len() > 1024)
    {
        return Err("Dependency metadata exceeds item or field limits".into());
    }
    Ok(vec![Mod {
        icon: root
            .get("icon")
            .or_else(|| root.get("metadata").and_then(|m| m.get("icon")))
            .and_then(|v| {
                v.as_str().map(str::to_owned).or_else(|| {
                    v.as_object()
                        .and_then(|m| {
                            m.get("128")
                                .or_else(|| m.get("64"))
                                .or_else(|| m.values().next())
                        })
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                })
            }),
        id: id.into(),
        version: root["version"].as_str().unwrap_or("unknown").into(),
        loader: if quilt { "Quilt" } else { "Fabric" }.into(),
        source: source.into(),
        descriptor: descriptor.into(),
        line: id_line(text, id),
        dependencies,
        descriptor_lines: crate::logs::parse(descriptor.into(), text.as_bytes(), false).lines,
    }])
}

fn toml_mods(text: &str, source: &str, descriptor: &str) -> Result<Vec<Mod>> {
    let value: toml::Value =
        toml::from_str(text).map_err(|e| format!("Invalid {descriptor}: {e}"))?;
    let mut mods = Vec::new();
    for item in value
        .get("mods")
        .and_then(|m| m.as_array())
        .ok_or("Missing mods array")?
    {
        let id = item
            .get("modId")
            .and_then(|m| m.as_str())
            .ok_or("Missing modId")?;
        if id.len() > 128
            || item
                .get("version")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.len() > 256)
        {
            return Err("Mod ID/version exceeds metadata limits".into());
        }
        let mut dependencies = Vec::new();
        if let Some(items) = value
            .get("dependencies")
            .and_then(|d| d.get(id))
            .and_then(|d| d.as_array())
        {
            for item in items {
                if let Some(dep) = item.get("modId").and_then(|m| m.as_str()) {
                    let kind = match item.get("type").and_then(|t| t.as_str()) {
                        Some("incompatible") => "conflict",
                        Some("optional") | Some("discouraged") => "optional",
                        _ if item.get("mandatory").and_then(|v| v.as_bool()) == Some(false) => {
                            "optional"
                        }
                        _ => "required",
                    };
                    dependencies.push(Dependency {
                        id: dep.into(),
                        requirement: item
                            .get("versionRange")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .into(),
                        kind: kind.into(),
                    });
                }
            }
        }
        if dependencies.len() > 256
            || dependencies
                .iter()
                .any(|d| d.id.len() > 128 || d.requirement.len() > 1024)
        {
            return Err("Dependency metadata exceeds item or field limits".into());
        }
        if mods.len() >= 128 {
            return Err("JAR exceeds 128 declared mods".into());
        }
        mods.push(Mod {
            icon: item
                .get("logoFile")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            id: id.into(),
            version: item
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .into(),
            loader: if descriptor.contains("neoforge") {
                "NeoForge"
            } else {
                "Forge"
            }
            .into(),
            source: source.into(),
            descriptor: descriptor.into(),
            line: id_line(text, id),
            dependencies,
            descriptor_lines: crate::logs::parse(descriptor.into(), text.as_bytes(), false).lines,
        });
    }
    Ok(mods)
}

pub fn read_jar(path: &Path) -> Result<Vec<Mod>> {
    crate::discovery::safe_path(path)?;
    let file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_JAR {
        return Err("JAR exceeds 128 MiB limit".into());
    }
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if archive.len() > MAX_ENTRIES {
        return Err("JAR exceeds archive entry limit".into());
    }
    let mut total = 0u64;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.enclosed_name().is_none()
            || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err("Unsafe archive path or symlink".into());
        }
        total = total.saturating_add(entry.size());
        if total > 512 * 1024 * 1024 {
            return Err("Archive uncompressed size exceeds limit".into());
        }
    }
    for descriptor in [
        "fabric.mod.json",
        "quilt.mod.json",
        "META-INF/neoforge.mods.toml",
        "META-INF/mods.toml",
    ] {
        match archive.by_name(descriptor) {
            Ok(entry) => {
                if entry.size() > MAX_METADATA
                    || entry.size() > entry.compressed_size().saturating_mul(200).max(4096)
                {
                    return Err("Metadata exceeds size or compression ratio limit".into());
                }
                let mut bytes = Vec::new();
                entry
                    .take(MAX_METADATA + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() as u64 > MAX_METADATA {
                    return Err("Metadata exceeds size limit".into());
                }
                let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
                let source = path.to_string_lossy();
                return if descriptor.ends_with("json") {
                    json_mods(text, &source, descriptor)
                } else {
                    toml_mods(text, &source, descriptor)
                };
            }
            Err(zip::result::ZipError::FileNotFound) => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("No recognized top-level mod descriptor; metadata unknown".into())
}

pub fn scan(folder: &Path, cancel: &AtomicBool) -> Result<(Vec<Mod>, Vec<String>)> {
    let mut mods = Vec::new();
    let mut warnings = Vec::new();
    let files = crate::discovery::children(folder)?;
    let jars = files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jar")))
        .collect::<Vec<_>>();
    if jars.len() > MAX_MODS {
        return Err("Mods directory exceeds 512 JAR limit".into());
    }
    for path in jars {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        match read_jar(path) {
            Ok(items) => mods.extend(items),
            Err(e) => warnings.push(format!("{}: {e}", path.display())),
        }
    }
    Ok((mods, warnings))
}

fn version(value: &str) -> Option<semver::Version> {
    if value == "unknown" || value.contains('$') {
        return None;
    }
    let value = value.strip_prefix('v').unwrap_or(value);
    let parts = value.split('.').count();
    semver::Version::parse(&match parts {
        1 => format!("{value}.0.0"),
        2 => format!("{value}.0"),
        _ => value.into(),
    })
    .ok()
}

/// Conservative subset of Fabric predicates and Maven intervals; unsupported syntax is unknown.
pub fn matches(requirement: &str, installed: &str) -> Option<bool> {
    let requirement = requirement.trim();
    if requirement == "*" {
        return Some(true);
    }
    if requirement.contains("||") {
        let results = requirement
            .split("||")
            .map(|r| matches(r, installed))
            .collect::<Vec<_>>();
        return if results.contains(&Some(true)) {
            Some(true)
        } else if results.contains(&None) {
            None
        } else {
            Some(false)
        };
    }
    let actual = version(installed)?;
    if requirement.starts_with(['[', '(']) {
        let inclusive_start = requirement.starts_with('[');
        let inclusive_end = requirement.ends_with(']');
        let inner = requirement.get(1..requirement.len().checked_sub(1)?)?;
        if let Some((low, high)) = inner.split_once(',') {
            if inner.matches(',').count() != 1 {
                return None;
            }
            let lower = if low.trim().is_empty() {
                true
            } else {
                let v = version(low.trim())?;
                if inclusive_start {
                    actual >= v
                } else {
                    actual > v
                }
            };
            let upper = if high.trim().is_empty() {
                true
            } else {
                let v = version(high.trim())?;
                if inclusive_end {
                    actual <= v
                } else {
                    actual < v
                }
            };
            return Some(lower && upper);
        }
        return Some(inclusive_start && inclusive_end && actual == version(inner)?);
    }
    if requirement.contains(' ') {
        let results = requirement
            .split_whitespace()
            .map(|r| matches(r, installed))
            .collect::<Vec<_>>();
        return if results.contains(&Some(false)) {
            Some(false)
        } else if results.contains(&None) {
            None
        } else {
            Some(true)
        };
    }
    for op in [">=", "<=", ">", "<", "="] {
        if let Some(raw) = requirement.strip_prefix(op) {
            let target = version(raw)?;
            return Some(match op {
                ">=" => actual >= target,
                "<=" => actual <= target,
                ">" => actual > target,
                "<" => actual < target,
                _ => actual == target,
            });
        }
    }
    if requirement.contains(['*', 'x', '^', '~']) {
        return semver::VersionReq::parse(requirement)
            .ok()
            .map(|r| r.matches(&actual));
    }
    Some(actual == version(requirement)?)
}

pub fn compare(mods: &[Mod], versions: &BTreeMap<String, String>) -> Vec<Finding> {
    let mut installed = versions.clone();
    let mut sources: BTreeMap<&str, Vec<&Mod>> = BTreeMap::new();
    let mut findings = Vec::new();
    for m in mods {
        installed.insert(m.id.clone(), m.version.clone());
        sources.entry(&m.id).or_default().push(m);
    }
    for (id, entries) in &sources {
        if entries.len() > 1 {
            findings.push(Finding::new(
                "duplicate",
                "error",
                format!("Duplicate mod ID: {id}"),
                "Review the listed JARs; test keeping one intended version in a disposable copy.",
                entries
                    .iter()
                    .map(|m| metadata_evidence(m, &m.id))
                    .collect(),
            ));
        }
    }
    for m in mods {
        for dependency in &m.dependencies {
            let actual = installed.get(&dependency.id);
            let builtin = [
                "minecraft",
                "java",
                "fabricloader",
                "quilt_loader",
                "forge",
                "neoforge",
            ]
            .contains(&dependency.id.as_str());
            let (severity, message) = match (dependency.kind.as_str(), actual) {
                ("unknown", _) => ("info", "Unsupported metadata; resolution unknown"),
                ("required", None) if builtin => ("info", "Runtime version unknown"),
                ("required", None) => (
                    "warning",
                    "Required ID not found among top-level descriptors; nested JARs or aliases may provide it",
                ),
                ("required", Some(v)) => match matches(&dependency.requirement, v) {
                    Some(false) => ("error", "Declared version requirement not met"),
                    None => ("info", "Version predicate unknown"),
                    Some(true) => continue,
                },
                ("conflict" | "warning", Some(v)) => match matches(&dependency.requirement, v) {
                    Some(true) => ("warning", "Declared incompatibility matches"),
                    None => ("info", "Conflict version predicate unknown"),
                    Some(false) => continue,
                },
                _ => continue,
            };
            let action = format!(
                "Declared {} requirement: {} {}. Installed: {}. Review the descriptor and confirm behavior in a disposable instance.",
                dependency.kind,
                dependency.id,
                dependency.requirement,
                actual.map_or("unknown", String::as_str)
            );
            findings.push(Finding::new(
                "compatibility",
                severity,
                format!("{} → {}: {message}", m.id, dependency.id),
                &action,
                vec![metadata_evidence(m, &dependency.id)],
            ));
        }
        let known_loader = [
            ("fabricloader", "Fabric"),
            ("quilt_loader", "Quilt"),
            ("forge", "Forge"),
            ("neoforge", "NeoForge"),
        ]
        .into_iter()
        .find(|(id, _)| versions.contains_key(*id));
        if let Some((_, loader)) = known_loader
            && m.loader != loader
            && !(loader == "Quilt" && m.loader == "Fabric")
        {
            findings.push(Finding::new("compatibility","warning",format!("{} declares {} metadata in a {loader} instance",m.id,m.loader),"Verify that this JAR supports the selected loader. Some multi-loader packages or compatibility layers need manual confirmation.",vec![metadata_evidence(m,&m.id)]));
        }
    }
    findings
}

fn metadata_evidence(m: &Mod, key: &str) -> Evidence {
    let line = m
        .descriptor_lines
        .iter()
        .find(|line| {
            line.text.contains(&format!("\"{key}\"")) || line.text.contains(&format!("'{key}'"))
        })
        .or_else(|| m.descriptor_lines.get(m.line.saturating_sub(1)));
    Evidence {
        source: format!("{}!{}", m.source, m.descriptor),
        line: line.map_or(m.line, |l| l.number),
        text: line.map_or_else(String::new, |l| l.text.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn jar(path: &Path, name: &str, text: &str) {
        let mut archive = zip::ZipWriter::new(File::create(path).unwrap());
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(text.as_bytes()).unwrap();
        archive.finish().unwrap();
    }
    #[test]
    fn predicates_are_conservative() {
        for (r, v, result) in [
            ("1.21", "1.21.1", Some(false)),
            (">=1.20 <1.22", "1.21.1", Some(true)),
            ("[1.20,1.22)", "1.22", Some(false)),
            ("1.21.x", "1.21.1", Some(true)),
            ("${file.jarVersion}", "1.2", None),
            ("*", "custom", Some(true)),
            ("[1.21]", "1.21", Some(true)),
        ] {
            assert_eq!(matches(r, v), result, "{r} {v}");
        }
    }
    #[test]
    fn archives_duplicates_and_unsafe_paths() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test.jar");
        jar(
            &path,
            "fabric.mod.json",
            r#"{"schemaVersion":1,"id":"example","version":"1.0.0","depends":{"minecraft":"1.21.x","missing":">=2"},"breaks":{"other":"*"}}"#,
        );
        let mods = read_jar(&path).unwrap();
        assert_eq!(mods[0].dependencies.len(), 3);
        let findings = compare(&[mods[0].clone(), mods[0].clone()], &BTreeMap::new());
        assert!(findings.iter().any(|f| f.code == "duplicate"));
        assert!(findings.iter().any(|f| f.title.contains("missing")));
        jar(&path, "../fabric.mod.json", "{}");
        assert!(read_jar(&path).is_err());
        std::fs::write(&path, b"not a zip").unwrap();
        assert!(read_jar(&path).is_err());
    }
    #[test]
    fn quilt_and_forge() {
        let q = json_mods(r#"{"quilt_loader":{"id":"q","version":"1","depends":[{"id":"minecraft","versions":"*"}]}}"#, "q.jar", "quilt.mod.json").unwrap();
        assert_eq!(q[0].loader, "Quilt");
        let f = toml_mods("[[mods]]\nmodId='f'\nversion='1'\n[[dependencies.f]]\nmodId='minecraft'\nmandatory=true\nversionRange='[1.21,1.22)'", "f.jar", "META-INF/mods.toml").unwrap();
        assert_eq!(f[0].dependencies[0].id, "minecraft");
    }

    #[test]
    fn metadata_references_are_actual_source_lines() {
        let text = "{\n  \"id\": \"example\",\n  \"version\": \"1\",\n  \"depends\": {\n    \"minecraft\": \"1.20\"\n  }\n}";
        let mods = json_mods(text, "test.jar", "fabric.mod.json").unwrap();
        let versions = BTreeMap::from([
            ("minecraft".into(), "1.21".into()),
            ("forge".into(), "1".into()),
        ]);
        let findings = compare(&mods, &versions);
        assert!(findings.len() >= 2);
        for finding in &findings {
            for e in &finding.evidence {
                assert_eq!(text.lines().nth(e.line - 1).unwrap(), e.text);
            }
        }
        assert!(
            findings
                .iter()
                .any(|f| f.evidence.iter().any(|e| e.line == 5))
        );
    }

    #[test]
    fn oversized_and_malformed_metadata_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("large.jar");
        jar(
            &path,
            "fabric.mod.json",
            &" ".repeat(MAX_METADATA as usize + 1),
        );
        assert!(read_jar(&path).unwrap_err().contains("Metadata exceeds"));
        jar(&path, "fabric.mod.json", "{malformed");
        assert!(read_jar(&path).is_err());
        let mut archive = zip::ZipWriter::new(File::create(&path).unwrap());
        archive
            .start_file(
                "fabric.mod.json",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        archive.write_all(&vec![b' '; 100_000]).unwrap();
        archive.finish().unwrap();
        assert!(read_jar(&path).unwrap_err().contains("compression ratio"));
    }
}
