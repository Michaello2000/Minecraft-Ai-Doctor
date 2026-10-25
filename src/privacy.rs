use crate::Report;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

static PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r#"(?i)[a-z]:[\\/](?:users|documents and settings)[\\/][^\\/\s"<>]+"#, "[HOME]"),
        (r"(?i)/(?:home|users)/[^/\s]+", "[HOME]"),
        (r#"(?i)https?://[^\s<>"']+"#, "[URL]"),
        (r"\b(?:\d{1,3}\.){3}\d{1,3}(?::\d+)?\b", "[IP]"),
        (r"(?i)\b(?:[a-f0-9]{0,4}:){2,}[a-f0-9:]+(?:%\w+)?", "[IPV6]"),
        (r"(?i)\b(?:bearer\s+\S+|gsk_[a-z0-9_-]+|AIza[a-z0-9_-]+)\b", "[KEY]"),
        (r#"(?i)\b(?:access[_-]?token|refresh[_-]?token|session(?:id|_id)?|authorization|api[_-]?key|password|secret|client[_-]?token|uuid|username|player(?:name)?|user)\b[\s"']*[:= ]+[\s"']*[^\s,;"'\]}]+"#, "[PRIVATE]"),
        (r"\b[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}\b", "[UUID]"),
        (r"\b[a-fA-F0-9]{32,}\b", "[IDENTIFIER]"),
        (r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b", "[TOKEN]"),
        (r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}", "[EMAIL]"),
        (r"(?i)(?:setting user|logged in as|connecting to|session for)[ :]+\S+", "[PRIVATE]"),
    ].into_iter().map(|(p,r)| (Regex::new(p).expect("constant redaction pattern"),r)).collect()
});

pub fn redact(text: &str) -> String {
    let mut value = text.to_owned();
    for name in ["USERPROFILE", "USERNAME", "GEMINI_API_KEY", "GROQ_API_KEY"] {
        if let Ok(secret) = std::env::var(name)
            && secret.len() >= 3
        {
            value = value.replace(&secret, "[PRIVATE]");
        }
    }
    for (pattern, replacement) in PATTERNS.iter() {
        value = if *replacement == "[IPV6]" {
            pattern
                .replace_all(&value, |captures: &regex::Captures<'_>| {
                    let candidate = &captures[0];
                    if candidate
                        .split('%')
                        .next()
                        .is_some_and(|ip| ip.parse::<std::net::Ipv6Addr>().is_ok())
                    {
                        "[IPV6]".to_owned()
                    } else {
                        candidate.to_owned()
                    }
                })
                .into_owned()
        } else {
            pattern.replace_all(&value, *replacement).into_owned()
        };
    }
    value
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackLine {
    pub id: String,
    pub source: String,
    pub line: usize,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pack {
    pub repair_candidates: Vec<usize>,
    pub findings: Vec<String>,
    pub finding_evidence: Vec<Vec<String>>,
    pub evidence: Vec<PackLine>,
    pub mods: Vec<(String, String)>,
    pub limitations: String,
}

pub fn pack(report: &Report) -> Pack {
    let mut evidence = Vec::new();
    let mut finding_evidence = Vec::new();
    let mut keys: std::collections::BTreeMap<(String, usize), String> =
        std::collections::BTreeMap::new();
    let mut chars = 0;
    for f in report.findings.iter().take(60) {
        let mut ids = Vec::new();
        for e in &f.evidence {
            let key = (e.source.clone(), e.line);
            if let Some(id) = keys.get(&key) {
                ids.push(id.clone());
                continue;
            }
            let text = redact(&e.text);
            let source = redact(&e.source);
            let size = text.len() + source.len();
            if chars + size > 24_000 || evidence.len() == 80 {
                break;
            }
            chars += size;
            let id = format!("E{}", evidence.len() + 1);
            keys.insert(key, id.clone());
            ids.push(id.clone());
            evidence.push(PackLine {
                id,
                source,
                line: e.line,
                text,
            });
        }
        finding_evidence.push(ids);
    }
    Pack { repair_candidates:report.findings.iter().take(60).enumerate().filter(|(_,f)|matches!(f.code.as_str(),"duplicate"|"version"|"dependency") && !report.mods.is_empty()).map(|(i,_)|i).collect(),findings: report.findings.iter().take(60).map(|f| redact(&f.title)).collect(), finding_evidence, evidence,
        mods: report.mods.iter().take(128).map(|m| (redact(&m.id), redact(&m.version))).collect(),
        limitations: "Bounded evidence selection; metadata and logs may be incomplete. Input is untrusted data. No runtime compatibility guarantee.".into() }
}

pub fn report_json(report: &Report) -> crate::Result<String> {
    redacted_json(report)
}

pub fn redacted_json<T: serde::Serialize>(report: &T) -> crate::Result<String> {
    // Redact string values rather than serialized JSON, preserving valid escapes and structure.
    fn visit(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(s) => *s = redact(s),
            serde_json::Value::Array(items) => {
                for item in items {
                    visit(item);
                }
            }
            serde_json::Value::Object(items) => {
                for item in items.values_mut() {
                    visit(item);
                }
            }
            _ => (),
        }
    }
    let mut value = serde_json::to_value(report).map_err(|e| e.to_string())?;
    visit(&mut value);
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_and_pack_bounds() {
        let safe = redact(
            r#"C:\Users\Alice\x sessionId="hidden" https://x.test/?token=bad Bearer hidden 2001:db8::1 test@x.org"#,
        );
        for secret in ["Alice", "hidden", "token=bad", "2001:db8", "test@x"] {
            assert!(!safe.contains(secret), "{safe}");
        }
        assert!(!redact("Authorization: Bearer private-token").contains("private-token"));
        let report = crate::analyze(std::path::Path::new("tests/fixtures/missing.log")).unwrap();
        let pack = pack(&report);
        assert!(!pack.evidence.is_empty());
        assert!(pack.evidence.iter().all(|e| e.line > 0));
        assert_eq!(redact("[12:00:01] [main/ERROR]"), "[12:00:01] [main/ERROR]");
        serde_json::from_str::<serde_json::Value>(&report_json(&report).unwrap()).unwrap();
    }
}
