use super::theme::{ACCENT, MUTED, SURFACE};
use crate::{
    Report,
    versions::{self, Project, Release, Target},
};
use eframe::egui::{self, RichText};

#[derive(Default)]
pub(super) struct Lookup {
    index: Option<usize>,
    target: Option<Target>,
    pub projects: Vec<Project>,
    pub releases: Vec<Release>,
    pub project: Option<Project>,
    pub message: String,
}
pub(super) enum Request {
    Search(Target),
    Releases(Project, Target),
    ReviewRepair,
}

impl Lookup {
    pub fn replacement(
        &self,
        report: &Report,
    ) -> crate::Result<Option<crate::repair::Replacement>> {
        let (Some(project), Some(release), Some(target)) =
            (&self.project, self.releases.first(), &self.target)
        else {
            return Ok(None);
        };
        if release.version_number == target.installed {
            return Ok(None);
        }
        let file = release
            .files
            .iter()
            .find(|f| f.primary && f.filename.ends_with(".jar"))
            .or_else(|| release.files.iter().find(|f| f.filename.ends_with(".jar")))
            .ok_or("Selected release has no JAR download metadata")?;
        let mod_id = self
            .index
            .and_then(|index| report.findings.get(index))
            .and_then(crate::triage::subject)
            .ok_or("The actual mod ID must be established before an automatic replacement")?;
        let download = crate::repair::Download {
            url: file.url.clone(),
            filename: file.filename.clone(),
            size: file.size,
            sha512: file.hashes.sha512.clone(),
            project_id: project.project_id.clone(),
            release_id: release.id.clone(),
            mod_id: mod_id.clone(),
            version: release.version_number.clone(),
            minecraft: target.minecraft.clone(),
            loader: target.loader.clone(),
        };
        crate::repair::validate_download(&download)?;
        let old_files = report
            .mods
            .iter()
            .filter(|m| m.id == mod_id)
            .filter_map(|m| {
                std::path::Path::new(&m.source)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .collect();
        Ok(Some(crate::repair::Replacement {
            old_files,
            download,
        }))
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        report: &Report,
        index: usize,
        icons: &super::icons::Textures,
    ) -> Option<Request> {
        if self.index != Some(index) {
            self.reset();
            self.index = Some(index);
            self.target = report
                .findings
                .get(index)
                .and_then(|f| versions::target(report, f));
        }
        let target = self.target.as_mut()?;
        let mut request = None;
        egui::Frame::new().fill(SURFACE).corner_radius(12).inner_margin(18).show(ui,|ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Find a replacement").size(18.0).strong());
            ui.label(format!("Look for a {} build that fits your pack.",target.query));
            if let Some(release)=self.releases.first().filter(|r|r.version_number!=target.installed){icons.transition(ui,&target.query,if target.installed.is_empty(){"Installed: unknown"}else{&target.installed},&release.version_number);}else{
                ui.horizontal(|ui|{
                    icons.show(ui,&target.query,36.0);
                    ui.vertical(|ui|{
                        ui.strong(&target.query);
                        ui.label(format!("Installed: {}",if target.installed.is_empty(){"not established by this input"}else{&target.installed}));
                    });
                });
            }
            ui.label(RichText::new(format!("Needs: {}",target.requirement.replace(" or version "," or "))).color(MUTED));
            let mut query_changed=false;
            egui::CollapsingHeader::new("Search for a different mod").id_salt("lookup_details").show(ui,|ui| {query_changed=ui.text_edit_singleline(&mut target.query).changed();});
            if query_changed{self.projects.clear();self.releases.clear();self.project=None;self.message.clear();}
            if self.project.is_none(){
                ui.label("1. Find the mod, then confirm its name and author.");
                if ui.button("Find mod on Modrinth").clicked(){self.projects.clear();self.releases.clear();self.message="Searching public mod projects…".into();request=Some(Request::Search(target.clone()));}
            }
            ui.label(RichText::new("Uses public Modrinth data. Confirming a project loads its icon. No logs or keys are sent.").small().color(MUTED));
            if !self.message.is_empty(){ui.label(&self.message);}
            let mut chosen=None;
            for project in &self.projects {
                ui.push_id(&project.project_id,|ui| {
                    ui.separator();ui.label(RichText::new(&project.title).strong());ui.label(RichText::new(format!("{} · by {}",project.slug,project.author)).small().color(MUTED));
                    if ui.button("Use this project").clicked(){chosen=Some(project.clone());}
                });
            }
            if let Some(project)=chosen{self.projects.clear();self.project=Some(project.clone());self.releases.clear();self.message="Loading artwork and checking releases…".into();request=Some(Request::Releases(project,target.clone()));}
            ui.separator();ui.label("2. Match your game's version and mod loader.");
            let mut changed=false;
            ui.horizontal_wrapped(|ui|{
                ui.label("Minecraft");changed|=ui.add(egui::TextEdit::singleline(&mut target.minecraft).hint_text("e.g. 1.21.1").desired_width(110.0)).changed();
                egui::ComboBox::from_id_salt("lookup_loader").selected_text(if target.loader.is_empty(){"Choose loader"}else{&target.loader}).show_ui(ui,|ui| {for loader in ["fabric","quilt","forge","neoforge"]{changed|=ui.selectable_value(&mut target.loader,loader.into(),loader).changed();}});
            });
            if changed{self.releases.clear();self.message.clear();}
            if target.minecraft.is_empty(){ui.label(RichText::new("The log does not establish your Minecraft version. Enter the exact version from your launcher.").color(ACCENT));}
            if let Some(project)=&self.project{
                ui.label(format!("Selected: {} · by {}",project.title,project.author));
                if ui.add_enabled(!target.minecraft.is_empty() && !target.loader.is_empty(),egui::Button::new("Find compatible releases")).clicked(){self.releases.clear();request=Some(Request::Releases(project.clone(),target.clone()));}
            }else{ui.label(RichText::new("Confirm a project above to unlock release search.").small().color(MUTED));}
            if let (Some(project),Some(release))=(&self.project,self.releases.first()) {
                ui.separator();
                let already_installed=release.version_number==target.installed;
                ui.label(RichText::new(if already_installed {format!("Already on listed version {}",release.version_number)}else{format!("Try {} {}",project.title,release.version_number)}).size(20.0).strong().color(ACCENT));
                if already_installed {ui.label("This listing does not identify a useful version change. Verify the exact JAR, project identity and other dependencies instead.");}
                ui.label(format!("Modrinth lists this {} for Minecraft {} / {}.",release.version_type,target.minecraft,target.loader));
                ui.label(RichText::new("This matches published game/loader metadata. Check dependencies and test a copy; runtime compatibility is not guaranteed.").small().color(MUTED));
                let url=format!("https://modrinth.com/mod/{}/version/{}",project.project_id,release.id);
                ui.hyperlink_to("View this release on Modrinth",&url);
                if ui.add_enabled(!report.mods.is_empty() && !already_installed,egui::Button::new("Review this change in auto-fix")).clicked(){request=Some(Request::ReviewRepair);}
                if report.mods.is_empty(){ui.label(RichText::new("Import the full instance with its mod JARs to build a repair copy. A log alone cannot be changed safely.").small().color(MUTED));}
                if ui.button("Copy version recommendation").clicked(){ui.ctx().copy_text(format!("{}: {} → {}\nMinecraft {} / {}\nSource: {}\nPublished metadata match; verify dependencies and test in a disposable copy.",project.title,if target.installed.is_empty(){"unknown"}else{&target.installed},release.version_number,target.minecraft,target.loader,url));}
                egui::CollapsingHeader::new("Release details & alternatives").id_salt("release_details").show(ui,|ui| {
                    ui.label(format!("Published: {}",release.date_published));
                    for dependency in &release.dependencies {ui.label(format!("{} dependency · project {} · version {}",dependency.dependency_type,dependency.project_id.as_deref().unwrap_or("unspecified"),dependency.version_id.as_deref().unwrap_or("unspecified")));}
                    for r in self.releases.iter().skip(1){ui.hyperlink_to(format!("{} · {} · {}",r.version_number,r.version_type,r.date_published),format!("https://modrinth.com/mod/{}/version/{}",project.project_id,r.id));}
                });
            }
        });
        request
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changing_findings_discards_other_mods_results() {
        let report =
            crate::analyze_text(include_str!("../../tests/fixtures/detailed.log")).unwrap();
        let ctx = egui::Context::default();
        let mut lookup = Lookup::default();
        let render = |index, lookup: &mut Lookup| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_width(332.0);
                    lookup.show(
                        ui,
                        &report,
                        index,
                        &super::super::icons::Textures::default(),
                    );
                });
            });
        };
        render(0, &mut lookup);
        assert_eq!(lookup.target.as_ref().unwrap().query, "nvidium");
        lookup.projects.push(Project {
            icon_url: None,
            project_id: "project1".into(),
            slug: "nvidium".into(),
            title: "Nvidium".into(),
            author: "fixture".into(),
            project_type: "mod".into(),
        });
        render(1, &mut lookup);
        assert!(lookup.projects.is_empty());
        assert!(lookup.releases.is_empty());
        assert!(lookup.project.is_none());
        assert_eq!(lookup.target.as_ref().unwrap().query, "fabric-api");
    }
}
