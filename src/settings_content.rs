//! Retained Settings page content. The child entity is notified by its parent
//! TaskApp only on real state changes, so the logo animation's ancestor dirty
//! marks do not rebuild the static Settings subtree every display frame.

use crate::app::TaskApp;
use gpui_kit::*;

pub(crate) struct SettingsContent {
    parent: WeakEntity<TaskApp>,
    _parent_subscription: Subscription,
}

impl SettingsContent {
    pub(crate) fn new(parent: Entity<TaskApp>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&parent, |_, _, cx| cx.notify());
        Self {
            parent: parent.downgrade(),
            _parent_subscription: subscription,
        }
    }
}

impl Render for SettingsContent {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(parent) = self.parent.upgrade() else {
            return div().into_any_element();
        };
        parent
            .update(cx, |app, cx| app.model_settings(cx))
            .into_any_element()
    }
}

impl TaskApp {
    pub(crate) fn retained_settings(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.settings_content.is_none() {
            let parent = cx.entity();
            self.settings_content = Some(
                cx.new(|cx| crate::settings_content::SettingsContent::new(parent, cx)),
            );
        }
        self.settings_content
            .clone()
            .expect("settings content initialized")
            .cached(StyleRefinement::default().size_full())
            .into_any_element()
    }
}
