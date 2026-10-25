use crate::Report;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Duplicates,
    Versions,
    Dependencies,
    Runtime,
    Other,
    Observations,
}
impl Category {
    pub fn title(self) -> &'static str {
        match self {
            Self::Duplicates => "Duplicate mods",
            Self::Versions => "Version conflicts",
            Self::Dependencies => "Missing dependencies",
            Self::Runtime => "Java / launch",
            Self::Other => "Needs investigation",
            Self::Observations => "Unconfirmed observations",
        }
    }
    pub fn guidance(self) -> &'static str {
        match self {
            Self::Duplicates => {
                "Start by keeping one intended version of each mod. Duplicate JARs can cause several follow-on errors."
            }
            Self::Versions => {
                "Keep the pack's game/loader baseline and choose matching mod releases."
            }
            Self::Dependencies => {
                "Check confirmed loader errors before installing dependencies; top-level metadata can miss bundled mods."
            }
            Self::Runtime => "Check the selected Java/runtime and the exact launch failure.",
            Self::Other => {
                "These signals need context. Address concrete mod problems first, then analyze a fresh launch."
            }
            Self::Observations => {
                "These are incomplete metadata observations, not confirmed broken mods."
            }
        }
    }
}
pub fn category(f: &crate::Finding) -> Category {
    if f.severity == "info"
        || (f.code == "compatibility" && f.title.contains("not found among top-level"))
    {
        return Category::Observations;
    }
    match f.code.as_str() {
        "duplicate" => Category::Duplicates,
        "version" => Category::Versions,
        "dependency" => Category::Dependencies,
        "java" | "memory" | "launcher" => Category::Runtime,
        "compatibility" if f.severity == "error" => Category::Versions,
        "compatibility" => Category::Versions,
        _ => Category::Other,
    }
}
pub fn groups(report: &Report) -> Vec<(Category, Vec<usize>)> {
    let mut groups = std::collections::BTreeMap::<Category, Vec<usize>>::new();
    for (i, f) in report.findings.iter().enumerate() {
        groups.entry(category(f)).or_default().push(i);
    }
    groups.into_iter().collect()
}
pub fn first(report: &Report) -> usize {
    groups(report)
        .first()
        .and_then(|(_, items)| items.first())
        .copied()
        .unwrap_or(0)
}

pub fn subject(finding: &crate::Finding) -> Option<String> {
    if let Some(id) = finding.title.strip_prefix("Duplicate mod ID: ") {
        return Some(id.into());
    }
    if let Some((id, _)) = finding.title.split_once(" → ") {
        return Some(id.into());
    }
    for evidence in &finding.evidence {
        if let Some(c) = crate::remedies::DEPENDENCY.captures(&evidence.text) {
            if finding.code == "version" {
                return c
                    .name("id")
                    .or_else(|| c.name("name"))
                    .map(|m| m.as_str().into());
            }
            let dependency = c
                .name("quoted_id")
                .or_else(|| c.name("bare_id"))
                .map(|m| m.as_str());
            if let Some(dep) = dependency
                && ![
                    "minecraft",
                    "fabricloader",
                    "quilt_loader",
                    "forge",
                    "neoforge",
                    "java",
                ]
                .contains(&dep)
            {
                return Some(dep.into());
            }
            return c
                .name("id")
                .or_else(|| c.name("name"))
                .map(|m| m.as_str().into());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn concrete_duplicates_come_before_generic_errors() {
        let mut report = crate::Report::default();
        for code in ["classloading", "duplicate", "version"] {
            report
                .findings
                .push(crate::Finding::new(code, "error", code, "review", vec![]));
        }
        assert_eq!(super::first(&report), 1);
        assert_eq!(super::groups(&report)[0].0, super::Category::Duplicates);
    }
}
