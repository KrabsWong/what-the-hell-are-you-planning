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
}

impl BrandOverlay {
    pub(crate) fn new(parent: Entity<TaskApp>, cx: &mut Context<Self>) -> Self {
        let _ = cx;
        Self {
            parent: parent.downgrade(),
        }
    }
}

impl Render for BrandOverlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "qa")]
        crate::brand::note_logo_render();
        let Some(parent) = self.parent.upgrade() else {
            return div().into_any_element();
        };
        let (offset, accent) = parent.read_with(cx, |app, cx| {
            (app.sidebar_scroll.offset().y, crate::appearance::palette(cx).accent)
        });
        // The sidebar reserves a 32px macOS title bar before the scroll area;
        // the mark is centered in the 60px identity row below it.
        let brand_top = if cfg!(target_os = "macos") { 46. } else { 14. };
        observed(
            div()
                .id("sidebar-brand-overlay")
                .absolute()
                .top(px(brand_top) + offset)
                .left(px(8.))
                .size(px(32.))
                .child(crate::brand::sidebar_logo(accent)),
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
                    .top(px(12.))
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
