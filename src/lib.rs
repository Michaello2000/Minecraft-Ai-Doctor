pub mod cache;
pub mod cli;
pub mod discovery;
pub mod lab;
pub mod logs;
pub mod mods;
pub mod privacy;
pub mod providers;
pub mod remedies;
pub mod repair;
pub mod rules;
mod svg_logo;
pub mod triage;
pub mod ui;
pub mod versions;

use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub source: String,
    pub line: usize,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub code: String,
    pub severity: String,
    pub confidence: String,
    pub title: String,
    pub action: String,
    pub evidence: Vec<Evidence>,
}

impl Finding {
    pub fn new(
        code: &str,
        severity: &str,
        title: impl Into<String>,
        action: &str,
        evidence: Vec<Evidence>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: severity.into(),
            confidence: "Deterministic signal; root cause may need confirmation".into(),
            title: title.into(),
            action: action.into(),
            evidence,
        }
    }
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Report {
    #[serde(default)]
    pub runtime: std::collections::BTreeMap<String, String>,
    pub logs: Vec<logs::Log>,
    pub mods: Vec<mods::Mod>,
    pub findings: Vec<Finding>,
    pub warnings: Vec<String>,
}

impl Report {
    pub fn likely_crash(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == "error" && f.code != "compatibility")
    }
    pub fn markdown(&self) -> String {
        let mut text = String::from(
            "# Minecraft Ai Doctor\n\nRead-only local analysis. Metadata checks cannot guarantee runtime compatibility.\n\n",
        );
        for f in &self.findings {
            text.push_str(&format!(
                "## {} [{}]\n{}\n\n{}\n",
                f.title,
                f.severity,
                f.confidence,
                remedies::for_finding(self, f).checklist()
            ));
            for e in &f.evidence {
                text.push_str(&format!("\n- {}:{} — {}\n", e.source, e.line, e.text));
            }
        }
        for w in &self.warnings {
            text.push_str(&format!("\nWarning: {w}\n"));
        }
        privacy::redact(&text)
    }
}

pub fn analyze(path: &Path) -> Result<Report> {
    analyze_cancellable(path, &AtomicBool::new(false))
}

pub fn analyze_text(text: &str) -> Result<Report> {
    if text.trim().is_empty() {
        return Err("Paste console output before analyzing.".into());
    }
    let end = text.floor_char_boundary(text.len().min(logs::MAX_BYTES));
    let log = logs::parse(
        "Pasted console".into(),
        &text.as_bytes()[..end],
        end < text.len(),
    );
    let mut report = Report {
        findings: rules::diagnose(&log),
        ..Default::default()
    };
    if log.truncated {
        report.warnings.push("Pasted console output was truncated to the safety limits; evidence covers only the retained prefix.".into());
    }
    report.logs.push(log);
    if report.findings.is_empty() {
        report.findings.push(Finding::new(
            "unknown",
            "info",
            "No obvious crash detected",
            "Import the failed launch's console output or crash report.",
            vec![],
        ));
    }
    Ok(report)
}

pub fn analyze_cancellable(path: &Path, cancel: &AtomicBool) -> Result<Report> {
    discovery::safe_path(path)?;
    let mut report = Report::default();
    let inputs = discovery::inputs(path)?;
    report.runtime = inputs.versions.clone();
    if inputs.logs.is_empty() && inputs.mods.is_none() {
        return Err("No supported logs or mods directory found. Select a .log/.txt file or Minecraft folder.".into());
    }
    report.warnings.extend(inputs.warnings);
    for file in inputs.logs {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        match logs::read(&file) {
            Ok(log) => {
                if log.truncated {
                    report.warnings.push(format!(
                        "{} was truncated; conclusions cover the retained prefix only.",
                        log.source
                    ));
                }
                report.findings.extend(rules::diagnose(&log));
                report.logs.push(log);
            }
            Err(e) => report.warnings.push(e),
        }
    }
    if let Some(folder) = inputs.mods {
        let (mods, warnings) = mods::scan(&folder, cancel)?;
        report.mods = mods;
        report.warnings.extend(warnings);
        report
            .findings
            .extend(mods::compare(&report.mods, &inputs.versions));
    }
    if report.logs.is_empty() && report.mods.is_empty() && !report.warnings.is_empty() {
        return Err(report.warnings.join("\n"));
    }
    if report.findings.is_empty() {
        report.findings.push(Finding::new("unknown", "info", "No obvious crash detected", "No recognized failure signal was found. This does not prove a successful launch or complete mod compatibility. Review the warnings and import the relevant crash report.", vec![]));
    }
    report.warnings.push("Mod metadata describes declared requirements only. Nested JARs, aliases, dynamic loader behavior and unsupported version syntax can leave results unknown.".into());
    Ok(report)
}
