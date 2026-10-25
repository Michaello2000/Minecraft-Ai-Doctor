use crate::{
    privacy,
    repair::{Plan, Receipt},
};
use eframe::egui::{self, RichText};

#[derive(Default)]
pub(super) struct Panel {
    pub open: bool,
    pub plan: Option<Plan>,
    pub receipt: Option<Receipt>,
    pub selected_jar: Option<std::path::PathBuf>,
    pub message: String,
    pub ai_proposed: bool,
    download: bool,
}
pub(super) enum Action {
    Prepare,
    AskAi,
    ChooseJar,
    Apply(Box<Plan>, Option<std::path::PathBuf>, bool),
    Rollback(std::path::PathBuf),
    AnalyzeCopy(std::path::PathBuf),
}
impl Panel {
    pub fn reset_plan(&mut self) {
        self.plan = None;
        self.selected_jar = None;
        self.download = false;
        self.message.clear();
    }
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        if !self.open {
            return None;
        }
        let mut action = None;
        egui::Frame::group(ui.style()).inner_margin(18).corner_radius(12).show(ui,|ui| {
            ui.horizontal(|ui| {ui.label(RichText::new("Review auto-fix").size(22.0).strong());if ui.small_button("Close").clicked(){self.open=false;}});
            ui.label("Changes are applied to a new mod-only repair copy. Your selected instance, worlds and accounts stay untouched.");
            if !self.message.is_empty(){ui.label(privacy::redact(&self.message));}
            ui.horizontal_wrapped(|ui|{
                if ui.button("Rebuild reviewed plan").clicked(){action=Some(Action::Prepare);}
                if ui.button("Ask AI for a proposal").clicked(){action=Some(Action::AskAi);}
            });
            if let Some(plan)=&mut self.plan {
                ui.label(RichText::new(if self.ai_proposed{"AI-selected findings, locally verified file actions"}else{"Local duplicate suggestions · no AI key needed"}).color(super::theme::MUTED));
                if !plan.duplicates.is_empty(){ui.label(format!("{} duplicate groups · scroll to review every file before applying",plan.duplicates.len()));}
                egui::ScrollArea::vertical().id_salt("review_duplicate_changes").max_height(240.0).show(ui,|ui|{for group in &mut plan.duplicates {
                    ui.push_id(&group.mod_id,|ui| {
                        ui.separator();ui.label(RichText::new(format!("{} · keep one version",group.mod_id)).strong());
                        egui::ComboBox::from_id_salt("keep_version").selected_text(&group.keep).show_ui(ui,|ui|{for candidate in &group.choices{ui.selectable_value(&mut group.keep,candidate.file.clone(),format!("{} · {}",candidate.version,candidate.file));}});
                        for candidate in &group.choices{if candidate.file!=group.keep{ui.label(format!("Back up and disable in copy: {}",candidate.file));}}
                    });
                }});
                if let Some(replacement)=&plan.replacement {
                    ui.separator();ui.label(RichText::new(format!("Replace {} with {}",replacement.download.mod_id,replacement.download.version)).strong());
                    ui.label(format!("Expected file: {} · {:.1} MiB",replacement.download.filename,replacement.download.size as f64/(1024.0*1024.0)));
                    ui.label("Select the already-downloaded JAR. The app verifies its SHA-512, mod ID, version, loader and declared game range before applying it.");
                    if ui.button("Choose replacement JAR…").clicked(){action=Some(Action::ChooseJar);}
                    if let Some(path)=&self.selected_jar{ui.label(path.file_name().unwrap_or_default().to_string_lossy());}
                    ui.checkbox(&mut self.download,"Download this exact JAR from Modrinth instead (explicit consent)");
                    if self.download{ui.monospace(&replacement.download.url);ui.label(format!("SHA-512: {}",replacement.download.sha512));ui.label("Only the reviewed file is downloaded; no logs or API keys are sent to Modrinth. Hash and descriptor checks are required before apply.");}
                }
                let count=plan.duplicates.iter().map(|d|d.choices.len().saturating_sub(1)).sum::<usize>()+usize::from(plan.replacement.is_some());
                ui.add_space(8.0);
                let ready=count>0 && (plan.replacement.is_none() || self.selected_jar.is_some() || self.download);
                let label=if self.download && plan.replacement.is_some(){format!("Download & apply {count} changes to repair copy")}else{format!("Apply {count} reviewed changes to repair copy")};
                if ui.add_enabled(ready,egui::Button::new(label).fill(super::theme::SELECTED)).clicked(){action=Some(Action::Apply(Box::new(plan.clone()),self.selected_jar.clone(),self.download));self.download=false;}
                if count==0{ui.label("No verified automatic changes are available for this input. Check exact mod releases or resolve the selected finding manually.");}
                egui::CollapsingHeader::new("What this plan does and does not cover").id_salt("repair_scope").show(ui,|ui|{for note in &plan.notes{ui.label(note);}});
            }
            if let Some(receipt)=&self.receipt {
                ui.separator();ui.label(format!("Repair copy: {}",receipt.directory.display()));
                ui.label(if receipt.rolled_back{"Rollback complete; copied mods restored."}else if receipt.interrupted{"Repair was interrupted. Review and roll back the copy before continuing."}else{"Reviewed file changes applied. A successful game launch has not been verified."});
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Analyze copied mods").clicked(){action=Some(Action::AnalyzeCopy(receipt.directory.join("mods")));}
                    if ui.add_enabled(!receipt.rolled_back,egui::Button::new("Roll back this repair copy")).clicked(){action=Some(Action::Rollback(receipt.directory.clone()));}
                });
            }
        });
        ui.add_space(16.0);
        action
    }
}
