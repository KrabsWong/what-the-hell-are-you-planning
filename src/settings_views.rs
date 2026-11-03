use crate::{
    app::{Scope, TaskApp},
    appearance::palette,
    views::*,
};
use gpui_kit::{
    component::{
        Disableable,
        tab::{Tab, TabBar},
    },
    prelude::FluentBuilder,
    *,
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SettingsTab {
    #[default]
    General,
    Models,
}

impl SettingsTab {
    const ALL: [Self; 2] = [Self::General, Self::Models];
}

impl TaskApp {
    pub fn set_execution_concurrency(&mut self, limit: usize, cx: &mut Context<Self>) {
        let limit = limit.clamp(1, crate::scheduler::MAX_CONCURRENT_RUNS);
        if self.store.max_concurrent_runs == limit { return; }
        self.snapshot();
        self.store.max_concurrent_runs = limit;
        self.confirm_save("Execution limit saved · running tasks will finish normally", cx);
        cx.notify();
    }

    pub fn open_model_settings(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.settings_tab = SettingsTab::Models;
        self.settings_scroll.set_offset(point(px(0.), px(0.)));
        self.navigate(Scope::Settings, w, cx);
    }

    pub fn refresh_global_model_input(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let model = self.store.default_model.clone();
        self.global_model
            .update(cx, |s, cx| s.set_value(model, w, cx));
    }

    pub fn agent_settings_panel(&self, cx: &mut Context<Self>) -> Div {
        let colors = palette(cx);
        let mut agents = col().gap_3();
        for agent in ["OpenCode", "Codex"] {
            let codex = agent == "Codex";
            let path = self.store.agent_path(agent);
            let version = if codex {
                self.codex_version.as_deref().unwrap_or("Not verified")
            } else {
                self.opencode_version.as_ref()
                    .filter(|(checked_path, version)| checked_path == path && !version.is_empty())
                    .map(|(_, version)| version.as_str()).unwrap_or("Not verified")
            };
            let status = if codex { &self.codex_status } else { &self.connection_status };
            let loading = if codex { self.discovering_codex_models() } else { self.discovering_models() };
            agents = agents.child(col().w_full().p_4().gap_2().rounded_md()
                .border_1().border_color(rgb(colors.line)).bg(rgb(colors.panel))
                .child(row().justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(agent))
                    .child(label(format!("Version · {version}"), cx)))
                .child(row().flex_wrap()
                    .child(label("Executable", cx))
                    .child(div().text_sm().child(if path.is_empty() { "Not configured".to_owned() } else { path.to_owned() })))
                .child(row().flex_wrap().justify_between().pt_2()
                    .border_t_1().border_color(rgb(colors.line))
                    .child(label(status.clone(), cx))
                    .child(button(if codex { "codex-refresh-models" } else { "research-refresh-models" }, "Refresh local models")
                        .loading(loading)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if codex { this.discover_codex_models(cx); }
                            else {
                                this.connection_project = this.research_project;
                                this.discover_models(cx);
                            }
                        })))));
        }
        agents
    }

    pub fn model_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let appearance = col()
            .gap_2()
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Appearance"))
            .child(label("Choose a theme. Follow system adapts automatically; changes are saved immediately.", cx))
            .child(
                row().flex_wrap().children(
                    [
                        (
                            "appearance-system",
                            "Follow system",
                            crate::appearance::Appearance::System,
                        ),
                        (
                            "appearance-light",
                            "Light",
                            crate::appearance::Appearance::Light,
                        ),
                        (
                            "appearance-dark",
                            "Dark",
                            crate::appearance::Appearance::Dark,
                        ),
                    ]
                    .into_iter()
                    .map(|(id, name, mode)| {
                        select_button(
                            id,
                            if self.store.appearance == mode {
                                format!("✓ {name}")
                            } else {
                                name.into()
                            },
                            self.store.appearance == mode,
                        )
                        .on_click(cx.listener(move |a, _, w, cx| a.set_appearance(mode, w, cx)))
                    }),
                ),
            );
        let general = col().gap_6()
            .child(appearance)
            .child(col().gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Execution"))
                .child(div().text_sm().child("Maximum simultaneous tasks"))
                .child(row()
                    .child(button("concurrency-decrease", "−")
                        .accessibility_label("Decrease maximum simultaneous tasks")
                        .disabled(self.store.concurrency_limit() <= 1)
                        .on_click(cx.listener(|a, _, _, cx| a.set_execution_concurrency(a.store.concurrency_limit().saturating_sub(1), cx))))
                    .child(div().w_8().text_center().child(self.store.concurrency_limit().to_string()))
                    .child(button("concurrency-increase", "+")
                        .accessibility_label("Increase maximum simultaneous tasks")
                        .disabled(self.store.concurrency_limit() >= crate::scheduler::MAX_CONCURRENT_RUNS)
                        .on_click(cx.listener(|a, _, _, cx| a.set_execution_concurrency(a.store.concurrency_limit() + 1, cx)))))
                .child(label("1–10 tasks. Recommended: 3; try 5 after checking stability. Higher values may cause more rate-limit errors, retries and browser load.", cx))
                .child(label("Saved immediately to this workspace. Lowering the limit lets running tasks finish. Dependencies, shared folders and browser Spaces can reduce actual concurrency.", cx)))
            .when_some(self.workspace_root.as_ref(), |view, root| view.child(col().gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Workspace"))
                .child(label("Projects, reports, materials and app data are stored in this folder's workspace subfolder.", cx))
                .child(div().text_sm().child(root.to_string_lossy().into_owned()))));
        let panel = match self.settings_tab {
            SettingsTab::General => general,
            SettingsTab::Models => self.research_settings_panel(cx),
        };
        col()
            .size_full()
            .child(
                col()
                    .flex_shrink_0()
                    .px_6()
                    .pt_6()
                    .gap_4()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Settings"),
                    )
                    .child(
                        TabBar::new("settings-tabs")
                            .underline()
                            .selected_index(Self::settings_tab_index(self.settings_tab))
                            .child(Tab::new().label("General"))
                            .child(Tab::new().label("Agent & models"))
                            .on_click(cx.listener(|this, index, _, cx| {
                                this.research_model_picker = None;
                                this.settings_tab = SettingsTab::ALL[*index];
                                this.settings_scroll.set_offset(point(px(0.), px(0.)));
                                cx.notify();
                            })),
                    ),
            )
            .child(
                col()
                    .id("model-settings-scroll")
                    .track_scroll(&self.settings_scroll)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_6()
                    .child(panel.max_w(px(840.)))
                    .map(observed),
            )
            .into_any_element()
    }

    fn settings_tab_index(tab: SettingsTab) -> usize {
        SettingsTab::ALL
            .iter()
            .position(|item| *item == tab)
            .unwrap()
    }
}
