use gpui::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;

/// App-specific assets (custom icons, etc.)
#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "icons/**/*.svg"]
pub struct AppAssets;

/// Combined asset source: app assets first, then gpui-component assets.
pub struct CombinedAssets;

impl AssetSource for CombinedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        // Try app assets first
        if let Some(data) = AppAssets::get(path) {
            return Ok(Some(data.data));
        }
        // Fall back to gpui-component assets
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut items: Vec<SharedString> = AppAssets::iter()
            .filter(|p| p.starts_with(path))
            .map(|p| p.into())
            .collect();
        items.extend(gpui_component_assets::Assets.list(path)?);
        Ok(items)
    }
}
