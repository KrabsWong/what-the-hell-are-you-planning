//! Native macOS 26+ window material, with an isolated QA preview.
use std::{cell::Cell, rc::Rc};
#[cfg(feature = "qa")]
use std::time::Duration;
use gpui_kit::*;
#[cfg(feature = "qa")]
use gpui_kit::test::TestWindowExt;
use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained, runtime::AnyClass};
use objc2_app_kit::{NSColor, NSGlassEffectView, NSGlassEffectViewStyle, NSView, NSWindowOrderingMode, NSWorkspace};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(crate) struct GlassPalette(pub bool);
impl Global for GlassPalette {}

pub(crate) struct NativeGlass {
    native_view: Retained<NSView>,
    pane: Retained<NSGlassEffectView>,
    enabled: Cell<bool>,
    forced_opaque: Cell<bool>,
    reduced_transparency: Cell<bool>,
}

impl NativeGlass {
    pub fn install(window: &Window) -> Option<Rc<Self>> {
        let mtm = MainThreadMarker::new()?;
        AnyClass::get(c"NSGlassEffectView")?;
        let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw() else {
            return None;
        };
        // GPUI owns this live NSView; retaining it keeps the bridge's reference valid.
        let native_view = unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>())? };
        // Accessed on the main thread while GPUI owns the native view hierarchy.
        let parent = unsafe { native_view.superview() }?;
        let pane = {
            let pane = NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), NSRect::ZERO);
            // Regular preserves legibility over other windows when this window is active.
            pane.setStyle(NSGlassEffectViewStyle::Regular);
            // One continuous material; the native window clips its outer corners.
            pane.setCornerRadius(0.);
            let content = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
            pane.setContentView(Some(&content));
            // Keep GPUI hit testing, text and popovers above the native underlay.
            parent.addSubview_positioned_relativeTo(&pane, NSWindowOrderingMode::Below, Some(&native_view));
            pane.setHidden(true);
            pane
        };
        Some(Rc::new(Self { native_view, pane, enabled: Cell::new(false), forced_opaque: Cell::new(false), reduced_transparency: Cell::new(false) }))
    }

    pub fn enabled(&self) -> bool { self.enabled.get() }

    pub fn unavailable_reason() -> Option<&'static str> {
        if AnyClass::get(c"NSGlassEffectView").is_none() {
            Some("Liquid Glass requires macOS 26 or later.")
        } else if NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency() {
            Some("Liquid Glass is unavailable while Reduce Transparency is enabled in macOS Accessibility settings.")
        } else {
            None
        }
    }

    pub fn refresh(&self, requested: bool, window: &Window, cx: &mut App) -> bool {
        let reduced = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency();
        let availability_changed = self.reduced_transparency.replace(reduced) != reduced;
        let enabled = requested && !self.forced_opaque.get() && !reduced;
        if self.enabled.replace(enabled) == enabled { return availability_changed; }
        window.set_background_appearance(if enabled {
            WindowBackgroundAppearance::Transparent
        } else {
            WindowBackgroundAppearance::Opaque
        });
        if enabled {
            // GPUI uses a near-black window backing to preserve its shadow.
            // A native material needs a genuinely clear sampling background.
            if let Some(native_window) = self.native_view.window() {
                native_window.setBackgroundColor(Some(&NSColor::clearColor()));
            }
        }
        self.pane.setHidden(!enabled);
        cx.set_global(GlassPalette(enabled));
        let mode = component::Theme::global(cx).mode;
        component::Theme::change(mode, None, cx);
        crate::appearance::apply_component_colors(cx);
        true
    }

    pub fn layout(&self, bounds: Bounds<Pixels>) {
        let Some(parent) = (unsafe { self.native_view.superview() }) else { return; };
        let height = f64::from(bounds.size.height.as_f32());
        let top = f64::from(bounds.origin.y.as_f32());
        let y = if self.native_view.isFlipped() { top } else { self.native_view.bounds().size.height - top - height };
        let rect = NSRect::new(
            NSPoint::new(f64::from(bounds.origin.x.as_f32()), y),
            NSSize::new(f64::from(bounds.size.width.as_f32()), height),
        );
        let rect = self.native_view.convertRect_toView(rect, Some(&parent));
        if self.pane.frame() != rect { self.pane.setFrame(rect); }
    }

    #[cfg(feature = "qa")]
    fn window_number(&self) -> isize {
        self.native_view.window().expect("preview window").windowNumber()
    }
}

impl Drop for NativeGlass {
    fn drop(&mut self) {
        // Retained NSView is main-thread-only; the owning TaskApp releases us there.
        self.pane.removeFromSuperview();
    }
}

#[cfg(feature = "qa")]
pub fn preview() {
    let verify = std::env::args().any(|arg| arg == "--glass-verify");
    let dark = std::env::args().any(|arg| arg == "--glass-dark");
    let opaque = std::env::args().any(|arg| arg == "--glass-opaque");
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(".local/native-glass/{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    application().with_assets(crate::ui_assets::AppAssets).run(move |cx| {
        init(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", crate::Quit, None)]);
        cx.on_action(|_: &crate::Quit, cx| cx.quit());
        let mut preview_app = None;
        let handle = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1200.), px(800.)), cx))),
            window_min_size: Some(size(px(1000.), px(680.))),
            titlebar: Some(TitlebarOptions {
                title: Some("Planning Glass Preview - isolated sample".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(20.), px(16.))),
                ..Default::default()
            }),
            app_owns_titlebar_drag: true,
            ..Default::default()
        }, |window, cx| {
            let app = cx.new(|cx| {
                let mut app = crate::app::TaskApp::new_mock(window, cx);
                // No production preferences, storage, agents or automation schedules are loaded.
                app.store = crate::domain::Store::empty();
                app.store.create_research_goal(
                    "Explore native glass navigation".into(),
                    "Compare the continuous native material and the softly tinted reading surface.".into(), 1,
                ).unwrap();
                let appearance = if dark { crate::appearance::Appearance::Dark } else { crate::appearance::Appearance::Light };
                app.store.set_appearance(appearance);
                app.store.set_liquid_glass(true);
                // Persist only this process-specific fixture so theme controls work normally.
                let storage = crate::persistence::Storage::new(directory.join("workspace.sqlite3"));
                storage.save(&app.store).unwrap();
                app.attach_storage(storage, window, cx);
                assert!(app.storage_error.is_empty(), "isolated preview storage: {}", app.storage_error);
                crate::appearance::apply(appearance, Some(window), cx);
                app.native_glass = NativeGlass::install(window);
                if let Some(glass) = &app.native_glass {
                    glass.forced_opaque.set(opaque);
                    glass.refresh(app.store.liquid_glass(), window, cx);
                    println!("NATIVE_GLASS window={} available=true enabled={} screenshot_dir={}", glass.window_number(), glass.enabled(), directory.display());
                } else {
                    println!("NATIVE_GLASS available=false fallback=opaque");
                }
                app
            });
            preview_app = Some(app.clone());
            cx.new(|cx| base::Root::new(app, window, cx).bg(transparent_black()))
        }).unwrap().into();
        let app = preview_app.unwrap();
        cx.activate(true);
        if verify {
            cx.spawn(async move |cx| {
                for (index, _name) in ["light", "dark", "compact", "menu", "opaque-fallback", "settings-off", "settings-on"].iter().enumerate() {
                    cx.background_executor().timer(Duration::from_millis(800)).await;
                    cx.update_window(handle, |_, w, cx| {
                        app.update(cx, |app, cx| {
                            if index == 1 {
                                app.set_appearance(crate::appearance::Appearance::Dark, w, cx);
                                assert!(component::Theme::global(cx).is_dark(), "dark preview theme applies");
                            }
                            if index == 2 { w.resize(size(px(1000.), px(680.))); }
                            if index == 4 {
                                if let Some(glass) = &app.native_glass {
                                    glass.forced_opaque.set(true);
                                    glass.refresh(app.store.liquid_glass(), w, cx);
                                }
                            }
                            cx.notify();
                        });
                        w.render_frame(cx);
                        if index == 3 {
                            w.click("workspace-menu", cx);
                            assert!(app.read(cx).workspace_menu_open, "workspace menu opens over glass");
                        }
                        if index == 4 {
                            w.press("escape", cx);
                            if let Some(glass) = &app.read(cx).native_glass {
                                assert!(!glass.enabled(), "opaque fallback disables native glass");
                                assert!(glass.pane.isHidden(), "opaque fallback hides the native underlay");
                            }
                        }
                        if let Some(glass) = &app.read(cx).native_glass {
                            if glass.enabled() {
                                let frame = glass.pane.frame();
                                assert!(frame.size.width > 0. && frame.size.height > 0., "glass tracks the window layout");
                            }
                        }
                        if index == 5 {
                            app.update(cx, |app, cx| {
                                if let Some(glass) = &app.native_glass {
                                    glass.forced_opaque.set(false);
                                }
                                app.refresh_liquid_glass(w, cx);
                                app.navigate(crate::app::Scope::Settings, w, cx);
                            });
                            w.render_frame(cx);
                        }
                        if index == 5 || index == 6 {
                            let supported = app.read(cx).liquid_glass_unavailable_reason().is_none();
                            let before = app.read(cx).store.liquid_glass();
                            w.click("settings-liquid-glass", cx);
                            w.render_frame(cx);
                            let expected = if supported { index == 6 } else { before };
                            assert_eq!(app.read(cx).store.liquid_glass(), expected, "settings switch respects availability");
                            if let Some(glass) = &app.read(cx).native_glass {
                                assert_eq!(glass.enabled(), expected && supported, "settings switch applies immediately");
                            }
                            let saved = crate::persistence::Storage::new(directory.join("workspace.sqlite3")).load().unwrap().unwrap();
                            assert_eq!(saved.liquid_glass(), expected, "settings switch persists");
                        }
                    }).unwrap();
                    cx.background_executor().timer(Duration::from_millis(600)).await;
                    if index == 4 {
                        cx.update_window(handle, |_, w, cx| {
                            w.render_frame(cx);
                            assert!(!app.read(cx).workspace_menu_open, "Escape closes the workspace menu");
                        }).unwrap();
                    }

                }
                println!("NATIVE_GLASS settings and interaction checks completed; visual review is manual: {}", directory.display());
                cx.update(|cx| cx.quit());
            }).detach();
        } else {
            // Refresh accessibility changes without changing the system preference.
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(500)).await;
                    if cx.update_window(handle, |_, w, cx| {
                        app.update(cx, |app, cx| {
                            if app.native_glass.as_ref().is_some_and(|glass| glass.refresh(app.store.liquid_glass(), w, cx)) { cx.notify(); }
                        });
                    }).is_err() { break; }
                }
            }).detach();
        }
    });
}
