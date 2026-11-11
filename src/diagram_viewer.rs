//! Native, isolated diagram inspection with SVG rasterization at the current zoom.
use gpui_kit::{component::Theme, prelude::FluentBuilder, *};
use std::sync::Arc;

const TOOLBAR_HEIGHT: f32 = 76.;
const MAX_PIXELS: f32 = 16_000_000.;

pub fn open(light: Arc<str>, dark: Arc<str>, width: f32, height: f32, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1000.), px(720.)), cx);
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(380.), px(300.))),
            titlebar: Some(TitlebarOptions {
                title: Some("Diagram".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| Viewer::new(light, dark, width, height, cx));
            view.update(cx, |this, cx| window.focus(&this.focus, cx));
            cx.new(|cx| base::Root::new(view, window, cx))
        },
    );
}

struct Viewer {
    sources: [Arc<str>; 2],
    parsed: Option<(bool, ParsedSvg)>,
    image: Option<(bool, f32, Arc<RenderImage>)>,
    width: f32,
    height: f32,
    zoom: f32,
    fitting: bool,
    max_zoom: f32,
    pan: Point<Pixels>,
    dragging: Option<Point<Pixels>>,
    focus: FocusHandle,
    error: Option<String>,
}

fn fit_scale(width: f32, height: f32, viewport: Size<Pixels>) -> f32 {
    ((f32::from(viewport.width) - 32.).max(1.) / width)
        .min((f32::from(viewport.height) - 32.).max(1.) / height)
        .min(1.)
}
fn raster_scale(width: f32, height: f32, zoom: f32, device_scale: f32) -> f32 {
    // The renderer multiplies ScaleFactor by two. Bound both dimensions and memory.
    (zoom * device_scale / 2.)
        .min(8192. / (width.max(height) * 2.))
        .min((MAX_PIXELS / (width * height)).sqrt() / 2.)
}
impl Viewer {
    fn new(
        light: Arc<str>,
        dark: Arc<str>,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            sources: [light, dark],
            parsed: None,
            image: None,
            width,
            height,
            zoom: 1.,
            fitting: true,
            max_zoom: 8.,
            pan: point(px(0.), px(0.)),
            dragging: None,
            focus: cx.focus_handle(),
            error: None,
        }
    }
    fn viewport(window: &Window) -> Size<Pixels> {
        let bounds = window.viewport_size();
        size(
            bounds.width,
            (bounds.height - px(TOOLBAR_HEIGHT)).max(px(1.)),
        )
    }
    fn zoom_by(&mut self, factor: f32, cx: &mut Context<Self>) {
        let next = (self.zoom * factor).clamp(0.01, self.max_zoom);
        self.pan = self.pan * (next / self.zoom);
        self.zoom = next;
        self.fitting = false;
        cx.notify();
    }
    fn fit(&mut self, cx: &mut Context<Self>) {
        self.fitting = true;
        self.pan = point(px(0.), px(0.));
        cx.notify();
    }
    fn clamp_pan(&mut self, viewport: Size<Pixels>) {
        let limit_x = (self.width * self.zoom - f32::from(viewport.width)).max(0.) / 2.;
        let limit_y = (self.height * self.zoom - f32::from(viewport.height)).max(0.) / 2.;
        self.pan.x = px(f32::from(self.pan.x).clamp(-limit_x, limit_x));
        self.pan.y = px(f32::from(self.pan.y).clamp(-limit_y, limit_y));
    }
}
impl Render for Viewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = crate::appearance::palette(cx);
        let viewport = Self::viewport(window);
        self.max_zoom = (raster_scale(self.width, self.height, 8., window.scale_factor()) * 2.
            / window.scale_factor())
        .min(8.);
        if self.fitting {
            self.zoom = fit_scale(self.width, self.height, viewport);
        }
        self.zoom = self.zoom.min(self.max_zoom);
        self.clamp_pan(viewport);
        let dark = Theme::global(cx).is_dark();
        let scale = raster_scale(self.width, self.height, self.zoom, window.scale_factor());
        if self
            .image
            .as_ref()
            .is_none_or(|(d, s, _)| *d != dark || (*s - scale).abs() > 0.001)
        {
            let renderer = cx.svg_renderer();
            if self.parsed.as_ref().is_none_or(|(d, _)| *d != dark) {
                match renderer.parse_svg(self.sources[usize::from(dark)].as_bytes()) {
                    Ok(svg) => self.parsed = Some((dark, svg)),
                    Err(e) => self.error = Some(format!("Diagram preview unavailable: {e}")),
                }
            }
            if let Some((_, svg)) = &self.parsed {
                match renderer.render_parsed(svg, scale) {
                    Ok(image) => {
                        self.image = Some((dark, scale, image));
                        self.error = None;
                    }
                    Err(e) => self.error = Some(format!("Diagram preview unavailable: {e}")),
                }
            }
        }
        let width = self.width * self.zoom;
        let height = self.height * self.zoom;
        let image = self.image.as_ref().map(|(_, _, image)| image.clone());
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(p.panel))
            .text_color(rgb(p.text))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, w, cx| {
                match e.keystroke.key.as_str() {
                    "escape" => w.remove_window(),
                    "+" | "=" => this.zoom_by(1.25, cx),
                    "-" => this.zoom_by(0.8, cx),
                    "0" => this.fit(cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(
                div()
                    .h(px(TOOLBAR_HEIGHT))
                    .flex_shrink_0()
                    .px_3()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                crate::controls::quiet_button("zoom-out", "Zoom out")
                                    .on_click(cx.listener(|this, _, _, cx| this.zoom_by(0.8, cx))),
                            )
                            .child(
                                crate::controls::quiet_button("zoom-in", "Zoom in")
                                    .on_click(cx.listener(|this, _, _, cx| this.zoom_by(1.25, cx))),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .w(px(48.))
                                    .child(format!("{:.0}%", self.zoom * 100.)),
                            )
                            .child(
                                crate::controls::quiet_button("fit", "Fit")
                                    .tooltip("Fit diagram to window (0)")
                                    .on_click(cx.listener(|this, _, _, cx| this.fit(cx))),
                            )
                            .child(
                                crate::controls::quiet_button("close", "Close")
                                    .on_click(|_, w, _| w.remove_window()),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(p.muted))
                            .child("Drag to pan · + / - to zoom · 0 to fit · Esc to close"),
                    ),
            )
            .child(
                div()
                    .id("diagram-canvas")
                    .relative()
                    .w_full()
                    .h(viewport.height)
                    .overflow_hidden()
                    .cursor(CursorStyle::OpenHand)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e: &MouseDownEvent, w, cx| {
                            this.dragging = Some(e.position);
                            w.focus(&this.focus, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                        if let Some(previous) = this.dragging
                            && e.dragging()
                        {
                            this.pan += e.position - previous;
                            this.dragging = Some(e.position);
                            this.fitting = false;
                            cx.notify();
                        } else {
                            this.dragging = None;
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.dragging = None),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.dragging = None),
                    )
                    .when_some(image, |el, image| {
                        el.child(
                            img(image)
                                .absolute()
                                .left(px((f32::from(viewport.width) - width) / 2.) + self.pan.x)
                                .top(px((f32::from(viewport.height) - height) / 2.) + self.pan.y)
                                .w(px(width))
                                .h(px(height)),
                        )
                    })
                    .when_some(self.error.clone(), |el, error| {
                        el.child(div().p_4().child(error))
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{fit_scale, raster_scale, MAX_PIXELS};
    use gpui_kit::{px, size};
    #[test]
    fn fitting_preserves_ratio_and_does_not_enlarge_small_diagrams() {
        assert_eq!(fit_scale(2000., 500., size(px(532.), px(432.))), 0.25);
        assert_eq!(fit_scale(100., 100., size(px(532.), px(432.))), 1.);
    }
    #[test]
    fn rasterization_uses_display_resolution_and_bounds_memory() {
        assert_eq!(raster_scale(1000., 500., 2., 2.), 2.);
        let scale = raster_scale(8000., 2000., 8., 2.) * 2.;
        assert!(8000. * scale <= 8192.);
        assert!(8000. * 2000. * scale * scale <= MAX_PIXELS);
    }
}

#[cfg(feature = "qa")]
pub fn verify() {
    use gpui_kit::test::{TestEventExt, TestWindowExt};
    std::fs::create_dir_all(".local/diagram-viewer-qa").unwrap();
    let (light, dark) = crate::diagram_renderer::render_pair(include_str!(
        "../experiments/rust-mermaid/fixtures/gates.mmd"
    ))
    .unwrap();
    let tree = usvg::Tree::from_str(&light, &usvg::Options::default()).unwrap();
    let dimensions = tree.size();
    for (name, appearance, width) in [
        ("light", crate::appearance::Appearance::Light, 1000.),
        ("dark", crate::appearance::Appearance::Dark, 1000.),
        ("narrow", crate::appearance::Appearance::Light, 420.),
    ] {
        let mut cx = HeadlessAppContext::with_platform(
            platform::current_platform(true).text_system(),
            Arc::new(crate::ui_assets::AppAssets),
            platform::current_headless_renderer,
        );
        cx.update(|cx| {
            init(cx);
            crate::appearance::apply(appearance, None, cx);
        });
        let mut viewer = None;
        let handle: AnyWindowHandle = cx
            .open_window(size(px(width), px(700.)), |w, cx| {
                let view = cx.new(|cx| {
                    Viewer::new(
                        Arc::from(light.as_str()),
                        Arc::from(dark.as_str()),
                        dimensions.width(),
                        dimensions.height(),
                        cx,
                    )
                });
                view.update(cx, |v, cx| w.focus(&v.focus, cx));
                viewer = Some(view.clone());
                cx.new(|cx| base::Root::new(view, w, cx))
            })
            .unwrap()
            .into();
        let viewer = viewer.unwrap();
        for _ in 0..3 {
            cx.update_window(handle, |_, w, cx| w.render_frame(cx))
                .unwrap();
            cx.run_until_parked();
        }
        cx.capture_screenshot(handle)
            .unwrap()
            .save(format!(".local/diagram-viewer-qa/{name}-fit.png"))
            .unwrap();
        cx.update_window(handle, |_, w, cx| {
            let initial = viewer.read(cx).zoom;
            let initial_pixels = viewer.read(cx).image.as_ref().unwrap().2.size(0);
            for _ in 0..5 {
                w.key_down("=", false, cx);
            }
            assert!(viewer.read(cx).zoom > initial);
            assert!(viewer.read(cx).image.as_ref().unwrap().2.size(0).width > initial_pixels.width);
            let start = point(px(width / 2.), px(300.));
            w.pointer_down(start, MouseButton::Left, cx);
            w.pointer_move(start + point(px(80.), px(20.)), Some(MouseButton::Left), cx);
            w.pointer_up(start + point(px(80.), px(20.)), MouseButton::Left, cx);
            assert!(viewer.read(cx).pan.x > px(0.));
        })
        .unwrap();
        cx.capture_screenshot(handle)
            .unwrap()
            .save(format!(".local/diagram-viewer-qa/{name}-zoom-pan.png"))
            .unwrap();
        cx.update_window(handle, |_, w, cx| {
            w.key_down("0", false, cx);
            assert!(viewer.read(cx).fitting);
            assert_eq!(viewer.read(cx).pan, point(px(0.), px(0.)));
        })
        .unwrap();
        cx.update(|cx| cx.quit());
    }
    println!(
        "Diagram viewer zoom, drag and fit checks passed; frames saved in .local/diagram-viewer-qa"
    );
}
