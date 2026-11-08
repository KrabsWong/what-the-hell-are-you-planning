//! Retained shell surfaces. Each child entity renders one visible boundary and
//! is notified only when its parent TaskApp is notified by a real state change.
//! Animation frames mark ancestor views dirty but do not notify these entities,
//! so the animated logo no longer forces the content and footer to rebuild at
//! display cadence. Invalidation stays semantic: every parent notification
//! re-renders the surface, so live model/runtime/publishing/error state and the
//! whole-Store saved comparison are never cached independently.

use crate::app::TaskApp;
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
