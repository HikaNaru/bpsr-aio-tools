use std::collections::HashMap;

/// Loads and caches game icon PNGs (`data/Images/{category}/{name}.png`) as egui
/// textures. Misses are cached too (as `None`) so a sparse asset folder — most gear
/// item icons aren't bundled — doesn't re-hit disk every frame.
#[derive(Default)]
pub struct IconCache {
    textures: HashMap<(&'static str, String), Option<egui::TextureHandle>>,
}

impl IconCache {
    /// `category` is the subfolder under `data/Images` (e.g. "Items", "Skills",
    /// "Skills_Imagines"). `icon_path` is a data-table `Icon` field value — only the
    /// last path segment is used as the filename, matching the convention observed
    /// across every icon-bearing table (e.g. `ui/textures/skill_aoyi/skill_aoyi_skill_icon_001`
    /// → `skill_aoyi_skill_icon_001.png`).
    pub fn get(&mut self, ctx: &egui::Context, category: &'static str, icon_path: &str) -> Option<egui::TextureHandle> {
        let name = icon_path.rsplit('/').next().unwrap_or(icon_path).to_string();
        if name.is_empty() {
            return None;
        }
        let key = (category, name);
        if let Some(cached) = self.textures.get(&key) {
            return cached.clone();
        }

        let handle = Self::load(ctx, category, &key.1);
        self.textures.insert(key, handle.clone());
        handle
    }

    fn load(ctx: &egui::Context, category: &str, name: &str) -> Option<egui::TextureHandle> {
        let path = core::data_tables::data_dir().join("Images").join(category).join(format!("{name}.png"));
        let bytes = std::fs::read(&path).ok()?;
        let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let color_image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
        Some(ctx.load_texture(format!("icon:{category}/{name}"), color_image, egui::TextureOptions::LINEAR))
    }
}
