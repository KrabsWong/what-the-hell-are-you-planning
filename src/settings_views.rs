use crate::{
    app::{Scope, TaskApp},
    appearance::palette,
    views::*,
};
use gpui_kit::{
    component::{
        Disableable,
        button::ButtonVariants,
        input::Input,
        menu::{DropdownMenu, PopupMenuItem},
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
    Publishing,
}

impl SettingsTab {
    const ALL: [Self; 3] = [Self::General, Self::Models, Self::Publishing];
}

impl TaskApp {
    pub fn model_profiles_panel(&self, cx: &mut Context<Self>) -> Div {
        let active = self
            .store
            .active_model_profile()
            .as_ref()
            .and_then(|name| self.store.model_profiles().iter().find(|p| &p.name == name));
        let changed = active.is_some_and(|p| self.store.model_profile_changed(p));
        let selected = self.store.active_model_profile().clone();
        let names: Vec<_> = self
            .store
            .model_profiles()
            .iter()
            .map(|p| p.name.clone())
            .collect();
        let entity = cx.entity().downgrade();
        let picker = button(
            "model-profile-picker",
            selected.clone().unwrap_or("Custom settings".into()),
        )
        .w(px(240.))
        .dropdown_caret(true)
        .disabled(names.is_empty())
        .tooltip("Apply a saved global and stage model configuration")
        .dropdown_menu(move |mut menu, _, _| {
            for name in &names {
                let entity = entity.clone();
                let name = name.clone();
                menu = menu.item(
                    PopupMenuItem::new(name.clone())
                        .checked(selected.as_ref() == Some(&name))
                        .on_click(move |_, w, cx| {
                            let _ = entity.update(cx, |this, cx| {
                                let name = name.clone();
                                if this
                                    .apply_domain_command_undoable(
                                        crate::scoped_domain::DomainCommand::ApplyModelProfile {
                                            name,
                                        },
                                        "Model profile applied",
                                        cx,
                                    )
                                    .is_none()
                                {
                                    return;
                                }
                                this.creating_model_profile = false;
                                this.research_model_picker = None;
                                this.refresh_global_model_input(w, cx);
                                this.refresh_research_inputs(w, cx);
                                this.save_feedback = None;
                            });
                        }),
                );
            }
            menu
        });
        let entity = cx.entity().downgrade();
        let active_name = active.map(|p| p.name.clone());
        let delete_name = active_name.clone();
        let management = quiet_button("model-profile-menu", "Manage")
            .dropdown_caret(true)
            .disabled(active.is_none())
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                let Some(name) = delete_name.clone() else {
                    return menu;
                };
                let restore_entity = entity.clone();
                let restore_name = name.clone();
                let delete_entity = entity.clone();
                menu.item(
                    PopupMenuItem::new("Restore saved settings")
                        .disabled(!changed)
                        .on_click(move |_, w, cx| {
                            let _ = restore_entity.update(cx, |this, cx| {
                                let name = restore_name.clone();
                                if this
                                    .apply_domain_command_undoable(
                                        crate::scoped_domain::DomainCommand::ApplyModelProfile {
                                            name,
                                        },
                                        "Saved profile settings restored",
                                        cx,
                                    )
                                    .is_none()
                                {
                                    return;
                                }
                                this.research_model_picker = None;
                                this.refresh_global_model_input(w, cx);
                                this.refresh_research_inputs(w, cx);
                                this.save_feedback = None;
                            });
                        }),
                )
                .separator()
                .item(
                    PopupMenuItem::new("Delete current profile").on_click(move |_, _, cx| {
                        let _ = delete_entity.update(cx, |this, cx| {
                            let name = name.clone();
                            let saved = this
                                .apply_domain_command_undoable(
                                    crate::scoped_domain::DomainCommand::DeleteModelProfile { name },
                                    "Profile deleted · current settings kept · Undo to restore",
                                    cx,
                                )
                                .is_some();
                            if saved {
                                this.save_feedback = None;
                            }
                        });
                    }),
                )
            });
        let toolbar =
            row()
                .w_full()
                .justify_between()
                .flex_wrap()
                .child(
                    row()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(selectable("Profile")),
                        )
                        .child(picker),
                )
                .child(
                    row()
                        .child(quiet_button("model-profile-new", "New profile").on_click(
                            cx.listener(|this, _, w, cx| {
                                this.creating_model_profile = true;
                                this.model_profile_name.update(cx, |s, cx| {
                                    s.set_value("", w, cx);
                                    s.focus(w, cx);
                                });
                                cx.notify();
                            }),
                        ))
                        .child(management)
                        .child(
                            button("model-profile-update", "Save profile")
                                .w(px(110.))
                                .primary()
                                .disabled(!changed)
                                .tooltip(if changed {
                                    "Save current model settings to this profile"
                                } else {
                                    "No changes to save"
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let Some(name) = active_name.clone() else {
                                        return;
                                    };
                                    let saved = this
                                        .apply_domain_command_undoable(
                                            crate::scoped_domain::DomainCommand::UpdateModelProfile {
                                                name,
                                            },
                                            "Model profile updated",
                                            cx,
                                        )
                                        .is_some();
                                    if saved {
                                        this.save_feedback = None;
                                    }
                                })),
                        ),
                );
        let mut panel = col().gap_2().child(toolbar);
        if self.creating_model_profile {
            panel = panel.child(
                col()
                    .gap_2()
                    .child(label(
                        "Save the current global and stage models as a new profile.",
                        cx,
                    ))
                    .child(
                        row()
                            .flex_wrap()
                            .child(div().w(px(240.)).child(
                                Input::new(&self.model_profile_name).id("model-profile-name"),
                            ))
                            .child(button("model-profile-create", "Save profile").on_click(
                                cx.listener(|this, _, _, cx| {
                                    let name = this.model_profile_name.read(cx).value().to_string();
                                    if let Err(error) =
                                        this.store.validate_model_profile_name(&name)
                                    {
                                        this.show_save_error(error, cx);
                                        return;
                                    }
                                    if this
                                        .apply_domain_command_undoable(
                                            crate::scoped_domain::DomainCommand::CreateModelProfile {
                                                name,
                                            },
                                            "Model profile saved",
                                            cx,
                                        )
                                        .is_none()
                                    {
                                        return;
                                    }
                                    this.creating_model_profile = false;
                                    this.save_feedback = None;
                                }),
                            ))
                            .child(quiet_button("model-profile-cancel", "Cancel").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.creating_model_profile = false;
                                    cx.notify();
                                }),
                            )),
                    ),
            );
        }
        panel
    }

    pub fn set_execution_concurrency(&mut self, limit: usize, cx: &mut Context<Self>) {
        let limit = limit.clamp(1, crate::scheduler::MAX_CONCURRENT_RUNS);
        if self.store.max_concurrent_runs() == limit {
            return;
        }
        self.apply_scoped_settings(
            crate::scoped_settings::SettingsChange::ExecutionConcurrency(limit),
            cx,
        );
        cx.notify();
    }

    pub fn set_automation_concurrency(&mut self, limit: usize, cx: &mut Context<Self>) {
        let limit = limit.clamp(1, crate::scheduler::MAX_CONCURRENT_RUNS);
        if self.store.max_concurrent_automations() == limit {
            return;
        }
        self.apply_scoped_settings(
            crate::scoped_settings::SettingsChange::AutomationConcurrency(limit),
            cx,
        );
        cx.notify();
    }

    pub fn open_model_settings(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.record_origin();
        self.set_settings_tab(SettingsTab::Models);
        self.settings_scroll.set_offset(point(px(0.), px(0.)));
        self.navigate(Scope::Settings, w, cx);
    }

    pub fn refresh_global_model_input(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let model = self.store.default_model().clone();
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
                self.opencode_version
                    .as_ref()
                    .filter(|(checked_path, version)| checked_path == path && !version.is_empty())
                    .map(|(_, version)| version.as_str())
                    .unwrap_or("Not verified")
            };
            let status = if codex {
                &self.codex_status
            } else {
                &self.connection_status
            };
            let loading = if codex {
                self.discovering_codex_models()
            } else {
                self.discovering_models()
            };
            agents = agents.child(
                col()
                    .w_full()
                    .p_4()
                    .gap_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(colors.line))
                    .bg(rgb(colors.panel))
                    .child(
                        row()
                            .justify_between()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(selectable(agent)),
                            )
                            .child(label(format!("Version · {version}"), cx)),
                    )
                    .child(row().flex_wrap().child(label("Executable", cx)).child(
                        div().text_sm().child(selectable(if path.is_empty() {
                            "Not configured".to_owned()
                        } else {
                            path.to_owned()
                        })),
                    ))
                    .child(
                        row()
                            .flex_wrap()
                            .justify_between()
                            .pt_2()
                            .border_t_1()
                            .border_color(rgb(colors.line))
                            .child(label(status.clone(), cx))
                            .child(
                                button(
                                    if codex {
                                        "codex-refresh-models"
                                    } else {
                                        "research-refresh-models"
                                    },
                                    "Refresh local models",
                                )
                                .loading(loading)
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if codex {
                                            this.discover_codex_models(cx);
                                        } else {
                                            this.connection_project = this.research_project;
                                            this.discover_models(cx);
                                        }
                                    },
                                )),
                            ),
                    ),
            );
        }
        agents
    }

    pub fn model_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(feature = "qa")]
        crate::brand::note_settings_render();
        let build_general = || {
            #[cfg(feature = "qa")]
            crate::brand::note_general_render();
            let appearance = col()
            .gap_2()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Appearance")))
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
                            name,
                            self.store.appearance() == mode,
                            cx,
                        )
                        .when(self.store.appearance() == mode, |b| b.icon(assets::IconName::Check))
                        .on_click(cx.listener(move |a, _, w, cx| a.set_appearance(mode, w, cx)))
                    }),
                ),
            );
            col().gap_6()
            .child(appearance)
            .child(col().gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Execution")))
                .child(div().text_sm().child(selectable("Maximum simultaneous research tasks")))
                .child(row()
                    .child(button("concurrency-decrease", "−")
                        .accessibility_label("Decrease maximum simultaneous research tasks")
                        .disabled(self.store.concurrency_limit() <= 1)
                        .on_click(cx.listener(|a, _, _, cx| a.set_execution_concurrency(a.store.concurrency_limit().saturating_sub(1), cx))))
                    .child(div().w_8().text_center().child(selectable(self.store.concurrency_limit().to_string())))
                    .child(button("concurrency-increase", "+")
                        .accessibility_label("Increase maximum simultaneous research tasks")
                        .disabled(self.store.concurrency_limit() >= crate::scheduler::MAX_CONCURRENT_RUNS)
                        .on_click(cx.listener(|a, _, _, cx| a.set_execution_concurrency(a.store.concurrency_limit() + 1, cx)))))
                .child(label("1–10 research tasks. Recommended: 3; try 5 after checking stability. Higher values may cause more rate-limit errors, retries and browser load.", cx))
                .child(label("Research and automation tasks use separate limits and never occupy each other's slots. Saved immediately to this workspace. Lowering a limit lets running work finish. Dependencies, shared folders and browser Spaces can still reduce actual concurrency.", cx))
                .child(div().text_sm().pt_2().child(selectable("Maximum simultaneous automation periods")))
                .child(row()
                    .child(button("automation-concurrency-decrease", "−")
                        .accessibility_label("Decrease maximum simultaneous automation periods")
                        .disabled(self.store.automation_concurrency_limit() <= 1)
                        .on_click(cx.listener(|a, _, _, cx| a.set_automation_concurrency(a.store.automation_concurrency_limit().saturating_sub(1), cx))))
                    .child(div().w_8().text_center().child(selectable(self.store.automation_concurrency_limit().to_string())))
                    .child(button("automation-concurrency-increase", "+")
                        .accessibility_label("Increase maximum simultaneous automation periods")
                        .disabled(self.store.automation_concurrency_limit() >= crate::scheduler::MAX_CONCURRENT_RUNS)
                        .on_click(cx.listener(|a, _, _, cx| a.set_automation_concurrency(a.store.automation_concurrency_limit() + 1, cx)))))
                .child(label("Counts only periods that are actually running. Queued periods and periods waiting for a retry do not occupy a slot. Recommended: 2.", cx)))
            .when_some(self.workspace_root.as_ref(), |view, root| view.child(col().gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Workspace")))
                .child(label("Projects, reports, materials and app data are stored in this folder's workspace subfolder.", cx))
                .child(div().text_sm().child(selectable(root.to_string_lossy().into_owned())))))
        };
        let panel = match self.settings_tab() {
            SettingsTab::General => build_general(),
            SettingsTab::Publishing => self.publishing_settings_panel(cx),
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
                            .child(selectable("Settings")),
                    )
                    .child(
                        TabBar::new("settings-tabs")
                            .underline()
                            .selected_index(Self::settings_tab_index(self.settings_tab()))
                            .child(Tab::new().label("General"))
                            .child(Tab::new().label("Agent & models"))
                            .child(Tab::new().label("Publishing"))
                            .on_click(cx.listener(|this, index, w, cx| {
                                this.refresh_publishing_inputs(w, cx);
                                this.research_model_picker = None;
                                this.set_settings_tab(SettingsTab::ALL[*index]);
                                this.settings_scroll.set_offset(point(px(0.), px(0.)));
                                cx.notify();
                            })),
                    ),
            )
            .when(self.settings_tab() == SettingsTab::Models, |view| {
                view.child(
                    div()
                        .flex_shrink_0()
                        .px_6()
                        .py_4()
                        .border_b_1()
                        .border_color(rgb(palette(cx).line))
                        .child(self.model_profiles_panel(cx).max_w(px(840.))),
                )
            })
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
