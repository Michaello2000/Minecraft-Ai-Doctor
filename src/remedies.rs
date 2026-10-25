//! Evidence-derived instructions only. This module never changes a game instance.
use crate::{Finding, Report};
use regex::Regex;
use std::{path::Path, sync::LazyLock};

pub(crate) static DEPENDENCY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Mod\s+'(?P<name>[^'\r\n]+)'(?:\s+\((?P<id>[^)\r\n]+)\))?(?:\s+(?P<mod_version>[0-9][^\s]*))?\s+requires\s+(?:version\s+)?(?P<requirement>.+?)\s+of\s+(?:mod\s+)?(?:'(?P<label>[^']+)'(?:\s+\((?P<quoted_id>[^)]+)\))?|(?P<bare_id>[A-Za-z0-9_.-]+))(?P<rest>.*)").expect("constant dependency pattern")
});
static CLASS_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"class file version (\d+)\.\d+").expect("constant Java pattern"));
static INSTALLED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"but only (?:the wrong versions are present:\s*)?([0-9][A-Za-z0-9_.+/-]*)")
        .expect("constant installed-version pattern")
});

pub(crate) fn installed(raw: &str) -> Option<String> {
    INSTALLED
        .captures(raw)
        .map(|c| c[1].split('/').collect::<Vec<_>>().join(", "))
}

const COPY_FIRST: &str =
    "Create a disposable copy of this instance before changing mods or Java settings.";
const VERIFY: &str = "Launch the copy once, then import its new latest.log here to check whether this failure is gone.";

#[derive(Clone, Debug)]
pub struct Guide {
    pub title: String,
    pub steps: Vec<String>,
    pub confirmation: Option<String>,
}

impl Guide {
    pub fn checklist(&self) -> String {
        let mut text = format!("{}\n\n", self.title);
        for (i, step) in self.steps.iter().enumerate() {
            text.push_str(&format!("{}. {step}\n", i + 1));
        }
        if let Some(confirmation) = &self.confirmation {
            text.push_str(&format!("\nNeeds confirmation: {confirmation}\n"));
        }
        crate::privacy::redact(&text)
    }
}

pub fn for_finding(report: &Report, finding: &Finding) -> Guide {
    if finding.code == "dependency" || finding.code == "version" {
        for evidence in &finding.evidence {
            if let Some(captures) = DEPENDENCY.captures(&evidence.text) {
                let rest = &captures["rest"];
                if finding.code == "dependency" && !rest.contains("missing") {
                    continue;
                }
                if finding.code == "version" && rest.contains("missing") {
                    continue;
                }
                let name = &captures["name"];
                let id = captures
                    .name("quoted_id")
                    .or_else(|| captures.name("bare_id"))
                    .or_else(|| captures.name("label"))
                    .map_or("unknown", |m| m.as_str());
                let required = &captures["requirement"];
                if finding.code == "dependency" {
                    return Guide { title: format!("Install {id} ({required})"), steps: vec![
                        COPY_FIRST.into(),
                        format!("{name} requires {id}, version {required}. On that mod's official page, choose a build for this copy's exact Minecraft version and loader."),
                        format!("Add the matching {id} JAR to the copy's mods folder. Keep a single intended version of that mod."),
                        VERIFY.into(),
                    ], confirmation: Some("The log supplies the dependency and version requirement. A compatible download has not been verified or downloaded.".into()) };
                }
                if let Some(installed) = installed(rest) {
                    return Guide {title:format!("Use a compatible build of {name}"),steps:vec![
                        COPY_FIRST.into(),
                        format!("{name} requires {id} {required}; this log reports {installed} installed."),
                        if ["minecraft","fabricloader","quilt_loader","forge","neoforge","java"].contains(&id){format!("Keep the pack's existing {id} {installed} baseline and replace {name} with a build supporting it. If no such build exists, test removing {name} from the copy, or use a separate instance for {id} {required}.")}else{format!("Choose a {name} build that supports the pack's intended {id} version. Alternatively, use {id} {required} in the copy after checking the other mods' requirements. Do not downgrade the whole pack blindly.")},
                        VERIFY.into(),
                    ],confirmation:Some("Review the replacement's declared game, loader and dependency requirements before downloading.".into())};
                }
            }
        }
    }
    if finding.code == "duplicate" {
        let id = finding.title.strip_prefix("Duplicate mod ID: ");
        let names = report
            .mods
            .iter()
            .filter(|m| id.is_some_and(|id| m.id == id))
            .map(|m| {
                Path::new(&m.source)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        return Guide {title:id.map_or("Keep one version of the duplicated mod".into(),|id|format!("Keep one version of {id}")),steps:vec![
            COPY_FIRST.into(),
            if names.is_empty() { "Read the evidence below to identify the duplicate mod ID and JAR filenames.".into() } else { format!("These JARs declare the same ID: {}.",names.join(", ")) },
            "Choose the version required by the pack and its other mods. Move the extra JARs out of the copy's mods folder; do not just rename their display names.".into(),
            VERIFY.into(),
        ],confirmation:Some("The Doctor cannot choose which version to keep without checking all affected dependencies.".into())};
    }
    if finding.code == "compatibility" {
        for m in &report.mods {
            for dependency in &m.dependencies {
                if finding
                    .title
                    .starts_with(&format!("{} → {}:", m.id, dependency.id))
                {
                    let filename = Path::new(&m.source)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    let conflict = dependency.kind == "conflict" || dependency.kind == "warning";
                    let unknown = finding.severity == "info"
                        || finding.title.contains("not found among top-level");
                    return Guide {title: if conflict {format!("Review {} with {}",m.id,dependency.id)} else {format!("Match {} to {}",dependency.id,m.id)},steps:vec![
                        COPY_FIRST.into(),
                        format!("{filename} declares a {} requirement: {} {}.",dependency.kind,dependency.id,dependency.requirement),
                        if unknown {format!("Check the launcher's dependency error before adding anything: {} may be supplied by a nested JAR or alias. If the loader confirms it is missing, use a build matching the declared requirement and this pack's game/loader.",dependency.id)}
                        else if conflict {format!("Test keeping only one of {} and {} in the copy, or select versions outside the declared conflict range. Check the pack's required dependencies before removing either.",m.id,dependency.id)}
                        else {format!("Replace {} in the copy with a version satisfying {}. For Minecraft or loader requirements, prefer a compatible build of {} rather than upgrading the entire pack blindly.",dependency.id,dependency.requirement,m.id)},
                        VERIFY.into(),
                    ],confirmation:Some(if unknown { "Top-level metadata cannot prove a missing dependency or evaluate this version predicate." } else { "Declared metadata is evidence of a requirement, not a guarantee of runtime compatibility." }.into())};
                }
            }
        }
    }
    if finding.code == "java"
        && let Some(class) = finding.evidence.iter().find_map(|e| {
            CLASS_VERSION
                .captures(&e.text)
                .and_then(|c| c[1].parse::<u32>().ok())
        })
        && (45..=100).contains(&class)
    {
        let java = class - 44;
        return Guide {
            title: format!("Select a Java {java} compatible runtime"),
            steps: vec![
                COPY_FIRST.into(),
                format!(
                    "The failing class uses class-file version {class}. It needs Java {java} or newer; also check the game's and loader's supported Java range."
                ),
                format!(
                    "In the copy's launcher Java settings, select an installed 64-bit Java {java} runtime supported by the pack. If it is unavailable, install it from an official Java distributor first."
                ),
                VERIFY.into(),
            ],
            confirmation: Some(
                "No Java runtime or launcher setting is changed automatically.".into(),
            ),
        };
    }
    let (title, steps, confirmation) = match finding.code.as_str() {
        "mixin" => (
            "Test the most recent mod changes",
            vec![
                COPY_FIRST,
                "Find the named mixin configuration and target class in the evidence below. Check which mod owns it.",
                "In the copy, undo one recent mod addition or replacement at a time. Use builds matching the pack's Minecraft and loader versions.",
                VERIFY,
            ],
            Some("A Mixin error alone does not prove which pair of mods conflicts."),
        ),
        "memory" => (
            "Review the copy's Java memory settings",
            vec![
                COPY_FIRST,
                "Check the evidence for heap exhaustion versus failure to reserve memory. Confirm the launcher uses 64-bit Java.",
                "For heap exhaustion, increase the copy's maximum heap only if system RAM is available. For a reservation failure, reduce the allocation or close other memory-heavy apps. Leave RAM for Windows.",
                VERIFY,
            ],
            Some("No safe exact RAM allocation can be inferred from this log alone."),
        ),
        "normal_exit" => (
            "No crash fix is needed for this exit",
            vec![
                "This log records exit code 0. Review any separate error findings if the game still behaved incorrectly.",
            ],
            None,
        ),
        "unknown" => (
            "Import the log from the failed launch",
            vec![
                "No supported failure signal was found in this input.",
                "Reproduce the problem yourself, then import that launch's latest.log or crash report. Check the timestamp so an older report does not mislead the diagnosis.",
            ],
            None,
        ),
        _ => (
            "Confirm the cause before changing the pack",
            vec![
                COPY_FIRST,
                "Read the exact error and Caused by chain under Evidence & context.",
                "Check the named mod, class or loader against the pack's game and loader versions. Undo one recent change in the copy at a time.",
                VERIFY,
            ],
            Some("This signal does not identify one safe automatic change."),
        ),
    };
    Guide {
        title: title.into(),
        steps: steps.into_iter().map(str::to_owned).collect(),
        confirmation: confirmation.map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn missing_dependency_names_the_actual_requirement() {
        let report = crate::analyze(Path::new("tests/fixtures/missing.log")).unwrap();
        let guide = for_finding(&report, &report.findings[0]);
        assert!(guide.title.contains("fabric-api"));
        assert!(guide.steps.join(" ").contains(">=0.100"));
        assert!(guide.steps.join(" ").contains("copy"));
    }

    #[test]
    fn java_fix_uses_the_observed_class_version() {
        let report = crate::analyze(Path::new("tests/fixtures/java.log")).unwrap();
        let guide = for_finding(&report, &report.findings[0]);
        assert!(guide.title.contains("Java 21"));
        assert!(!guide.steps.join(" ").contains("Java 25"));
    }

    #[test]
    fn version_fix_preserves_the_existing_game_baseline() {
        let report = crate::analyze(Path::new("tests/fixtures/version.log")).unwrap();
        let guide = for_finding(&report, &report.findings[0]);
        let steps = guide.steps.join(" ");
        assert!(steps.contains("1.20.1"));
        assert!(steps.contains("1.21.1"));
        assert!(steps.contains("Example"));
    }

    #[test]
    fn uncertain_findings_do_not_invent_a_target_mod() {
        let report = crate::analyze(Path::new("tests/fixtures/mixin.log")).unwrap();
        let guide = for_finding(&report, &report.findings[0]);
        assert!(guide.confirmation.is_some());
        assert!(!guide.title.starts_with("Remove"));
    }
}
