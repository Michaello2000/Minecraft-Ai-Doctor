use crate::{
    Report, Result,
    discovery::{self, Instance},
    privacy,
    providers::{self, Adapter, AiReport, Provider},
};
use eframe::egui::{self, RichText};
mod analysis;
mod icons;
mod lookup;
mod repair_panel;
mod theme;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};
use theme::{ACCENT, MUTED};

#[cfg(test)]
mod regression_tests {
    use super::*;

    #[test]
    fn paste_workflow_runs_in_background_and_keeps_original_line_numbers() {
        let ctx = egui::Context::default();
        let mut app = Doctor::new(&ctx, None, false, Some(Report::default()));
        app.input_mode = InputMode::Paste;
        app.console = include_str!("../tests/fixtures/detailed.log").into();
        app.import(&ctx);
        assert!(app.job.is_some());
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while app.job.is_some() && std::time::Instant::now() < deadline {
            app.poll(&ctx);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.job.is_none(), "Paste worker did not complete");
        let report = app.report.as_ref().unwrap();
        assert_eq!(report.logs[0].source, "Pasted console");
        assert_eq!(report.logs[0].lines[2].number, 3);
        assert_eq!(report.findings.len(), 3);
        assert!(!app.input_open);
        for width in [640.0, 1024.0] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 800.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.analyze_ui(ui, ctx));
                },
            );
        }
    }

    #[test]
    fn repeated_evidence_headers_have_independent_ids() {
        let ctx = egui::Context::default();
        let mut ids = Vec::new();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for index in 0..2 {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        let finding = crate::Finding::new(
                            "dependency",
                            "error",
                            "Missing dependency",
                            "Review requirement",
                            vec![],
                        );
                        ids.push(
                            analysis::evidence_section(ui, index, &finding)
                                .header_response
                                .id,
                        );
                    });
                }
            });
        });
        assert_ne!(
            ids[0], ids[1],
            "Repeated labels must not share interaction state"
        );
    }

    #[test]
    fn evidence_expands_and_closes_without_toggling_another_finding() {
        let ctx = egui::Context::default();
        ctx.style_mut(|style| style.animation_time = 0.0);
        let finding = crate::Finding::new(
            "dependency",
            "error",
            "Missing dependency",
            "Review requirement",
            vec![crate::Evidence {
                source: "fixture.log".into(),
                line: 3,
                text: "requires fabric-api".into(),
            }],
        );
        let render = |events: Vec<egui::Event>| {
            let mut headers = Vec::new();
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        for index in 0..2 {
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                let response = analysis::evidence_section(ui, index, &finding);
                                headers.push((
                                    response.header_response.rect,
                                    response.body_returned.is_some(),
                                ));
                            });
                        }
                    });
                },
            );
            headers
        };
        let click = |position: egui::Pos2| {
            render(vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ]);
            render(vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }]);
            render(vec![])
        };
        let initial = render(vec![]);
        assert!(!initial[0].1 && !initial[1].1);
        let first_open = click(initial[0].0.center());
        assert!(first_open[0].1 && !first_open[1].1);
        let both_open = click(first_open[1].0.center());
        assert!(both_open[0].1 && both_open[1].1);
        let first_closed = click(both_open[0].0.center());
        assert!(!first_closed[0].1 && first_closed[1].1);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum InputMode {
    Files,
    Paste,
}
#[derive(PartialEq, Clone, Copy)]
enum Page {
    Instances,
    Analyze,
    Mods,
    Lab,
    Settings,
}
enum Event {
    Local(Result<(Report, bool)>),
    Cloud(Result<(AiReport, bool)>),
    Pick(Option<PathBuf>),
    Instances(Vec<Instance>, Vec<(String, egui::ColorImage)>),
    Export(Result<()>),
    Plan(Result<String>),
    Projects(Result<Vec<crate::versions::Project>>),
    Releases(
        Result<Vec<crate::versions::Release>>,
        String,
        Result<Option<egui::ColorImage>>,
    ),
    Icons(Vec<(String, egui::ColorImage)>),
    RepairPlan(Result<Box<crate::repair::Plan>>),
    RepairResult(Result<crate::repair::Receipt>),
    ReplacementJar(Option<PathBuf>),
    Cleared(Result<()>),
}
struct Job {
    receiver: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    label: String,
}

pub struct Doctor {
    report_cache: Arc<std::sync::Mutex<crate::cache::Reports>>,
    icons: icons::Textures,
    instance_icons: icons::Textures,
    issue_search: String,
    repair: repair_panel::Panel,
    cloud_open: bool,
    logo: egui::TextureHandle,
    input_mode: InputMode,
    console: String,
    input_open: bool,
    launcher_filter: String,
    instance_search: String,
    lookup: lookup::Lookup,
    page: Page,
    path: String,
    custom_root: String,
    instances: Vec<Instance>,
    report: Option<Report>,
    selected_finding: usize,
    ai: Option<AiReport>,
    sent_pack: Option<privacy::Pack>,
    provider: Provider,
    google_model: String,
    groq_model: String,
    key_input: String,
    preview: String,
    pack: Option<privacy::Pack>,
    consent: bool,
    message: String,
    lab_plan: String,
    job: Option<Job>,
    smoke_frames: Option<usize>,
}

impl Doctor {
    fn new(
        context: &egui::Context,
        path: Option<PathBuf>,
        smoke: bool,
        preloaded: Option<Report>,
    ) -> Self {
        theme::apply(context);
        let initial = path
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut app = Self {
            report_cache: Arc::new(std::sync::Mutex::new(crate::cache::Reports::default())),
            icons: icons::Textures::default(),
            instance_icons: icons::Textures::default(),
            issue_search: String::new(),
            repair: repair_panel::Panel::default(),
            cloud_open: false,
            logo: context.load_texture("chroma-ring", theme::logo(), egui::TextureOptions::LINEAR),
            input_mode: InputMode::Files,
            console: String::new(),
            input_open: preloaded.is_none(),
            launcher_filter: "All".into(),
            instance_search: String::new(),
            lookup: lookup::Lookup::default(),
            page: if initial.is_empty() {
                Page::Instances
            } else {
                Page::Analyze
            },
            path: initial,
            custom_root: String::new(),
            instances: vec![],
            report: preloaded,
            selected_finding: 0,
            ai: None,
            sent_pack: None,
            provider: Provider::Google,
            google_model: Provider::Google.model().into(),
            groq_model: Provider::Groq.model().into(),
            key_input: String::new(),
            preview: String::new(),
            pack: None,
            consent: false,
            message: String::new(),
            lab_plan: String::new(),
            job: None,
            smoke_frames: smoke.then_some(0),
        };
        if let Some(report) = &app.report {
            app.selected_finding = crate::triage::first(report);
            if smoke {
                app.icons
                    .update(context, icons::load_report(report, &AtomicBool::new(false)));
                if let Some(path) = std::env::var_os("PRISM_DOCTOR_SMOKE_ICON")
                    && let Ok(image) = icons::file_image(std::path::Path::new(&path))
                    && let Some(id) = report
                        .findings
                        .get(app.selected_finding)
                        .and_then(crate::triage::subject)
                {
                    app.icons.insert(context, id, image);
                }
            }
            app.message = "Analysis complete.".into();
            if smoke && std::env::var_os("PRISM_DOCTOR_SMOKE_REPAIR").is_some() {
                app.repair.open = true;
                match crate::repair::plan(report, None, None) {
                    Ok(plan) => app.repair.plan = Some(plan),
                    Err(e) => app.repair.message = e,
                }
            }
        } else if !app.path.is_empty() {
            app.import(context);
        } else if smoke {
            app.instances = discovery::discover(None);
            app.instance_icons
                .update(context, icons::instance_images(&app.instances));
        } else {
            app.refresh(context);
        }
        app
    }
    fn model(&self) -> &str {
        match self.provider {
            Provider::Google => &self.google_model,
            Provider::Groq => &self.groq_model,
        }
    }
    fn spawn(
        &mut self,
        ctx: &egui::Context,
        label: &str,
        work: impl FnOnce(Arc<AtomicBool>) -> Event + Send + 'static,
    ) {
        if self.job.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = work(token);
            if sender.send(result).is_ok() {
                repaint.request_repaint();
            }
        });
        self.job = Some(Job {
            receiver,
            cancel,
            label: label.into(),
        });
    }
    fn refresh(&mut self, ctx: &egui::Context) {
        let root =
            (!self.custom_root.trim().is_empty()).then(|| PathBuf::from(self.custom_root.trim()));
        self.spawn(ctx, "Discovering local profiles…", move |_| {
            let instances = discovery::discover(root.as_deref());
            let images = icons::instance_images(&instances);
            Event::Instances(instances, images)
        });
    }
    fn import(&mut self, ctx: &egui::Context) {
        if self.job.is_some() {
            return;
        }
        self.report = None;
        self.repair.reset_plan();
        self.repair.open = false;
        self.issue_search.clear();
        self.lookup.reset();
        self.ai = None;
        self.sent_pack = None;
        self.preview.clear();
        self.pack = None;
        self.consent = false;
        self.message.clear();
        let path = PathBuf::from(self.path.trim());
        let cache = self.report_cache.clone();
        if self.input_mode == InputMode::Paste {
            let text = self.console.clone();
            self.spawn(ctx, "Analyzing pasted console…", move |cancel| {
                if cancel.load(Ordering::Relaxed) {
                    Event::Local(Err("Cancelled".into()))
                } else {
                    Event::Local(
                        cache
                            .lock()
                            .map_err(|_| "Report cache lock failed".to_string())
                            .and_then(|mut cache| cache.text(&text)),
                    )
                }
            });
        } else {
            self.spawn(ctx, "Reading logs and mod metadata…", move |cancel| {
                Event::Local(
                    cache
                        .lock()
                        .map_err(|_| "Report cache lock failed".to_string())
                        .and_then(|mut cache| cache.analyze(&path, &cancel)),
                )
            });
        }
    }
    fn pick(&mut self, ctx: &egui::Context, folder: bool) {
        self.spawn(ctx, "Choose an import…", move |_| {
            Event::Pick(if folder {
                rfd::FileDialog::new().pick_folder()
            } else {
                rfd::FileDialog::new()
                    .add_filter("Minecraft log or crash report", &["log", "txt"])
                    .pick_file()
            })
        });
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let result = self.job.as_ref().map(|job| job.receiver.try_recv());
        if let Some(Ok(event)) = result {
            let cancelled = self
                .job
                .as_ref()
                .is_some_and(|j| j.cancel.load(Ordering::Relaxed));
            self.job = None;
            if cancelled && !matches!(&event, Event::RepairResult(_)) {
                self.lookup.message = "Lookup cancelled. Offline findings are unchanged.".into();
                self.message =
                    "Cancelled. Any cloud payload already sent cannot be recalled.".into();
                return;
            }
            match event {
                Event::Local(result) => match result {
                    Ok((report, cached)) => {
                        self.input_open = false;
                        self.selected_finding = crate::triage::first(&report);
                        self.message = format!(
                            "{} · {} logs · {} mods",
                            if cached {
                                "Loaded unchanged analysis from cache"
                            } else {
                                "Analysis complete"
                            },
                            report.logs.len(),
                            report.mods.len()
                        );
                        let icon_report = report.clone();
                        self.report = Some(report);
                        self.spawn(ctx, "Loading bundled mod icons…", move |cancel| {
                            Event::Icons(icons::load_report(&icon_report, &cancel))
                        });
                    }
                    Err(e) => self.message = privacy::redact(&e),
                },
                Event::Cloud(result) => match result {
                    Ok((ai, cached)) => {
                        self.ai = Some(ai);
                        self.message = if cached {
                            "Loaded cached AI explanation; no API request.".into()
                        } else {
                            "AI explanation received and cached.".into()
                        };
                    }
                    Err(e) => self.message = e,
                },
                Event::Pick(path) => {
                    if let Some(path) = path {
                        self.input_mode = InputMode::Files;
                        self.path = path.to_string_lossy().into_owned();
                        self.page = Page::Analyze;
                        self.import(ctx);
                    }
                }
                Event::Instances(instances, images) => {
                    self.instances = instances;
                    self.instance_icons.update(ctx, images);
                }
                Event::Icons(images) => self.icons.update(ctx, images),
                Event::RepairPlan(result) => match result {
                    Ok(plan) => {
                        self.repair.plan = Some(*plan);
                        self.repair.message.clear();
                    }
                    Err(e) => self.repair.message = e,
                },
                Event::RepairResult(result) => match result {
                    Ok(receipt) => {
                        self.repair.message = if receipt.rolled_back {
                            "Rollback complete.".into()
                        } else {
                            format!("Applied {} changes in the repair copy.", receipt.changes)
                        };
                        self.repair.receipt = Some(receipt);
                    }
                    Err(e) => {
                        self.repair.message = e;
                        match crate::repair::latest(&crate::repair::workspace()) {
                            Ok(receipt) => self.repair.receipt = receipt,
                            Err(e) => self.message = e,
                        }
                    }
                },
                Event::ReplacementJar(path) => self.repair.selected_jar = path,
                Event::Cleared(result) => {
                    self.message = match result {
                        Ok(()) => "Saved AI and metadata caches cleared.".into(),
                        Err(e) => e,
                    }
                }
                Event::Projects(result) => match result {
                    Ok(projects) => {
                        self.lookup.message = if projects.is_empty() {
                            "No matching projects found. Correct the lookup details or consult the mod's official page.".into()
                        } else {
                            "Choose the correct project and author before checking releases.".into()
                        };
                        self.lookup.projects = projects;
                    }
                    Err(e) => self.lookup.message = e,
                },
                Event::Releases(result, id, icon) => {
                    match icon {
                        Ok(Some(image)) => self.icons.insert(ctx, id, image),
                        Ok(None) => (),
                        Err(e) => self.message = format!("Project artwork unavailable: {e}"),
                    };
                    match result {
                        Ok(releases) => {
                            self.lookup.message = if releases.is_empty() {
                                "No published release matched this game, loader and supported version requirement. Do not install an arbitrary newer version.".into()
                            } else {
                                self.lookup.projects.clear();
                                String::new()
                            };
                            self.lookup.releases = releases;
                        }
                        Err(e) => self.lookup.message = e,
                    }
                }
                Event::Export(result) => {
                    self.message = match result {
                        Ok(()) => "Redacted report exported.".into(),
                        Err(e) => e,
                    }
                }
                Event::Plan(result) => match result {
                    Ok(text) => self.lab_plan = text,
                    Err(e) => self.message = privacy::redact(&e),
                },
            }
        } else if let Some(Err(mpsc::TryRecvError::Disconnected)) = result {
            self.job = None;
            self.message = "Background operation ended unexpectedly.".into();
        }
    }
    fn export(&mut self, ctx: &egui::Context, json: bool) {
        let Some(report) = self.report.as_ref() else {
            return;
        };
        let text = if json {
            match privacy::report_json(report) {
                Ok(s) => s,
                Err(e) => {
                    self.message = e;
                    return;
                }
            }
        } else {
            report.markdown()
        };
        let mut protected = self
            .instances
            .iter()
            .map(|instance| instance.path.clone())
            .collect::<Vec<_>>();
        let selected = PathBuf::from(self.path.trim());
        if selected.is_dir() {
            protected.push(selected);
        } else if let Some(parent) = selected.parent() {
            protected.push(parent.to_path_buf());
        }
        self.spawn(ctx, "Choose report destination…", move |_| {
            let extension = if json { "json" } else { "md" };
            let file = rfd::FileDialog::new()
                .set_file_name(format!("minecraft-ai-doctor-report.{extension}"))
                .save_file();
            Event::Export(match file {
                None => Err("Export cancelled".into()),
                Some(path) => export_protected(&path, &text, &protected),
            })
        });
    }
    fn instances_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Your instances");
        ui.label(RichText::new("Choose a profile to inspect, or start with a log.").color(MUTED));
        ui.add_space(16.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Import a log").clicked() {
                self.pick(ctx, false);
            }
            if ui.button("Paste console output").clicked() {
                self.page = Page::Analyze;
                self.input_mode = InputMode::Paste;
                self.input_open = true;
            }
            if ui.button("Open a folder").clicked() {
                self.pick(ctx, true);
            }
        });
        ui.add_space(20.0);
        ui.horizontal_wrapped(|ui| {
            for launcher in discovery::LAUNCHERS {
                let count = self
                    .instances
                    .iter()
                    .filter(|i| launcher == "All" || i.launcher == launcher)
                    .count();
                ui.selectable_value(
                    &mut self.launcher_filter,
                    launcher.into(),
                    format!("{launcher}  {count}"),
                );
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.instance_search)
                    .hint_text("Search profiles…")
                    .desired_width((ui.available_width() - 110.0).max(120.0)),
            );
            if ui.button("Refresh").clicked() {
                self.refresh(ctx);
            }
        });
        ui.add_space(12.0);
        let mut selected = None;
        let mut visible = 0;
        let search = self.instance_search.trim().to_lowercase();
        for (index, instance) in self.instances.iter().enumerate() {
            if self.launcher_filter != "All" && instance.launcher != self.launcher_filter
                || !instance.name.to_lowercase().contains(&search)
            {
                continue;
            }
            visible += 1;
            ui.push_id(("instance", index), |ui| {
                ui.separator();
                ui.horizontal(|ui| {
                    self.instance_icons.show_named(
                        ui,
                        &instance.path.to_string_lossy(),
                        &instance.name,
                        44.0,
                    );
                    let text_width = (ui.available_width() - 120.0).max(100.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(text_width, 56.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(text_width);
                            ui.label(RichText::new(&instance.name).size(17.0).strong())
                                .on_hover_text(instance.path.display().to_string());
                            ui.label(
                                RichText::new(format!("{} · Local profile", instance.launcher))
                                    .small()
                                    .color(MUTED),
                            );
                        },
                    );
                    if ui.button("Analyze").clicked() {
                        selected = Some(index);
                    }
                });
            });
        }
        if visible == 0 {
            ui.add_space(24.0);
            ui.label(
                "No matching profiles. Import a log or choose a custom folder to get started.",
            );
        }
        ui.add_space(16.0);
        egui::CollapsingHeader::new("Portable or custom launcher root")
            .id_salt("custom_root")
            .show(ui, |ui| {
                ui.label("Enter the directory containing your profiles / instances.");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.custom_root);
                    if ui.button("Add root").clicked() {
                        self.refresh(ctx);
                    }
                });
            });
        if let Some(index) = selected {
            self.input_mode = InputMode::Files;
            self.path = self.instances[index].path.to_string_lossy().into_owned();
            self.page = Page::Analyze;
            self.import(ctx);
        }
    }
    fn analyze_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading(if self.report.is_some() {
            "Diagnosis"
        } else {
            "What went wrong?"
        });
        ui.add_space(8.0);
        if self.input_open || self.report.is_none() {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.input_mode, InputMode::Files, "File or instance");
                ui.selectable_value(&mut self.input_mode, InputMode::Paste, "Paste console");
            });
            ui.add_space(8.0);
            if self.input_mode == InputMode::Paste {
                ui.label(
                    RichText::new(
                        "Paste the console output here (Ctrl+V). It stays on this computer.",
                    )
                    .color(MUTED),
                );
                egui::ScrollArea::vertical()
                    .id_salt("paste_editor_scroll")
                    .max_height(180.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.console)
                                .hint_text("Paste Minecraft or launcher console output…")
                                .font(egui::TextStyle::Monospace)
                                .char_limit(crate::logs::MAX_BYTES)
                                .desired_rows(7)
                                .desired_width(f32::INFINITY),
                        );
                    });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.console.trim().is_empty(),
                            egui::Button::new("Analyze console output").fill(theme::SELECTED),
                        )
                        .clicked()
                    {
                        self.import(ctx);
                    }
                    if ui.button("Clear text").clicked() {
                        self.console.clear();
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.path)
                            .hint_text("Drop a log here, or enter a local path")
                            .desired_width((ui.available_width() - 210.0).max(120.0)),
                    );
                    ui.menu_button("Browse…", |ui| {
                        if ui.button("Log or crash report").clicked() {
                            ui.close();
                            self.pick(ctx, false);
                        }
                        if ui.button("Instance or mods folder").clicked() {
                            ui.close();
                            self.pick(ctx, true);
                        }
                    });
                    if ui
                        .add(egui::Button::new("Analyze").fill(theme::SELECTED))
                        .clicked()
                    {
                        self.import(ctx);
                    }
                });
            }
        } else {
            ui.horizontal_wrapped(|ui| {
                let name = if self.input_mode == InputMode::Paste {
                    "Pasted console".into()
                } else {
                    PathBuf::from(&self.path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                };
                ui.label(RichText::new(name).color(MUTED)).on_hover_text(
                    if self.input_mode == InputMode::Files {
                        self.path.as_str()
                    } else {
                        "Console text was analyzed in memory"
                    },
                );
                if ui.small_button("Change input").clicked() {
                    self.input_open = true;
                }
            });
        }
        if self.report.is_none() {
            ui.add_space(24.0);
            ui.label(RichText::new("Start with the output from the failed launch.").size(20.0));
            ui.label(RichText::new("We'll identify the affected mods, explain the evidence, and help you find the next step. No AI key required.").color(MUTED));
            return;
        }
        ui.add_space(10.0);
        let mut prepare = false;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(egui::Button::new("Auto-fix · review repair copy").fill(theme::SELECTED))
                .clicked()
            {
                self.repair.open = !self.repair.open;
                prepare = self.repair.open && self.repair.plan.is_none();
                if self.repair.receipt.is_none() {
                    match crate::repair::latest(&crate::repair::workspace()) {
                        Ok(receipt) => self.repair.receipt = receipt,
                        Err(e) => self.repair.message = e,
                    }
                }
            }
            if self.repair.receipt.as_ref().is_some_and(|r| !r.rolled_back)
                && ui.button("Review rollback").clicked()
            {
                self.repair.open = true;
            }
            ui.label(
                RichText::new("Your original instance stays untouched")
                    .small()
                    .color(MUTED),
            );
        });
        let repair_action = self.repair.show(ui);
        if prepare {
            self.repair_action(ctx, repair_panel::Action::Prepare);
        }
        if let Some(action) = repair_action {
            self.repair_action(ctx, action);
        }
        let report = self.report.as_ref().expect("checked report");
        let mut export_md = false;
        let mut export_json = false;
        ui.horizontal_wrapped(|ui| {
            let issues = report
                .findings
                .iter()
                .filter(|f| crate::triage::category(f) != crate::triage::Category::Observations)
                .count();
            ui.label(
                RichText::new(format!(
                    "{issues} issue{} to review · {} logs · {} mods",
                    if issues == 1 { "" } else { "s" },
                    report.logs.len(),
                    report.mods.len()
                ))
                .color(MUTED),
            );
            ui.menu_button("Report", |ui| {
                if ui.button("Copy redacted report").clicked() {
                    ctx.copy_text(report.markdown());
                    ui.close();
                }
                if ui.button("Export Markdown…").clicked() {
                    export_md = true;
                    ui.close();
                }
                if ui.button("Export JSON…").clicked() {
                    export_json = true;
                    ui.close();
                }
            });
        });
        ui.add_space(8.0);
        analysis::choose(
            ui,
            report,
            &mut self.selected_finding,
            &mut self.issue_search,
            &self.icons,
        );
        let mut request = None;
        let width = ui.available_width();
        if width >= 880.0 {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(width - 352.0, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(width - 352.0);
                        if let Some(error) =
                            analysis::detail(ui, report, self.selected_finding, &self.icons)
                        {
                            self.message = error;
                        }
                    },
                );
                ui.add_space(8.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(332.0, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(332.0);
                        request = self
                            .lookup
                            .show(ui, report, self.selected_finding, &self.icons);
                    },
                );
            });
        } else {
            egui::CollapsingHeader::new("Find a replacement mod · optional Modrinth lookup")
                .id_salt("narrow_version_guidance")
                .show(ui, |ui| {
                    request = self
                        .lookup
                        .show(ui, report, self.selected_finding, &self.icons);
                });
            if let Some(error) = analysis::detail(ui, report, self.selected_finding, &self.icons) {
                self.message = error;
            }
        }
        if let Some(request) = request {
            match request {
                lookup::Request::ReviewRepair => {
                    self.repair.open = true;
                    self.repair_action(ctx, repair_panel::Action::Prepare);
                }
                lookup::Request::Search(target) => {
                    self.spawn(ctx, "Finding matching mod projects…", move |token| {
                        Event::Projects(crate::versions::search(&target, &token))
                    })
                }
                lookup::Request::Releases(project, target) => {
                    self.spawn(ctx, "Checking published mod versions…", move |token| {
                        let icon = project
                            .icon_url
                            .as_deref()
                            .map(|url| icons::download(&target.query, url, &token))
                            .transpose();
                        Event::Releases(
                            crate::versions::releases(&project.project_id, &target, &token),
                            target.query.clone(),
                            icon,
                        )
                    })
                }
            }
        }
        if export_md {
            self.export(ctx, false);
        }
        if export_json {
            self.export(ctx, true);
        }
        ui.add_space(14.0);
        ui.separator();
        egui::CollapsingHeader::new("Optional AI explanation")
            .id_salt("cloud_explanation")
            .open(self.cloud_open.then_some(true))
            .show(ui, |ui| self.cloud_ui(ui, ctx));
        self.cloud_open = false;
    }
    fn repair_action(&mut self, ctx: &egui::Context, action: repair_panel::Action) {
        use repair_panel::Action;
        match action {
            Action::Prepare => {
                self.repair.reset_plan();
                let Some(report) = self.report.clone() else {
                    return;
                };
                let replacement = match self.lookup.replacement(&report) {
                    Ok(value) => value,
                    Err(e) => {
                        self.repair.message = e;
                        return;
                    }
                };
                let indices = self
                    .ai
                    .as_ref()
                    .map(|ai| ai.proposed_repair_indices.clone())
                    .filter(|v| !v.is_empty());
                self.repair.ai_proposed = indices.is_some();
                self.repair.selected_jar = None;
                self.spawn(ctx, "Verifying repair candidates…", move |_| {
                    Event::RepairPlan(
                        crate::repair::plan(&report, indices.as_deref(), replacement).map(Box::new),
                    )
                });
            }
            Action::AskAi => {
                self.prepare_payload();
                self.cloud_open = true;
                self.repair.message="Review the exact payload under Optional AI explanation, then confirm Send. Rebuild this plan after the explanation arrives.".into();
            }
            Action::ChooseJar => self.spawn(ctx, "Choose verified replacement JAR…", move |_| {
                Event::ReplacementJar(
                    rfd::FileDialog::new()
                        .add_filter("Mod JAR", &["jar"])
                        .pick_file(),
                )
            }),
            Action::Apply(plan, jar, download) => self.spawn(
                ctx,
                "Preparing and repairing a separate mod copy…",
                move |cancel| {
                    Event::RepairResult(crate::repair::apply_reviewed(
                        &plan,
                        jar.as_deref(),
                        download,
                        &cancel,
                    ))
                },
            ),
            Action::Rollback(root) => {
                self.spawn(ctx, "Restoring repair-copy backups…", move |_| {
                    Event::RepairResult(crate::repair::rollback(&crate::repair::workspace(), &root))
                })
            }
            Action::AnalyzeCopy(path) => {
                self.input_mode = InputMode::Files;
                self.path = path.to_string_lossy().into_owned();
                self.import(ctx);
            }
        }
    }
    fn prepare_payload(&mut self) {
        let Some(report) = &self.report else { return };
        let pack = privacy::pack(report);
        self.preview = serde_json::to_string_pretty(&self.provider.body(self.model(), &pack))
            .expect("serializable request");
        self.pack = Some(pack);
        self.consent = false;
    }
    fn cloud_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label("Ask the selected provider to explain the evidence. Review the redacted payload and approve each Send. Your API key is sent separately in an authentication header.");
        ui.label(format!("{} · {}", self.provider.name(), self.model()));
        if ui.button("Prepare redacted payload").clicked() {
            self.prepare_payload();
        }
        if !self.preview.is_empty() {
            ui.collapsing("Exact request JSON — inspect before sending", |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.preview.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(12)
                        .desired_width(f32::INFINITY),
                );
            });
            ui.checkbox(&mut self.consent,"I reviewed this payload and consent to sending it to the selected provider for this request.");
            if ui
                .add_enabled(
                    self.consent && self.job.is_none(),
                    egui::Button::new("Send this payload"),
                )
                .clicked()
            {
                let provider = self.provider;
                let model = self.model().to_owned();
                let pack = self.pack.clone().expect("preview has pack");
                self.sent_pack = Some(pack.clone());
                self.ai = None;
                self.consent = false;
                self.preview.clear();
                self.pack = None;
                self.spawn(ctx, "Waiting for cloud explanation…", move |cancel| {
                    Event::Cloud(providers::analyze_cached(provider, model, pack, cancel))
                });
            }
        }
        if let Some(ai) = &self.ai {
            ui.add_space(12.0);
            ui.label(providers::display(ai));
            if let Some(pack) = &self.sent_pack {
                ui.collapsing("Validated AI citation map", |ui| {
                    for e in &pack.evidence {
                        ui.label(format!("{} → {}:{}", e.id, e.source, e.line));
                        ui.monospace(&e.text);
                    }
                });
            }
        }
    }
    fn mods_ui(&self, ui: &mut egui::Ui) {
        ui.heading("Mods & dependencies");
        ui.label(RichText::new("Declared metadata, read directly from local JARs. No runtime compatibility guarantee.").color(MUTED));
        let Some(report) = &self.report else {
            ui.label("Analyze an instance or mods directory to inspect metadata.");
            return;
        };
        if report.mods.is_empty() {
            ui.label("No mod metadata available. Log-only diagnosis still works.");
        }
        for (index, m) in report.mods.iter().enumerate() {
            egui::CollapsingHeader::new(format!("{}  {}  ·  {}", m.id, m.version, m.loader))
                .id_salt(("mod_metadata", index))
                .show(ui, |ui| {
                    ui.label(format!("{}!{}:{}", m.source, m.descriptor, m.line));
                    if m.dependencies.is_empty() {
                        ui.label("No declared dependency entries.");
                    }
                    for d in &m.dependencies {
                        ui.label(format!("{}  {}  {}", d.kind, d.id, d.requirement));
                    }
                });
        }
    }
    fn settings_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Settings");
        ui.label("API credentials stay in Windows Credential Manager. Environment variables are a fallback. No keys are saved in app configuration or reports.");
        let old = self.provider;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.provider, Provider::Google, "Google / Gemma");
            ui.selectable_value(&mut self.provider, Provider::Groq, "Groq / Qwen");
        });
        ui.label("Model ID (session setting)");
        let changed = match self.provider {
            Provider::Google => ui.text_edit_singleline(&mut self.google_model).changed(),
            Provider::Groq => ui.text_edit_singleline(&mut self.groq_model).changed(),
        };
        if changed || old != self.provider {
            self.preview.clear();
            self.pack = None;
            self.consent = false;
            self.key_input.clear();
        }
        ui.add_space(14.0);
        ui.label("API key");
        ui.add(
            egui::TextEdit::singleline(&mut self.key_input)
                .password(true)
                .hint_text("Enter key here, never in chat")
                .desired_width(420.0),
        );
        ui.horizontal(|ui| {
            if ui.button("Save securely").clicked() {
                self.message = match self.provider.save_key(&self.key_input) {
                    Ok(()) => {
                        self.key_input.clear();
                        "Key saved in Credential Manager.".into()
                    }
                    Err(e) => e,
                };
            }
            if ui.button("Remove saved key").clicked() {
                self.message = match self.provider.delete_key() {
                    Ok(()) => "Saved key removed. Environment fallback may still apply.".into(),
                    Err(e) => e,
                };
            }
        });
        ui.label(format!("Environment fallback: {}", self.provider.env()));
        ui.add_space(18.0);
        ui.label("Provider access, pricing and quotas depend on your account. Cloud explanations are unverified interpretations; the app validates citation IDs and structured mod/version claims.");
        ui.label("Hook: use --from-prism to open after every exit, or add --only-crashes to filter fresh latest.log failure signals. See docs/PRISM_SETUP.md. No hooks are installed automatically.");
        ui.add_space(18.0);
        ui.label("Analysis cache");
        ui.label("Unchanged local reports are reused during this session. Redacted AI explanations are saved for 7 days; public mod metadata for 6 hours. API keys and raw logs are never written to these caches.");
        if ui.button("Clear saved API results").clicked() {
            self.spawn(ctx, "Clearing saved API results…", move |_| {
                Event::Cleared(crate::cache::Disk::user().clear())
            });
        }
    }
    fn lab_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Compatibility Lab");
        ui.label(RichText::new("EXPERIMENTAL · Read-only planning").color(ACCENT));
        ui.label("Generate a SHA-256 inventory and a controlled experiment plan. The Doctor does not create or mutate a game instance, download mods, launch Minecraft or accept EULAs.");
        ui.text_edit_singleline(&mut self.path);
        if ui.button("Build inventory & experiment plan").clicked() {
            let path = PathBuf::from(self.path.trim());
            self.spawn(ctx, "Hashing selected mod inventory…", move |_| {
                Event::Plan(
                    crate::lab::plan(&path)
                        .and_then(|p| serde_json::to_string_pretty(&p).map_err(|e| e.to_string())),
                )
            });
        }
        if !self.lab_plan.is_empty() {
            if ui.button("Copy plan").clicked() {
                ctx.copy_text(self.lab_plan.clone());
            }
            ui.monospace(&self.lab_plan);
        }
    }
}

impl Drop for Doctor {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl eframe::App for Doctor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
        if let Some(count) = &mut self.smoke_frames {
            // Wait for import/discovery so the smoke run exercises the actual result UI.
            if self.job.is_none() {
                *count += 1;
                if *count >= 3 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped
            && self.job.is_none()
        {
            self.input_mode = InputMode::Files;
            self.path = path.to_string_lossy().into_owned();
            self.page = Page::Analyze;
            self.import(ctx);
        }
        egui::SidePanel::left("navigation")
            .exact_width(200.0)
            .frame(egui::Frame::new().fill(theme::SURFACE).inner_margin(18))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.image((self.logo.id(), egui::vec2(38.0, 38.0)));
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Minecraft").size(20.0).strong());
                        ui.label(RichText::new("Ai Doctor").size(13.0).color(MUTED));
                    });
                });
                ui.add_space(30.0);
                for (page, label) in [
                    (Page::Instances, "Instances"),
                    (Page::Analyze, "Analyze"),
                    (Page::Mods, "Mods"),
                    (Page::Settings, "Settings"),
                ] {
                    ui.add_enabled_ui(self.job.is_none(), |ui| {
                        if ui
                            .add_sized(
                                [ui.available_width(), 38.0],
                                egui::Button::selectable(self.page == page, label),
                            )
                            .clicked()
                        {
                            self.page = page;
                        }
                    });
                }
                ui.add_space(30.0);
                ui.separator();
                if ui
                    .selectable_label(self.page == Page::Lab, "Compatibility Lab")
                    .clicked()
                {
                    self.page = Page::Lab;
                }
                ui.add_space(20.0);
                ui.label(
                    RichText::new("Local analysis.\nYour instances stay untouched.")
                        .small()
                        .color(MUTED),
                );
            });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            if let Some(job) = &self.job {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(&job.label);
                    if ui.button("Cancel").clicked() {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                });
            }
            ui.label(privacy::redact(&self.message));
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(28))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(4.0);
                    ui.add_enabled_ui(self.job.is_none(), |ui| match self.page {
                        Page::Instances => self.instances_ui(ui, ctx),
                        Page::Analyze => self.analyze_ui(ui, ctx),
                        Page::Mods => self.mods_ui(ui),
                        Page::Lab => self.lab_ui(ui, ctx),
                        Page::Settings => self.settings_ui(ui, ctx),
                    });
                });
            });
    }
}

pub fn export_new(path: &std::path::Path, text: &str) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or("Export requires a parent folder")?;
    discovery::safe_path(parent)?;
    let safe = path.to_string_lossy().to_lowercase();
    if safe.contains("prismlauncher")
        || safe.contains(".lunarclient")
        || safe.contains(".minecraft")
    {
        return Err("Choose a report destination outside launcher profile folders.".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("Cannot create report (existing files are never overwritten): {e}"))?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

pub fn export_protected(path: &std::path::Path, text: &str, protected: &[PathBuf]) -> Result<()> {
    let parent = path.parent().ok_or("Export requires a parent folder")?;
    let destination = parent.canonicalize().map_err(|e| e.to_string())?;
    for root in protected {
        if let Ok(root) = root.canonicalize()
            && destination.starts_with(root)
        {
            return Err("Choose an export destination outside discovered profiles and the imported source folder.".into());
        }
    }
    export_new(path, text)
}

pub fn run(path: Option<PathBuf>, smoke: bool) -> Result<()> {
    // Preload only the rendering harness so screenshots contain results, not a loading race.
    let preloaded = if smoke {
        path.as_deref().map(crate::analyze).transpose()?
    } else {
        None
    };
    let smoke_size = if smoke {
        std::env::var("PRISM_DOCTOR_SMOKE_SIZE")
            .ok()
            .and_then(|s| {
                s.split_once('x')
                    .and_then(|(w, h)| Some([w.parse::<u16>().ok()?, h.parse::<u16>().ok()?]))
            })
            .filter(|[w, h]| (900..=3840).contains(w) && (640..=2160).contains(h))
            .map(|[w, h]| [f32::from(w), f32::from(h)])
    } else {
        None
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(Arc::new(theme::icon()))
            .with_inner_size(smoke_size.unwrap_or([1280.0, 880.0]))
            .with_min_inner_size([900.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Minecraft Ai Doctor",
        options,
        Box::new(move |cc| Ok(Box::new(Doctor::new(&cc.egui_ctx, path, smoke, preloaded)))),
    )
    .map_err(|e| format!("Desktop initialization failed: {e}"))
}
