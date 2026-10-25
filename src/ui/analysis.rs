use super::{ACCENT, MUTED};
use crate::{Finding, Report, discovery, privacy, remedies};
use eframe::egui::{self, Color32, RichText};
use std::{collections::BTreeSet, path::PathBuf};

fn severity_color(severity: &str) -> Color32 {
    match severity {
        "error" => Color32::from_rgb(255, 143, 137),
        "warning" => Color32::from_rgb(244, 201, 120),
        _ => ACCENT,
    }
}

pub(super) fn evidence_section(
    ui: &mut egui::Ui,
    index: usize,
    finding: &Finding,
) -> egui::CollapsingResponse<Option<String>> {
    egui::CollapsingHeader::new("Evidence & context")
        .id_salt(("finding_evidence", index))
        .show(ui, |ui| {
            ui.label(RichText::new(&finding.confidence).small().color(MUTED));
            let mut sources = BTreeSet::new();
            let mut error = None;
            let mut evidence = finding.evidence.iter().collect::<Vec<_>>();
            evidence.sort_by_key(|e| (&e.source, e.line));
            for e in evidence {
                ui.label(
                    RichText::new(format!("{}:{}", e.source, e.line))
                        .small()
                        .color(ACCENT),
                );
                ui.add(
                    egui::Label::new(RichText::new(privacy::redact(&e.text)).monospace()).wrap(),
                );
                if !e.source.contains('!') && std::path::Path::new(&e.source).is_file() {
                    sources.insert(&e.source);
                }
            }
            for source in sources {
                ui.push_id(source, |ui| {
                    if ui.small_button("Open source in Notepad").clicked() {
                        match std::process::Command::new("notepad.exe")
                            .arg(source)
                            .spawn()
                        {
                            Ok(_) => (),
                            Err(e) => error = Some(format!("Cannot open source: {e}")),
                        }
                    }
                });
            }
            error
        })
}

fn mods_folder(report: &Report) -> Option<PathBuf> {
    report
        .mods
        .first()
        .and_then(|m| PathBuf::from(&m.source).parent().map(PathBuf::from))
        .or_else(|| {
            report.logs.iter().find_map(|log| {
                let path = PathBuf::from(&log.source);
                let parent = path.parent()?;
                let root = if parent
                    .file_name()
                    .is_some_and(|n| n == "logs" || n == "crash-reports")
                {
                    parent.parent()?
                } else {
                    parent
                };
                let mods = root.join("mods");
                mods.is_dir().then_some(mods)
            })
        })
        .filter(|p| discovery::safe_path(p).is_ok())
}

fn issue_choice(
    ui: &mut egui::Ui,
    report: &Report,
    index: usize,
    selected: &mut usize,
    icons: &super::icons::Textures,
) {
    let finding = &report.findings[index];
    let guide = remedies::for_finding(report, finding);
    ui.push_id(("issue_choice", index), |ui| {
        ui.horizontal(|ui| {
            if let Some(id) = crate::triage::subject(finding) {
                icons.show(ui, &id, 24.0);
            }
            if ui
                .selectable_label(
                    *selected == index,
                    RichText::new(format!("{} · {}", finding.severity, guide.title)).color(
                        if *selected == index {
                            Color32::from_rgb(228, 236, 242)
                        } else {
                            severity_color(&finding.severity)
                        },
                    ),
                )
                .clicked()
            {
                *selected = index;
            }
        });
    });
}

pub(super) fn choose(
    ui: &mut egui::Ui,
    report: &Report,
    selected: &mut usize,
    query: &mut String,
    icons: &super::icons::Textures,
) {
    if report.findings.is_empty() {
        ui.label("No findings available. Import a log to begin.");
        return;
    }
    *selected = (*selected).min(report.findings.len() - 1);
    let groups = crate::triage::groups(report);
    ui.horizontal_wrapped(|ui| {
        for (group, indices) in &groups {
            if ui
                .selectable_label(
                    *group == crate::triage::category(&report.findings[*selected]),
                    format!("{}  {}", group.title(), indices.len()),
                )
                .clicked()
            {
                *selected = indices[0];
                query.clear();
            }
        }
    });
    let group = crate::triage::category(&report.findings[*selected]);
    ui.label(RichText::new(group.guidance()).color(MUTED));
    let attention = groups
        .iter()
        .find(|(g, _)| *g == group)
        .map(|(_, indices)| indices.as_slice())
        .unwrap_or(&[]);
    if attention.len() > 1 {
        ui.add(
            egui::TextEdit::singleline(query)
                .hint_text("Filter affected mods…")
                .desired_width(300.0),
        );
        let filter = query.trim().to_lowercase();
        egui::ScrollArea::vertical()
            .id_salt("issue_choices")
            .max_height(130.0)
            .show(ui, |ui| {
                ui.spacing_mut().button_padding = egui::vec2(12.0, 6.0);
                ui.spacing_mut().item_spacing.y = 6.0;
                for &index in attention {
                    if report.findings[index]
                        .title
                        .to_lowercase()
                        .contains(&filter)
                    {
                        issue_choice(ui, report, index, selected, icons);
                    }
                }
            });
        ui.add_space(10.0);
        ui.separator();
    }
}

pub(super) fn detail(
    ui: &mut egui::Ui,
    report: &Report,
    selected: usize,
    icons: &super::icons::Textures,
) -> Option<String> {
    let finding = report.findings.get(selected)?;
    let guide = remedies::for_finding(report, finding);
    ui.add_space(12.0);
    ui.horizontal_wrapped(|ui| {
        if let Some(id) = crate::triage::subject(finding) {
            icons.show(ui, &id, 32.0);
        }
        ui.label(RichText::new(&guide.title).size(23.0).strong());
    });
    if finding.code == "duplicate"
        && let Some(id) = crate::triage::subject(finding)
    {
        let count = report.mods.iter().filter(|m| m.id == id).count();
        icons.transition(
            ui,
            &id,
            &format!("{count} installed JARs"),
            "Keep one version",
        );
    }
    if guide.title != finding.title {
        ui.label(RichText::new(&finding.title).color(MUTED));
    }
    ui.add_space(6.0);
    ui.label(RichText::new("How to fix").size(17.0).strong());
    for (index, step) in guide.steps.iter().enumerate() {
        ui.horizontal_top(|ui| {
            ui.label(RichText::new(format!("{}.", index + 1)).color(MUTED));
            ui.add(egui::Label::new(privacy::redact(step)).wrap());
        });
    }
    if let Some(confirmation) = &guide.confirmation {
        ui.label(
            RichText::new(privacy::redact(confirmation))
                .small()
                .color(MUTED),
        );
    }
    let mut error = None;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("Copy fix steps").clicked() {
            ui.ctx().copy_text(guide.checklist());
        }
        if let Some(folder) = mods_folder(report)
            && ui.button("Open mods folder").clicked()
        {
            match std::process::Command::new("explorer.exe")
                .arg(folder)
                .spawn()
            {
                Ok(_) => (),
                Err(e) => error = Some(format!("Cannot open mods folder: {e}")),
            }
        }
    });
    ui.add_space(10.0);
    if let Some(Some(source_error)) = evidence_section(ui, selected, finding).body_returned {
        error = Some(source_error);
    }
    if !report.warnings.is_empty() {
        egui::CollapsingHeader::new(format!("Analysis notes ({})", report.warnings.len()))
            .id_salt("analysis_notes")
            .show(ui, |ui| {
                for warning in &report.warnings {
                    ui.label(privacy::redact(warning));
                }
            });
    }
    error
}
