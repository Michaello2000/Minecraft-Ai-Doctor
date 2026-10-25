use crate::{discovery, mods::Mod};
use eframe::egui::{self, RichText};
use std::{
    collections::HashMap,
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
pub(super) struct Textures(pub HashMap<String, egui::TextureHandle>);

fn read_icon(m: &Mod) -> crate::Result<Option<egui::ColorImage>> {
    let Some(name) = &m.icon else { return Ok(None) };
    if name.len() > 256
        || !name.ends_with(".png")
        || name.contains(['\\', ':'])
        || Path::new(name)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Ok(None);
    }
    discovery::safe_path(Path::new(&m.source))?;
    let file = std::fs::File::open(&m.source).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 128 * 1024 * 1024 {
        return Ok(None);
    }
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if archive.len() > 20_000 {
        return Ok(None);
    }
    let entry = match archive.by_name(name) {
        Ok(e) => e,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if entry.size() > 256 * 1024
        || entry.enclosed_name().is_none()
        || entry.size() > entry.compressed_size().saturating_mul(200).max(4096)
    {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    entry
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 256 * 1024 {
        return Ok(None);
    }
    decode(&bytes).map(Some)
}

pub(super) fn decode(bytes: &[u8]) -> crate::Result<egui::ColorImage> {
    if bytes.len() > 512 * 1024 {
        return Err("Icon exceeds the 512 KiB safety limit".into());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);
    limits.max_alloc = Some(8 * 1024 * 1024);
    reader.limits(limits);
    let rgba = reader
        .decode()
        .map_err(|e| e.to_string())?
        .resize(64, 64, image::imageops::FilterType::Triangle)
        .into_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

pub(super) fn instance_images(
    instances: &[discovery::Instance],
) -> Vec<(String, egui::ColorImage)> {
    let mut images = Vec::new();
    for instance in instances.iter().take(128) {
        if let Some(path) = &instance.icon {
            let result = file_image(path);
            if let Ok(image) = result {
                images.push((instance.path.to_string_lossy().into_owned(), image));
            }
        }
    }
    images
}

pub(super) fn file_image(path: &Path) -> crate::Result<egui::ColorImage> {
    discovery::safe_path(path)?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 512 * 1024 {
        return Err("Artwork file exceeds 512 KiB".into());
    }
    let mut bytes = Vec::new();
    file.take(512 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    decode(&bytes)
}

pub(super) fn load(mods: &[Mod], cancel: &AtomicBool) -> Vec<(String, egui::ColorImage)> {
    let mut result = Vec::new();
    for m in mods.iter().take(128) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if result.iter().any(|(id, _)| id == &m.id) {
            continue;
        }
        // Icons are optional artwork: malformed or unsupported images use the labelled fallback.
        if let Ok(Some(image)) = read_icon(m) {
            result.push((m.id.clone(), image));
        }
    }
    result
}

fn binding_key(id: &str) -> String {
    crate::cache::key(format!("confirmed-project-artwork-v1:{id}").as_bytes())
}
pub(super) fn download(
    id: &str,
    url: &str,
    cancel: &AtomicBool,
) -> crate::Result<egui::ColorImage> {
    let bytes = crate::versions::project_icon(url, cancel)?;
    let image = decode(&bytes)?;
    if let Err(e) = crate::cache::Disk::user().put("metadata", &binding_key(id), &url) {
        crate::cache::warning(&e);
    }
    Ok(image)
}
pub(super) fn load_report(
    report: &crate::Report,
    cancel: &AtomicBool,
) -> Vec<(String, egui::ColorImage)> {
    let mut images = load(&report.mods, cancel);
    let disk = crate::cache::Disk::user();
    for id in report
        .findings
        .iter()
        .take(128)
        .filter_map(crate::triage::subject)
    {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if images.iter().any(|(key, _)| key == &id) {
            continue;
        }
        let cached = (|| {
            let url = disk.get::<String>("metadata", &binding_key(&id), crate::cache::AI_TTL)?;
            let Some(url) = url else { return Ok(None) };
            let Some(bytes) = disk.get::<Vec<u8>>(
                "artwork",
                &crate::cache::key(url.as_bytes()),
                crate::cache::AI_TTL,
            )?
            else {
                return Ok(None);
            };
            decode(&bytes).map(Some)
        })();
        if let Ok(Some(image)) = cached {
            images.push((id, image));
        }
    }
    images
}
impl Textures {
    pub fn insert(&mut self, ctx: &egui::Context, id: String, image: egui::ColorImage) {
        self.0.insert(
            id.clone(),
            ctx.load_texture(format!("icon-{id}"), image, egui::TextureOptions::LINEAR),
        );
    }
    pub fn update(&mut self, ctx: &egui::Context, images: Vec<(String, egui::ColorImage)>) {
        self.0.clear();
        for (id, image) in images {
            self.0.insert(
                id.clone(),
                ctx.load_texture(format!("mod-{id}"), image, egui::TextureOptions::LINEAR),
            );
        }
    }
    pub fn show(&self, ui: &mut egui::Ui, id: &str, size: f32) {
        self.show_named(ui, id, id, size);
    }
    pub fn show_named(&self, ui: &mut egui::Ui, id: &str, label: &str, size: f32) {
        if let Some(texture) = self.0.get(id) {
            ui.image((texture.id(), egui::vec2(size, size)));
        } else {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            ui.painter().rect_filled(rect, 8, super::theme::SELECTED);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label
                    .chars()
                    .take(if label.starts_with(|c: char| c.is_ascii_digit()) {
                        6
                    } else {
                        2
                    })
                    .collect::<String>()
                    .to_uppercase(),
                egui::FontId::proportional(
                    size * if label.starts_with(|c: char| c.is_ascii_digit()) {
                        0.28
                    } else {
                        0.36
                    },
                ),
                super::theme::FOREGROUND,
            );
        }
    }
    pub fn transition(&self, ui: &mut egui::Ui, id: &str, current: &str, next: &str) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                self.show(ui, id, 40.0);
                ui.label(RichText::new(current).small());
            });
            let (rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 28.0), egui::Sense::hover());
            ui.painter().arrow(
                rect.left_center() + egui::vec2(5.0, 0.0),
                egui::vec2(32.0, 0.0),
                egui::Stroke::new(2.0_f32, super::theme::ACCENT),
            );
            ui.vertical(|ui| {
                self.show(ui, id, 40.0);
                ui.label(RichText::new(next).small().color(super::theme::ACCENT));
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_icon_is_bounded_and_unsafe_paths_fall_back() {
        use std::io::Write;
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("icon.jar");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
        zip.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"schemaVersion":1,"id":"example","version":"1.0.0","icon":"assets/icon.png"}"#,
        )
        .unwrap();
        zip.start_file("assets/icon.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(include_bytes!("../../assets/chroma-ring.png"))
            .unwrap();
        zip.finish().unwrap();
        let mut m = crate::mods::read_jar(&jar).unwrap().remove(0);
        let image = read_icon(&m).unwrap().unwrap();
        assert_eq!(image.size, [64, 64]);
        m.icon = Some("../outside.png".into());
        assert!(read_icon(&m).unwrap().is_none());
        m.icon = Some("missing.png".into());
        assert!(read_icon(&m).unwrap().is_none());
        assert!(load(&[m], &AtomicBool::new(true)).is_empty());
    }
    #[test]
    #[ignore = "Requires approved public Modrinth metadata and icon downloads"]
    fn live_nvidium_artwork() {
        let target = crate::versions::Target {
            query: "nvidium".into(),
            minecraft: "1.21.1".into(),
            loader: "fabric".into(),
            ..Default::default()
        };
        let cancel = AtomicBool::new(false);
        let project = crate::versions::search(&target, &cancel)
            .unwrap()
            .into_iter()
            .find(|p| p.slug == "nvidium" && p.author.eq_ignore_ascii_case("cortex"))
            .expect("Nvidium project by cortex");
        let url = project.icon_url.as_deref().expect("published icon URL");
        let image = download("nvidium", url, &cancel).unwrap();
        assert_eq!(image.size, [64, 64]);
        let bytes = crate::versions::project_icon(url, &cancel).unwrap();
        std::fs::write("target/qa-nvidium-icon", &bytes).unwrap();
        let report =
            crate::analyze_text(include_str!("../../tests/fixtures/sodium-conflict.log")).unwrap();
        assert!(
            load_report(&report, &cancel)
                .iter()
                .any(|(id, _)| id == "nvidium")
        );
        println!(
            "Verified Nvidium icon: {} bytes from {}, decoded 64 x 64 and reloaded from cache",
            bytes.len(),
            url
        );
    }
}
