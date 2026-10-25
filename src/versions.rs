//! Explicit, metadata-only Modrinth lookups. No mod binaries or log uploads.
use crate::{Finding, Report, Result, mods, remedies};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Target {
    pub query: String,
    pub installed: String,
    pub minecraft: String,
    pub loader: String,
    pub requirement: String,
    pub version_range: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Project {
    #[serde(default)]
    pub icon_url: Option<String>,
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub author: String,
    pub project_type: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Release {
    #[serde(default)]
    pub files: Vec<ReleaseFile>,
    pub id: String,
    pub project_id: String,
    pub version_number: String,
    pub name: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub date_published: String,
    pub version_type: String,
    #[serde(default)]
    pub dependencies: Vec<ReleaseDependency>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseFile {
    pub url: String,
    pub filename: String,
    pub size: u64,
    #[serde(default)]
    pub primary: bool,
    pub hashes: ReleaseHashes,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseHashes {
    pub sha512: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReleaseDependency {
    pub dependency_type: String,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
}

static GAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Loading Minecraft ([A-Za-z0-9_.+-]+)").expect("game version pattern")
});

fn context(report: &Report, finding: &Finding) -> (String, String) {
    let primary = finding
        .evidence
        .first()
        .and_then(|e| report.logs.iter().find(|log| log.source == e.source));
    let logs = primary.map_or_else(|| report.logs.iter().collect::<Vec<_>>(), |log| vec![log]);
    let minecraft = report
        .runtime
        .get("minecraft")
        .cloned()
        .or_else(|| {
            logs.iter()
                .flat_map(|l| &l.lines)
                .find_map(|l| GAME.captures(&l.text).map(|c| c[1].into()))
        })
        .unwrap_or_default();
    let loader = [
        ("fabricloader", "fabric"),
        ("quilt_loader", "quilt"),
        ("forge", "forge"),
        ("neoforge", "neoforge"),
    ]
    .iter()
    .find(|(key, _)| report.runtime.contains_key(*key))
    .map(|(_, v)| v.to_string())
    .or_else(|| {
        let text = logs
            .iter()
            .flat_map(|l| &l.lines)
            .map(|l| l.text.to_lowercase())
            .collect::<Vec<_>>()
            .join("\n");
        if text.contains("quilt loader") {
            Some("quilt".into())
        } else if text.contains("fabric loader") || text.contains("net.fabricmc.loader") {
            Some("fabric".into())
        } else if text.contains("neoforge") {
            Some("neoforge".into())
        } else if text.contains("forge mod loader") {
            Some("forge".into())
        } else {
            None
        }
    })
    .unwrap_or_default();
    (minecraft, loader)
}

fn range(raw: &str) -> String {
    if raw.contains(" or ") {
        return raw
            .split(" or ")
            .map(|v| range(v.trim().strip_prefix("version ").unwrap_or(v.trim())))
            .collect::<Vec<_>>()
            .join(" || ");
    }
    raw.trim()
        .strip_suffix(" or later")
        .map_or_else(|| raw.trim().to_string(), |v| format!(">={}", v.trim()))
}

pub fn target(report: &Report, finding: &Finding) -> Option<Target> {
    let (minecraft, loader) = context(report, finding);
    if matches!(finding.code.as_str(), "dependency" | "version") {
        for evidence in &finding.evidence {
            let Some(c) = remedies::DEPENDENCY.captures(&evidence.text) else {
                continue;
            };
            let missing = c["rest"].contains("missing");
            if (finding.code == "dependency") != missing {
                continue;
            }
            let dependency = c
                .name("quoted_id")
                .or_else(|| c.name("bare_id"))
                .or_else(|| c.name("label"))?
                .as_str();
            let builtin = [
                "minecraft",
                "fabricloader",
                "quilt_loader",
                "forge",
                "neoforge",
                "java",
            ]
            .contains(&dependency);
            let changing_requirer = builtin || finding.code == "version";
            let query = if changing_requirer {
                c.name("id").or_else(|| c.name("name"))?.as_str()
            } else {
                dependency
            };
            let installed = if changing_requirer {
                c.name("mod_version")
                    .map(|v| v.as_str().to_string())
                    .unwrap_or_default()
            } else {
                report
                    .mods
                    .iter()
                    .find(|m| m.id == dependency)
                    .map(|m| m.version.clone())
                    .or_else(|| remedies::installed(&c["rest"]))
                    .unwrap_or_default()
            };
            let minecraft = if dependency == "minecraft" {
                remedies::installed(&c["rest"]).unwrap_or(minecraft)
            } else {
                minecraft
            };
            return Some(Target {
                query: query.into(),
                installed,
                minecraft,
                loader,
                requirement: format!("{} {}", dependency, &c["requirement"]),
                version_range: (!changing_requirer).then(|| range(&c["requirement"])),
            });
        }
    }
    for m in &report.mods {
        for dep in &m.dependencies {
            if finding
                .title
                .starts_with(&format!("{} → {}:", m.id, dep.id))
            {
                let builtin = [
                    "minecraft",
                    "fabricloader",
                    "quilt_loader",
                    "forge",
                    "neoforge",
                    "java",
                ]
                .contains(&dep.id.as_str());
                if dep.kind != "required" {
                    return None;
                }
                return Some(Target {
                    query: if builtin {
                        m.id.clone()
                    } else {
                        dep.id.clone()
                    },
                    installed: if builtin {
                        m.version.clone()
                    } else {
                        report
                            .mods
                            .iter()
                            .find(|m| m.id == dep.id)
                            .map(|m| m.version.clone())
                            .unwrap_or_default()
                    },
                    minecraft,
                    loader,
                    requirement: format!("{} {}", dep.id, dep.requirement),
                    version_range: (!builtin).then(|| dep.requirement.clone()),
                });
            }
        }
    }
    None
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
fn validate(target: &Target) -> Result<()> {
    if target.query.trim().is_empty() || target.query.len() > 128 {
        return Err("Enter the mod name or ID (up to 128 characters).".into());
    }
    if target.minecraft.is_empty()
        || target.minecraft.len() > 40
        || !target
            .minecraft
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
    {
        return Err("Enter the exact Minecraft version before looking up releases.".into());
    }
    if !["fabric", "quilt", "forge", "neoforge"].contains(&target.loader.as_str()) {
        return Err("Select the actual mod loader before looking up releases.".into());
    }
    Ok(())
}

pub fn search_url(target: &Target) -> Result<reqwest::Url> {
    if target.query.trim().is_empty() || target.query.len() > 128 {
        return Err("Enter a mod name or ID first.".into());
    }
    if !target.minecraft.is_empty() || !target.loader.is_empty() {
        let mut checked = target.clone();
        if checked.minecraft.is_empty() {
            checked.minecraft = "1.0".into();
        }
        if checked.loader.is_empty() {
            checked.loader = "fabric".into();
        }
        validate(&checked)?;
    }
    let mut url =
        reqwest::Url::parse("https://api.modrinth.com/v2/search").map_err(|e| e.to_string())?;
    let mut filters = vec![vec!["project_type:mod".to_string()]];
    if !target.loader.is_empty() {
        filters.push(vec![format!("categories:{}", target.loader)]);
    }
    if !target.minecraft.is_empty() {
        filters.push(vec![format!("versions:{}", target.minecraft)]);
    }
    let facets = serde_json::to_string(&filters).expect("search facets");
    url.query_pairs_mut()
        .append_pair("query", &target.query)
        .append_pair("facets", &facets)
        .append_pair("limit", "5");
    Ok(url)
}

pub fn releases_url(project: &str, target: &Target) -> Result<reqwest::Url> {
    validate(target)?;
    if !valid_id(project) {
        return Err("Invalid Modrinth project ID".into());
    }
    let mut url = reqwest::Url::parse(&format!(
        "https://api.modrinth.com/v2/project/{project}/version"
    ))
    .map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair(
            "game_versions",
            &serde_json::json!([target.minecraft]).to_string(),
        )
        .append_pair("loaders", &serde_json::json!([target.loader]).to_string());
    Ok(url)
}

pub fn parse_projects(bytes: &[u8]) -> Result<Vec<Project>> {
    #[derive(Deserialize)]
    struct Search {
        hits: Vec<Project>,
    }
    let result: Search = serde_json::from_slice(bytes)
        .map_err(|_| "Modrinth returned invalid project metadata".to_string())?;
    Ok(result
        .hits
        .into_iter()
        .filter(|p| {
            p.project_type == "mod"
                && valid_id(&p.project_id)
                && valid_id(&p.slug)
                && p.title.len() <= 256
                && p.author.len() <= 128
        })
        .take(5)
        .collect())
}

pub fn parse_releases(bytes: &[u8], project: &str, target: &Target) -> Result<Vec<Release>> {
    let releases: Vec<Release> = serde_json::from_slice(bytes)
        .map_err(|_| "Modrinth returned invalid release metadata".to_string())?;
    if releases.len() > 512 {
        return Err("Modrinth release list exceeds safety limit".into());
    }
    let mut result = releases
        .into_iter()
        .filter(|r| {
            r.project_id == project
                && valid_id(&r.id)
                && r.version_number.len() <= 256
                && !r.version_number.is_empty()
                && r.name.len() <= 512
                && r.date_published.len() <= 40
                && r.dependencies.len() <= 128
                && r.dependencies.iter().all(|d| {
                    ["required", "optional", "incompatible", "embedded"]
                        .contains(&d.dependency_type.as_str())
                        && d.project_id.as_deref().is_none_or(valid_id)
                        && d.version_id.as_deref().is_none_or(valid_id)
                })
                && r.game_versions.contains(&target.minecraft)
                && r.loaders.contains(&target.loader)
                && target
                    .version_range
                    .as_ref()
                    .is_none_or(|range| mods::matches(range, &r.version_number) == Some(true))
        })
        .collect::<Vec<_>>();
    result.sort_by(|a, b| {
        (a.version_type != "release")
            .cmp(&(b.version_type != "release"))
            .then_with(|| b.date_published.cmp(&a.date_published))
    });
    result.truncate(12);
    Ok(result)
}

fn fetch(url: reqwest::Url, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Cannot initialize metadata lookup".to_string())?;
    runtime.block_on(async {
        let client=reqwest::Client::builder().https_only(true).redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(25)).user_agent("MinecraftAiDoctor/0.1.0 (read-only Minecraft compatibility companion; Modrinth metadata lookup)").build().map_err(|_|"Cannot initialize HTTPS client".to_string())?;
        let operation=async {
            if cancel.load(Ordering::Relaxed){return Err("Cancelled".into());}
            for attempt in 0..3 {
                let mut response=client.get(url.clone()).send().await.map_err(|_|"Modrinth lookup failed or timed out. Offline analysis is still available.".to_string())?;
                let status=response.status();
                if (status.as_u16()==429 || status.is_server_error()) && attempt<2 {
                    let wait=response.headers().get("retry-after").and_then(|v|v.to_str().ok()).and_then(|s|s.parse::<u64>().ok()).unwrap_or(1<<attempt).clamp(1,10);
                    tokio::time::sleep(Duration::from_secs(wait)).await;continue;
                }
                if !status.is_success(){return Err(format!("Modrinth returned HTTP {}. Try later; no mod files were downloaded.",status.as_u16()));}
                let mut bytes=Vec::new();
                while let Some(chunk)=response.chunk().await.map_err(|_|"Cannot read Modrinth response".to_string())? {
                    if bytes.len()+chunk.len()>2*1024*1024 {return Err("Modrinth response exceeds 2 MiB limit".into());} bytes.extend_from_slice(&chunk);
                }
                return Ok(bytes);
            }
            Err("Metadata lookup retries exhausted".into())
        };
        let cancellation=async {loop {if cancel.load(Ordering::Relaxed){return;}tokio::time::sleep(Duration::from_millis(60)).await;}};
        tokio::select! {result=operation=>result,_=cancellation=>Err("Cancelled".into())}
    })
}

pub fn project_icon(url: &str, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "Invalid project artwork URL")?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("cdn.modrinth.com")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.path().starts_with("/data/")
    {
        return Err("Project artwork must use the direct HTTPS Modrinth CDN".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }
    let disk = crate::cache::Disk::user();
    let hash = crate::cache::key(url.as_bytes());
    match disk.get::<Vec<u8>>("artwork", &hash, crate::cache::AI_TTL) {
        Ok(Some(bytes)) if bytes.len() <= 512 * 1024 => return Ok(bytes),
        Ok(_) => (),
        Err(e) => crate::cache::warning(&e),
    }
    let bytes = fetch(parsed, cancel)?;
    if bytes.len() > 512 * 1024 {
        return Err("Project icon exceeds 512 KiB".into());
    }
    if let Err(e) = disk.put("artwork", &hash, &bytes) {
        crate::cache::warning(&e);
    }
    Ok(bytes)
}

pub fn search(target: &Target, cancel: &AtomicBool) -> Result<Vec<Project>> {
    let url = search_url(target)?;
    let hash = crate::cache::key(format!("projects-with-artwork-v2:{url}").as_bytes());
    let disk = crate::cache::Disk::user();
    match disk.get::<Vec<Project>>("metadata", &hash, crate::cache::METADATA_TTL) {
        Ok(Some(projects)) => {
            return parse_projects(
                &serde_json::to_vec(&serde_json::json!({"hits":projects}))
                    .map_err(|e| e.to_string())?,
            );
        }
        Ok(None) => (),
        Err(e) => crate::cache::warning(&e),
    }
    let projects = parse_projects(&fetch(url, cancel)?)?;
    if let Err(e) = disk.put("metadata", &hash, &projects) {
        crate::cache::warning(&e);
    }
    Ok(projects)
}
pub fn releases(project: &str, target: &Target, cancel: &AtomicBool) -> Result<Vec<Release>> {
    let url = releases_url(project, target)?;
    let hash = crate::cache::key(format!("{}:{:?}", url, target.version_range).as_bytes());
    let disk = crate::cache::Disk::user();
    match disk.get::<Vec<Release>>("metadata", &hash, crate::cache::METADATA_TTL) {
        Ok(Some(releases)) => {
            return parse_releases(
                &serde_json::to_vec(&releases).map_err(|e| e.to_string())?,
                project,
                target,
            );
        }
        Ok(None) => (),
        Err(e) => crate::cache::warning(&e),
    }
    let releases = parse_releases(&fetch(url, cancel)?, project, target)?;
    if let Err(e) = disk.put("metadata", &hash, &releases) {
        crate::cache::warning(&e);
    }
    Ok(releases)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artwork_urls_and_missing_context_stay_safe() {
        let cancel = AtomicBool::new(false);
        for url in [
            "http://cdn.modrinth.com/data/image.png",
            "https://localhost/data/image.png",
            "https://cdn.modrinth.com.evil.example/data/image.png",
            "https://cdn.modrinth.com/data/image.png?secret=test",
        ] {
            assert!(project_icon(url, &cancel).is_err());
        }
        assert!(
            project_icon(
                "https://cdn.modrinth.com/data/image.png",
                &AtomicBool::new(true)
            )
            .unwrap_err()
            .contains("Cancelled")
        );
        let target = Target {
            query: "nvidium".into(),
            ..Default::default()
        };
        assert!(search_url(&target).is_ok());
        assert!(releases_url("project", &target).is_err());
        assert_eq!(range("0.5.9 or version 0.5.11"), "0.5.9 || 0.5.11");
    }
    #[test]
    fn fabric_wrong_versions_identifies_both_mods_without_english_words_as_versions() {
        let report =
            crate::analyze_text(include_str!("../tests/fixtures/sodium-conflict.log")).unwrap();
        let finding = report
            .findings
            .iter()
            .find(|f| f.code == "version")
            .unwrap();
        let guide = remedies::for_finding(&report, finding).checklist();
        assert!(guide.contains("sodium"), "{guide}");
        assert!(guide.contains("0.8.12-beta.2+mc1.21.1"), "{guide}");
        assert!(!guide.contains("the installed"), "{guide}");
        let target = target(&report, finding).unwrap();
        assert_eq!(target.query, "nvidium");
        assert_eq!(target.installed, "0.3.1");
        assert_eq!(target.minecraft, "1.21.1");
        assert!(target.version_range.is_none());
        assert_eq!(crate::triage::subject(finding).as_deref(), Some("nvidium"));
    }
    #[test]
    fn target_is_the_mod_to_replace_not_the_game() {
        let report = crate::analyze_text(include_str!("../tests/fixtures/detailed.log")).unwrap();
        let f = report
            .findings
            .iter()
            .find(|f| f.code == "version")
            .unwrap();
        let t = target(&report, f).unwrap();
        assert_eq!(t.query, "nvidium");
        assert_eq!(t.installed, "0.4.1");
        assert_eq!(t.minecraft, "1.21.1");
        assert_eq!(t.loader, "fabric");
        assert!(t.version_range.is_none());
    }
    #[test]
    fn runtime_context_comes_from_the_findings_log_not_a_stale_other_log() {
        let mut report =
            crate::analyze_text(include_str!("../tests/fixtures/detailed.log")).unwrap();
        report.logs.insert(
            0,
            crate::logs::parse(
                "older.log".into(),
                b"Loading Minecraft 1.20.1 with Fabric Loader 0.15",
                false,
            ),
        );
        let index = report
            .findings
            .iter()
            .position(|f| f.code == "dependency")
            .unwrap();
        assert_eq!(
            target(&report, &report.findings[index]).unwrap().minecraft,
            "1.21.1"
        );
        report.logs[1].lines.retain(|line| line.number != 1);
        assert!(
            target(&report, &report.findings[index])
                .unwrap()
                .minecraft
                .is_empty()
        );
    }
    #[test]
    fn releases_must_match_game_loader_project_and_required_range() {
        let t = Target {
            query: "example".into(),
            minecraft: "1.21.1".into(),
            loader: "fabric".into(),
            version_range: Some(">=2".into()),
            ..Default::default()
        };
        let mut good = serde_json::json!({"id":"version1","project_id":"project1","version_number":"2.1.0","name":"Example","game_versions":["1.21.1"],"loaders":["fabric"],"date_published":"2026-01-01T00:00:00Z","version_type":"release","dependencies":[]});
        let mut wrong = good.clone();
        wrong["loaders"] = serde_json::json!(["forge"]);
        let mut old = good.clone();
        old["version_number"] = serde_json::json!("1.0.0");
        let bytes = serde_json::to_vec(&vec![good.clone(), wrong, old]).unwrap();
        assert_eq!(parse_releases(&bytes, "project1", &t).unwrap().len(), 1);
        good["project_id"] = serde_json::json!("another");
        assert!(
            parse_releases(&serde_json::to_vec(&vec![good]).unwrap(), "project1", &t)
                .unwrap()
                .is_empty()
        );
        assert!(search_url(&Target::default()).is_err());
        assert!(releases_url("../escape", &t).is_err());
        assert!(parse_projects(b"invalid").is_err());
    }
    #[test]
    #[ignore = "Requires explicit approval for public Modrinth metadata requests"]
    fn live_public_metadata_lookup() {
        let t = Target {
            query: "nvidium".into(),
            minecraft: "1.21.1".into(),
            loader: "fabric".into(),
            ..Default::default()
        };
        let token = AtomicBool::new(false);
        let projects = search(&t, &token).unwrap();
        let project = projects
            .iter()
            .find(|p| p.slug == "nvidium")
            .expect("official Nvidium search result");
        let releases = releases(&project.project_id, &t, &token).unwrap();
        assert!(!releases.is_empty());
        println!(
            "Modrinth metadata verified: {} by {} — release {} for Minecraft {} / {}",
            project.title, project.author, releases[0].version_number, t.minecraft, t.loader
        );
    }
}
