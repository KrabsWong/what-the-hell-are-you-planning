//! Native rendering of the refined Assembly mark; GPUI rasterizes SVGs without CSS animation.
use gpui_kit::*;
use std::time::Duration;

// Matches the approved slide-flow-2x preview: each direction has 0.48s
// lead-in, 1.52s of uninterrupted motion, and 0.48s of settled hold.
const CYCLE_SECONDS: f32 = 4.96;
fn motion(phase: f32) -> (f32, f32) {
    let time = phase.clamp(0., 1.) * CYCLE_SECONDS;
    let reverse = time >= 2.48;
    let local = if reverse { time - 2.48 } else { time };
    let t = ((local - 0.48) / 1.52).clamp(0., 1.);
    let turn = t * t * (3. - 2. * t);
    let spread = if t == 0. || t == 1. { 0. } else {
        (std::f32::consts::PI * turn).sin() * 10.
    };
    (if reverse { 1. - turn } else { turn }, spread)
}

pub(crate) struct SidebarLogo;
impl Render for SidebarLogo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        sidebar_logo(crate::appearance::palette(cx).accent)
    }
}

pub(crate) fn sidebar_logo(color: u32) -> AnyElement {
    div()
        .id("sidebar-brand")
        .relative()
        .size(px(32.))
        .flex_shrink_0()
        .with_animation(
            "assembly",
            Animation::new(Duration::from_millis(4960)).repeat(),
            move |mark, phase| {
                mark.child(logo_frame(color, phase))
            },
        )
        .into_any_element()
}

// GPUI snaps quad edges; path MSAA also has too few coverage levels for this
// small logo motion. Integrate rectangle coverage analytically at device-pixel edges.
// Interior + four edge strips + four corners cost at most nine quads per bar.
fn paint_bar(bounds: Bounds<Pixels>, color: u32, window: &mut Window) {
    let scale = window.scale_factor();
    let left = f32::from(bounds.left()) * scale;
    let top = f32::from(bounds.top()) * scale;
    let right = f32::from(bounds.right()) * scale;
    let bottom = f32::from(bounds.bottom()) * scale;
    let xs = [left.floor(), left.ceil(), right.floor(), right.ceil()];
    let ys = [top.floor(), top.ceil(), bottom.floor(), bottom.ceil()];
    for x in xs.windows(2) {
        for y in ys.windows(2) {
            let w = x[1] - x[0]; let h = y[1] - y[0];
            if w <= 0. || h <= 0. { continue; }
            let coverage = ((right.min(x[1]) - left.max(x[0])).max(0.) / w)
                * ((bottom.min(y[1]) - top.max(y[0])).max(0.) / h);
            window.paint_quad(fill(Bounds::new(
                point(px(x[0] / scale), px(y[0] / scale)), size(px(w / scale), px(h / scale)),
            ), rgb(color).alpha(coverage)));
        }
    }
}

fn logo_frame(color: u32, phase: f32) -> impl IntoElement {
    const SCALE: f32 = 32.0 / 128.0;
    // Original bounds, tangential slide, and outward normal. Bars stay horizontal/vertical.
    const BARS: [(f32, f32, f32, f32, f32, f32, f32, f32); 4] = [
        (44., 18., 66., 20., -26., 0., 0., -1.),
        (90., 44., 20., 66., 0., -26., 1., 0.),
        (18., 90., 66., 20., 26., 0., 0., 1.),
        (18., 18., 20., 66., 0., 26., -1., 0.),
    ];
    canvas(|_, _, _| (), move |bounds, _, window, _| {
        let (turn, spread) = motion(phase);
        for (x, y, w, h, dx, dy, nx, ny) in BARS {
            let origin = bounds.origin + point(
                px((x + dx * turn + nx * spread) * SCALE),
                px((y + dy * turn + ny * spread) * SCALE),
            );
            paint_bar(Bounds::new(origin, size(px(w * SCALE), px(h * SCALE))), color, window);
        }
    }).size(px(32.))
}

#[cfg(test)]
mod tests {
    use super::{motion, CYCLE_SECONDS};
    #[test]
    fn approved_timing_holds_both_alignments_and_loops_continuously() {
        for time in [0., 0.24, 0.48, 4.48, 4.72, 4.96] {
            assert_eq!(motion(time / CYCLE_SECONDS), (0., 0.));
        }
        for time in [2.0, 2.24, 2.48, 2.72, 2.96] {
            let (turn, spread) = motion(time / CYCLE_SECONDS);
            assert!((turn - 1.).abs() < 0.00001 && spread.abs() < 0.00001);
        }
        for time in [1.24, 3.72] {
            let (turn, spread) = motion(time / CYCLE_SECONDS);
            assert!((turn - 0.5).abs() < 0.00001 && (spread - 10.).abs() < 0.00001);
        }
        for frame in 0..496 {
            let (turn, spread) = motion(frame as f32 / 496.);
            assert!((0.0..=1.0).contains(&turn));
            assert!((0.0..=10.0).contains(&spread));
        }
    }
}

/// Render real-size consecutive motion samples through the actual renderer.
#[cfg(feature = "qa")]
pub(crate) fn verify_frames() {
    use gpui_kit::test::TestWindowExt;
    struct Sample(f32);
    impl Render for Sample {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().bg(rgb(0xeeeeec)).child(logo_frame(0x795822, self.0))
        }
    }
    let mut cx = HeadlessAppContext::with_platform(platform::current_platform(true).text_system(),
        std::sync::Arc::new(crate::ui_assets::AppAssets), platform::current_headless_renderer);
    let mut sample = None;
    let window = cx.open_window(size(px(32.), px(32.)), |_, cx| {
        let view = cx.new(|_| Sample(0.14)); sample = Some(view.clone()); view
    }).unwrap();
    let handle = window.into();
    let sample = sample.unwrap();
    let root = std::env::current_dir().unwrap().join(".local/brand-motion-qa");
    std::fs::create_dir_all(&root).unwrap();
    for frame in 0..24 {
        cx.update_window(handle, |_, w, cx| {
            sample.update(cx, |s, cx| { s.0 = (0.70 + frame as f32 / 60.) / CYCLE_SECONDS; cx.notify(); });
            w.render_frame(cx);
        }).unwrap();
        cx.capture_screenshot(handle).unwrap().save(root.join(format!("{frame:02}.png"))).unwrap();
    }
    println!("24 consecutive 60 Hz motion samples: {}", root.display());
}
