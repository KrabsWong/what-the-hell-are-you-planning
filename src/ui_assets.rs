use gpui_kit::{AssetSource, Result, SharedString, assets};
use std::borrow::Cow;
// Embed application icons in debug builds too; the default bundle reads from disk.
assets::icon_assets!(
    AppIcons,
    [
        Archive,
        ArrowLeft,
        ArrowRight,
        CalendarDays,
        ChartNoAxesCombined,
        ChevronDown,
        ChevronLeft,
        ChevronRight,
        ChevronUp,
        Clock,
        Close,
        Code,
        Copy,
        Ellipsis,
        ExternalLink,
        Folder,
        Layers,
        LayoutDashboard,
        Menu,
        Monitor,
        Moon,
        Pause,
        Pencil,
        Play,
        Plus,
        RefreshCw,
        Search,
        Settings,
        Settings2,
        Sun,
        Workflow
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
        if let Some(asset) = AppIcons.load(path)? {
            return Ok(Some(asset));
        }
        assets::Assets.load(path)
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = assets::Assets.list(path)?;
        paths.extend(AppIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_icons_are_embedded() {
        // Borrowed bytes prove these do not depend on the process working directory.
        for path in AppIcons.list("icons/").unwrap() {
            let bytes = AppAssets.load(path.as_ref()).unwrap().unwrap();
            assert!(matches!(bytes, Cow::Borrowed(_)), "{path} must be embedded");
            assert!(!bytes.is_empty(), "{path} must contain SVG data");
        }
        for icon in [
            assets::IconName::Play,
            assets::IconName::Folder,
            assets::IconName::Search,
            assets::IconName::Settings,
            assets::IconName::Pencil,
        ] {
            assert!(AppIcons.load(icon.path().as_ref()).unwrap().is_some());
        }
    }
}
