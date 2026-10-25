use crate::{Finding, logs::Log};
use std::collections::HashSet;

pub fn diagnose(log: &Log) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut seen = HashSet::new();
    for (index, line) in log.lines.iter().enumerate() {
        let t = line.text.to_lowercase();
        let rule = if t.contains("unsupportedclassversionerror") || t.contains("class file version")
        {
            Some((
                "java",
                "error",
                "Java runtime is too old for a loaded class",
                "Check the launcher's selected Java runtime. Class 61 requires Java 17; class 65 requires Java 21; class 69 requires Java 25. Match the game and loader requirements.",
            ))
        } else if t.contains("duplicate") && (t.contains("mod") || t.contains("id")) {
            Some((
                "duplicate",
                "error",
                "Duplicate mod identity reported",
                "Compare the named JARs and their mod IDs. Keep one intended version in a disposable copy before changing the real instance.",
            ))
        } else if (t.contains("requires") && t.contains("missing"))
            || t.contains("modresolutionexception")
            || t.contains("missing mandatory dependencies")
        {
            Some((
                "dependency",
                "error",
                "Required mod dependency is missing or unresolved",
                "Check the exact dependency ID and required version in the evidence. Obtain a matching game/loader build from the mod's official source after review.",
            ))
        } else if (t.contains("requires")
            && (t.contains("minecraft") || t.contains("fabricloader") || t.contains("but only")))
            || t.contains("incompatible mod set")
        {
            Some((
                "version",
                "error",
                "Game, loader or dependency version mismatch",
                "Compare installed versions with the declared requirement. Use a mod build matching this instance's exact Minecraft version and loader.",
            ))
        } else if t.contains("mixinapplyerror")
            || t.contains("invalidinjectionexception")
            || t.contains("mixin transformation")
            || t.contains("injectionerror")
            || (t.contains("mixin") && t.contains("failed"))
        {
            Some((
                "mixin",
                "error",
                "Mixin transformation failed",
                "Inspect the named mixin and its Caused by chain. Check mod/game versions; test one suspected addition at a time in a disposable copy. This signal alone does not prove a conflict between specific mods.",
            ))
        } else if t.contains("outofmemoryerror") || t.contains("could not reserve enough space") {
            Some((
                "memory",
                "error",
                "Java memory allocation failed",
                "Review the heap setting, available RAM and 64-bit Java. A heap error can also indicate a leak; avoid assigning all system RAM.",
            ))
        } else if t.contains("noclassdeffounderror") || t.contains("classnotfoundexception") {
            Some((
                "classloading",
                "error",
                "A required Java class could not be loaded",
                "Check the named class and the preceding Caused by entries. Missing dependencies or wrong loader builds are possibilities, not confirmed mod pairs.",
            ))
        } else if t.contains("loadingfailederror")
            || t.contains("failed to load mods")
            || t.contains("modloadingexception")
            || t.contains("unable to launch")
            || t.contains("could not create the java virtual machine")
        {
            Some((
                "launcher",
                "error",
                "Launcher or mod-loader failure reported",
                "Review the loader error and its surrounding stack trace. Confirm Java, game, loader and mod versions before making changes.",
            ))
        } else if t.contains("caused by:")
            || t.contains("exception in thread")
            || t.contains("crash report saved")
        {
            Some((
                "exception",
                "warning",
                "Exception or crash signal needs review",
                "Read the complete stack trace. Some exceptions are recovered during a successful launch; this signal alone does not establish a crash.",
            ))
        } else if t.contains("process exited with code 0")
            || t.contains("minecraft exited with code 0")
        {
            Some((
                "normal_exit",
                "info",
                "Successful process exit recorded",
                "Exit code 0 indicates a normal process exit. Earlier errors may still require attention; the hook firing alone does not establish a crash.",
            ))
        } else {
            None
        };
        if let Some((code, severity, title, action)) = rule {
            let specific = matches!(code, "dependency" | "version") && t.contains("requires");
            let key = if specific {
                format!("{code}:{}", line.text.trim())
            } else {
                code.into()
            };
            if !seen.insert(key) {
                continue;
            }
            if !specific
                && code == "dependency"
                && log.lines.iter().any(|l| {
                    l.text.contains("requires")
                        && (l.text.contains("missing") || l.text.contains("but only"))
                })
            {
                continue;
            }
            if !specific
                && code == "version"
                && log
                    .lines
                    .iter()
                    .any(|l| l.text.contains("requires") && l.text.contains("but only"))
            {
                continue;
            }
            let start = index.saturating_sub(1);
            let end = (index + 4).min(log.lines.len());
            let mut evidence = vec![log.evidence(index)];
            evidence.extend(
                (start..end)
                    .filter(|&i| i != index)
                    .map(|i| log.evidence(i)),
            );
            findings.push(Finding::new(code, severity, title, action, evidence));
            if findings.len() >= 256 {
                break;
            }
        }
    }
    findings
}
