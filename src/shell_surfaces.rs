//! Retained shell surfaces. Each child entity renders one visible boundary and
//! is notified only when its parent TaskApp is notified by a real state change.
//! Animation frames mark ancestor views dirty but do not notify these entities,
//! so the animated logo no longer forces the content and footer to rebuild at
//! display cadence. Invalidation stays semantic: every parent notification
//! re-renders the surface, so live model/runtime/publishing/error state and the
//! whole-Store saved comparison are never cached independently.

use crate::app::TaskApp;
use crate::views::{col, observed};
use gpui_kit::*;

pub(crate) struct ContentSurface {
    parent: WeakEntity<TaskApp>,
    _parent_subscription: Subscription,
}

impl ContentSurface {
    pub(crate) fn new(parent: Entity<TaskApp>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&parent, |_, _, cx| cx.notify());
        Self {
            parent: parent.downgrade(),
            _parent_subscription: subscription,
        }
    }
}

impl Render for ContentSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(parent) = self.parent.upgrade() else {
            return div().into_any_element();
        };
        parent
            .update(cx, |app, cx| app.main_content(cx))
            .into_any_element()
    }
}

pub(crate) struct SidebarSurface {
    parent: WeakEntity<TaskApp>,
    _parent_subscription: Subscription,
}

impl SidebarSurface {
    pub(crate) fn new(parent: Entity<TaskApp>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&parent, |_, _, cx| cx.notify());
        Self {
            parent: parent.downgrade(),
            _parent_subscription: subscription,
        }
    }
}

impl Render for SidebarSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(parent) = self.parent.upgrade() else {
            return div().into_any_element();
        };
        parent
            .update(cx, |app, cx| app.sidebar_surface(cx))
            .into_any_element()
    }
}

pub(crate) struct BrandOverlay {
    parent: WeakEntity<TaskApp>,
    started: std::time::Instant,
    wake_scheduled: bool,
}

impl BrandOverlay {
    pub(crate) fn new(parent: Entity<TaskApp>, cx: &mut Context<Self>) -> Self {
        let _ = cx;
        Self {
            parent: parent.downgrade(),
            started: std::time::Instant::now(),
            wake_scheduled: false,
        }
    }
}

impl Render for BrandOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "qa")]
        crate::brand::note_logo_render();
        let Some(parent) = self.parent.upgrade() else {
            return div().into_any_element();
        };
        let (offset, accent) = parent.read_with(cx, |app, cx| {
            (app.sidebar_scroll.offset().y, crate::appearance::palette(cx).accent)
        });
        let elapsed = self.started.elapsed().as_secs_f32();
        #[cfg(feature = "qa")]
        let static_diagnostic =
            std::env::var("PLANNING_T19_DIAGNOSTIC_STATIC_LOGO").as_deref() == Ok("1");
        #[cfg(not(feature = "qa"))]
        let static_diagnostic = false;
        let pinned = static_diagnostic || cx.reduce_motion();
        let phase = crate::brand::brand_render_phase(elapsed, pinned);
        let static_frame = pinned || crate::brand::brand_frame_is_static(elapsed);
        if !static_frame {
            window.request_animation_frame();
        } else if !pinned && !self.wake_scheduled {
            // Wake exactly when the next moving segment starts; do not repaint
            // the window during the visually static lead-in/hold.
            self.wake_scheduled = true;
            let wait = crate::brand::brand_next_motion_wait(elapsed);
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs_f32(wait.max(0.001)))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.wake_scheduled = false;
                    cx.notify();
                });
            })
            .detach();
        }
        observed(
            div()
                .id("sidebar-brand-overlay")
                .absolute()
                .top(px(14.) + offset)
                .left(px(8.))
                .size(px(32.))
                .child(crate::brand::sidebar_logo_at(accent, phase)),
        )
        .into_any_element()
    }
}

pub(crate) struct FooterBar {
    parent: WeakEntity<TaskApp>,
    _parent_subscription: Subscription,
}

impl FooterBar {
    pub(crate) fn new(parent: Entity<TaskApp>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&parent, |_, _, cx| cx.notify());
        Self {
            parent: parent.downgrade(),
            _parent_subscription: subscription,
        }
    }
}

impl Render for FooterBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(parent) = self.parent.upgrade() else {
            return div().into_any_element();
        };
        parent
            .update(cx, |app, cx| app.footer_bar(cx))
            .into_any_element()
    }
}

impl TaskApp {
    /// The navigation sidebar is retained as a sibling of the animated brand
    /// mark. The mark is painted as an overlay that follows the sidebar scroll
    /// offset; animation therefore dirties only the cheap overlay branch, not
    /// the navigation tree. Semantic changes still notify the parent and
    /// rebuild the cached sidebar.
    pub(crate) fn retained_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.sidebar_surface.is_none() {
            let parent = cx.entity();
            self.sidebar_surface = Some(
                cx.new(|cx| crate::shell_surfaces::SidebarSurface::new(parent, cx)),
            );
        }
        if self.brand_overlay.is_none() {
            let parent = cx.entity();
            self.brand_overlay = Some(
                cx.new(|cx| crate::shell_surfaces::BrandOverlay::new(parent, cx)),
            );
        }
        col()
            .relative()
            .size_full()
            .child(
                self.sidebar_surface
                    .clone()
                    .expect("sidebar surface initialized")
                    .cached(StyleRefinement::default().size_full()),
            )
            .child(observed(
                div()
                    .id("sidebar-logo-clip")
                    .absolute()
                    .top(px(if cfg!(target_os = "macos") { 44. } else { 12. }))
                    .left(px(12.))
                    .right(px(12.))
                    .bottom(px(12.))
                    .overflow_hidden()
                    .child(
                        self.brand_overlay
                            .clone()
                            .expect("brand overlay initialized"),
                    ),
            ))
            .into_any_element()
    }

    pub(crate) fn retained_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.content_surface.is_none() {
            let parent = cx.entity();
            self.content_surface = Some(
                cx.new(|cx| crate::shell_surfaces::ContentSurface::new(parent, cx)),
            );
        }
        self.content_surface
            .clone()
            .expect("content surface initialized")
            .cached(StyleRefinement::default().size_full())
            .into_any_element()
    }

    /// The footer has a definite 30px height, so it can be retained without
    /// measuring its contents. Buttons, focus and hitboxes are replayed from
    /// the cached subtree; a real parent notification rebuilds them.
    pub(crate) fn retained_footer(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.footer_bar.is_none() {
            let parent = cx.entity();
            self.footer_bar =
                Some(cx.new(|cx| crate::shell_surfaces::FooterBar::new(parent, cx)));
        }
        self.footer_bar
            .clone()
            .expect("footer bar initialized")
            .cached(StyleRefinement::default().w_full().h(px(30.)))
            .into_any_element()
    }
}
