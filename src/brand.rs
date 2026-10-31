//! Native rendering of the refined Assembly mark; GPUI rasterizes SVGs without CSS animation.
use gpui_kit::*;
use std::time::Duration;

// Same cubic-bezier(.4, 0, .2, 1) as assets/brand/planning.svg.
fn ease(progress: f32) -> f32 {
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..16 {
        let t = (low + high) * 0.5;
        let x = 3.0 * (1.0 - t) * (1.0 - t) * t * 0.4 + 3.0 * (1.0 - t) * t * t * 0.2 + t * t * t;
        if x < progress {
            low = t;
        } else {
            high = t;
        }
    }
    let t = (low + high) * 0.5;
    3.0 * (1.0 - t) * t * t + t * t * t
}

fn excursion(phase: f32, index: usize) -> f32 {
    let t = phase * 100.0 - (12.0 + index as f32 * 4.0);
    if t <= 0.0 || t >= 32.0 {
        0.0
    } else if t < 10.0 {
        ease(t / 10.0)
    } else if t <= 18.0 {
        1.0
    } else {
        1.0 - ease((t - 18.0) / 14.0)
    }
}

pub(crate) fn sidebar_logo(color: u32) -> AnyElement {
    const SCALE: f32 = 32.0 / 128.0;
    // Rotations of M44 18H110V38H44Z around (64, 64).
    const BARS: [(f32, f32, f32, f32, f32, f32); 4] = [
        (44., 18., 66., 20., -3., -3.),
        (90., 44., 20., 66., 3., -3.),
        (18., 90., 66., 20., 3., 3.),
        (18., 18., 20., 66., -3., 3.),
    ];
    div()
        .id("sidebar-brand")
        .relative()
        .size(px(32.))
        .flex_shrink_0()
        .with_animation(
            "assembly",
            Animation::new(Duration::from_secs(9))
                .repeat()
                .with_max_fps(30.),
            move |mark, phase| {
                mark.children(
                    BARS.into_iter()
                        .enumerate()
                        .map(|(i, (x, y, w, h, dx, dy))| {
                            let amount = excursion(phase, i);
                            div()
                                .absolute()
                                .left(px((x + dx * amount) * SCALE))
                                .top(px((y + dy * amount) * SCALE))
                                .w(px(w * SCALE))
                                .h(px(h * SCALE))
                                .bg(rgb(color))
                        }),
                )
            },
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::excursion;
    #[test]
    fn assembly_returns_home_and_stays_within_its_envelope() {
        for i in 0..4 {
            assert_eq!(excursion(0., i), 0.);
            assert_eq!(excursion(1., i), 0.);
            assert_eq!(excursion((24. + i as f32 * 4.) / 100., i), 1.);
            for frame in 0..=900 {
                assert!((0.0..=1.0).contains(&excursion(frame as f32 / 900., i)));
            }
        }
    }
}
