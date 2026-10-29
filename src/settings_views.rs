use crate::{
    app::{Scope, TaskApp},
    appearance::palette,
    research::ResearchKind,
    views::*,
};
use gpui_kit::{
    component::{Disableable, checkbox::Checkbox, input::Input},
    prelude::FluentBuilder,
    *,
};

impl TaskApp {
    pub fn refresh_global_model_input(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let model = self.store.default_model.clone();
        self.global_model
            .update(cx, |s, cx| s.set_value(model, w, cx));
        self.global_model_filter
            .update(cx, |s, cx| s.set_value("", w, cx));
        self.show_global_models = false;
        self.use_global_for_all = false;
    }

    pub fn save_global_model(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let model = self.global_model.read(cx).value().trim().to_string();
        if !crate::opencode::valid_model(&model) {
            self.show_save_error(
                "Choose a model with its provider prefix, for example provider/model.",
                cx,
            );
            return;
        }
        self.snapshot();
        self.store.default_model = model.clone();
        if self.use_global_for_all {
            self.store.research_config.models = Default::default();
            for policy in &mut self.store.policies {
                if policy.agent == "OpenCode" {
                    policy.model.clear();
                }
            }
            for input in &self.research_models {
                input.update(cx, |s, cx| s.set_value("", w, cx));
            }
            if self.policy_agent == "OpenCode" {
                self.policy_model.update(cx, |s, cx| s.set_value("", w, cx));
            }
        }
        self.use_global_for_all = false;
        self.global_model
            .update(cx, |s, cx| s.set_value(model, w, cx));
        self.confirm_save(
            "Global model saved to local workspace · applies to future runs",
            cx,
        );
    }

    pub fn model_source_label(&self, stage_override: &str) -> String {
        let model = self.store.effective_model(stage_override);
        if model.is_empty() {
            "No model selected · choose a global default".into()
        } else {
            format!(
                "{}: {model}",
                if stage_override.is_empty() {
                    "Global default"
                } else {
                    "Stage override"
                }
            )
        }
    }

    pub fn model_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let mut overrides = vec![];
        for kind in ResearchKind::ALL {
            let model = &self.store.research_config.models[kind.index()];
            if !model.is_empty() {
                overrides.push((format!("Research · {}", kind.label()), model.clone()));
            }
        }
        for policy in &self.store.policies {
            if policy.agent == "OpenCode" && !policy.model.is_empty() {
                overrides.push((
                    format!("Task stage · {}", policy.stage.label()),
                    policy.model.clone(),
                ));
            }
        }
        let query = self.global_model_filter.read(cx).value().to_lowercase();
        let matches: Vec<_> = self
            .available_models
            .iter()
            .filter(|m| m.to_lowercase().contains(&query))
            .collect();
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
        let mut content = col().w_full().max_w(px(840.)).gap_5()
            .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child("Settings"))
            .when_some(self.workspace_root.as_ref(), |view, root| view.child(col().gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Workspace"))
                .child(label("Projects, reports, materials and app data are stored in this folder's workspace subfolder.", cx))
                .child(div().text_sm().child(root.to_string_lossy().into_owned()))))
            .child(appearance)
            .child(col().gap_2()
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Model settings"))
                .child(label("Choose one default for project planning, research and OpenCode tasks. A stage can override it when needed.", cx)))
            .child(col().gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Local OpenCode"))
                .child(label("Connect to your local OpenCode installation to load available models.", cx))
                .child(row().flex_wrap()
                    .child(button("global-refresh-models", "Refresh local models")
                        .loading(self.discovering_models())
                        .on_click(cx.listener(|this, _, _, cx| this.discover_models(cx))))
                    .child(div().text_sm().text_color(rgb(colors.text)).child(self.connection_status.clone())))
                .child(row().flex_wrap()
                    .child(label("Executable path", cx))
                    .child(div().text_sm().text_color(rgb(colors.text)).child(self.store.opencode_path.clone()))))
            .child(col().gap_3()
                .child(row().flex_wrap()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Global default model"))
                    .when(self.global_model.read(cx).value().trim() != self.store.default_model || self.use_global_for_all, |d| d.child(label("Unsaved changes", cx).text_color(rgb(colors.accent)))))
                .child(label("Keep the provider prefix, for example tencent-token-plan/. Changes apply to future runs; running tasks keep their current model.", cx))
                .child(Input::new(&self.global_model).id("global-model-input")
                    .suffix(quiet_button("choose-global-model", if self.show_global_models { "Hide models" } else { "Choose model" })
                        .h_7().px_2()
                        .tooltip("Choose from local OpenCode models")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_global_models = !this.show_global_models;
                            if this.show_global_models && this.available_models.is_empty() { this.discover_models(cx); }
                            cx.notify();
                        })))));
        if self.show_global_models {
            content = content.child(col().gap_2()
                .child(Input::new(&self.global_model_filter).id("global-model-search"))
                .child(div().id("global-model-scroll-boundary")
                    .on_scroll(|_, _, cx| cx.stop_propagation())
                    .child(col().id("global-model-list").h(px(216.)).overflow_y_scroll().gap_1()
                    .when(matches.is_empty(), |d| d.child(label(
                        if self.discovering_models() { "Loading models from local OpenCode…" }
                        else if self.available_models.is_empty() { "No models loaded. Refresh local models, or enter the full model identifier above." }
                        else { "No matching model. Try a different provider or model name." }, cx)))
                    .children(matches.into_iter().map(|model| {
                        let model = model.clone();
                        let selected = self.global_model.read(cx).value().as_ref() == model;
                        quiet_button(format!("global-model-option-{model}"), "")
                            .w_full().h_8().px_2().tooltip(model.clone())
                            .accessibility_label(model.clone())
                            .child(row().w_full().min_w_0().gap_2()
                                .child(div().w_4().flex_shrink_0().text_color(rgb(colors.accent))
                                    .child(if selected { "✓" } else { "" }))
                                .child(single_line_text(format!("global-model-name-{model}"), model.clone())))
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.global_model.update(cx, |s, cx| s.set_value(model.clone(), w, cx));
                                this.show_global_models = false;
                                cx.notify();
                            }))
                    })).map(observed))));
        }
        content = content
            .child(col().gap_3()
                .child(col().gap_2()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Stage defaults"))
                    .child(label(if self.store.all_stages_use_global_model() {
                        "All stages follow the global model. Set individual overrides below when needed. Manual stages stay manual."
                    } else { "Apply the global model to every stage and clear existing overrides when saving. Manual stages stay manual." }, cx)))
                .child(Checkbox::new("global-model-all-stages")
                    .label("Use this model for every stage")
                    .checked(self.use_global_for_all || self.store.all_stages_use_global_model())
                    .disabled(self.store.all_stages_use_global_model())
                    .on_change(cx.listener(|this, checked, _, cx| { this.use_global_for_all = *checked; cx.notify(); })))
                .child(row().flex_wrap()
                    .child(primary_button("save-global-model", "Save global model")
                        .on_click(cx.listener(|this, _, w, cx| this.save_global_model(w, cx))))
                    .child(button("reset-global-model", "Reset changes")
                        .on_click(cx.listener(|this, _, w, cx| { this.refresh_global_model_input(w, cx); this.error.clear(); this.save_feedback = None; cx.notify(); }))))
                .child(col().gap_1().pl_3().py_2().border_l_2().border_color(rgb(colors.accent))
                    .child(label("Saved model · used for future runs", cx))
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(rgb(colors.text))
                        .child(if self.store.default_model.is_empty() { "Not configured".to_string() } else { self.store.default_model.clone() })))
                .when(!self.error.is_empty(), |d| d.child(div().text_sm().text_color(rgb(colors.danger)).child(self.error.clone())))
                )
            .child(col().gap_3().pt_4().border_t_1().border_color(rgb(colors.line))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(format!("Stage overrides · {}", overrides.len())))
                .child(label("Choose a different model for a specific research or task stage.", cx))
                .when(overrides.is_empty(), |d| d.child(div().text_sm().text_color(rgb(colors.text)).child("All OpenCode stages use the global default.")))
                .children(overrides.into_iter().map(|(name, model)| col().gap_1().py_2().border_b_1().border_color(rgb(colors.line))
                    .child(label(name, cx))
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(rgb(colors.text)).child(model))))
                .child(row().flex_wrap()
                    .child(button("global-research-overrides", "Research stage overrides →").on_click(cx.listener(|this, _, w, cx| { this.navigate(Scope::Research, w, cx); this.show_research_settings = true; })))
                    .child(button("global-task-overrides", "Task stage overrides →").on_click(cx.listener(|this, _, w, cx| this.navigate(Scope::Workflow, w, cx))))));
        col()
            .id("model-settings-scroll")
            .size_full()
            .overflow_y_scroll()
            .p_6()
            .child(content)
            .map(observed)
    }
}
