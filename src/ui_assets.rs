use gpui_kit::{AssetSource, Result, SharedString, assets};
use std::borrow::Cow;
assets::icon_assets!(
    SidebarIcons,
    [
        Layers,
        CalendarDays,
        Clock,
        RefreshCw,
        Archive,
        Workflow,
        ChartNoAxesCombined,
        Code,
        Sun,
        Moon,
        Monitor
    ]
);
pub struct AppAssets;
impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == "brand/planning.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/brand/planning.svg"
            ))));
        }
        if let Some(asset) = SidebarIcons.load(path)? {
            return Ok(Some(asset));
        }
        assets::Assets.load(path)
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = assets::Assets.list(path)?;
        paths.extend(SidebarIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
