use crate::{
    app::{Scope, TaskApp},
    appearance::palette,
    i18n::{InterfaceLanguage, tr},
    research::OutputLanguage,
    views::*,
};
use gpui_kit::{
    component::{
        Disableable, Selectable,
        button::{ButtonCustomVariant, ButtonVariants},
        input::Input,
        menu::{DropdownMenu, PopupMenuItem},
        switch::Switch,
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
    About,
}

impl SettingsTab {
    const ALL: [Self; 4] = [Self::General, Self::Models, Self::Publishing, Self::About];
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
        .tooltip(crate::i18n::tr(
            "Apply a saved global and stage model configuration",
        ))
        .dropdown_menu(move |mut menu, _, _| {
            for name in &names {
                let entity = entity.clone();
                let name = name.clone();
                menu = menu.item(
                    PopupMenuItem::new(crate::i18n::tr(name.clone()))
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
                    PopupMenuItem::new(crate::i18n::tr("Restore saved settings"))
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
                    PopupMenuItem::new(crate::i18n::tr("Delete current profile")).on_click(
                        move |_, _, cx| {
                            let _ = delete_entity.update(cx, |this, cx| {
                                let name = name.clone();
                                let saved = this
                                    .apply_domain_command_undoable(
                                        crate::scoped_domain::DomainCommand::DeleteModelProfile {
                                            name,
                                        },
                                        "Profile deleted · current settings kept · Undo to restore",
                                        cx,
                                    )
                                    .is_some();
                                if saved {
                                    this.save_feedback = None;
                                }
                            });
                        },
                    ),
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
                                .tooltip(crate::i18n::tr(if changed {
                                    "Save current model settings to this profile"
                                } else {
                                    "No changes to save"
                                }))
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

    pub fn set_output_language(&mut self, language: OutputLanguage, cx: &mut Context<Self>) {
        if self.store.output_language() == language {
            return;
        }
        self.apply_scoped_settings(
            crate::scoped_settings::SettingsChange::OutputLanguage(language),
            cx,
        );
        cx.notify();
    }

    pub fn set_interface_language(
        &mut self,
        language: InterfaceLanguage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.store.interface_language() == language {
            return;
        }
        if !self.apply_scoped_settings(
            crate::scoped_settings::SettingsChange::InterfaceLanguage(language),
            cx,
        ) {
            return;
        }
        crate::i18n::set_active(language);
        self.refresh_localized_placeholders(window, cx);
        cx.notify();
    }

    /// Input placeholders are stored state, so they are rebuilt explicitly
    /// when the interface language changes or a saved language is loaded.
    pub(crate) fn refresh_localized_placeholders(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search.update(cx, |state, cx| {
            state.set_placeholder(crate::i18n::tr("Filter this list…"), window, cx)
        });
        self.global_search.update(cx, |state, cx| {
            state.set_placeholder(
                crate::i18n::tr("Search research and automations…"),
                window,
                cx,
            )
        });
        self.refresh_model_placeholders(window, cx);
        self.executable_input.update(cx, |state, cx| {
            state.set_placeholder(crate::i18n::tr("Path to opencode"), window, cx)
        });
        self.codex_executable_input.update(cx, |state, cx| {
            state.set_placeholder(crate::i18n::tr("Path to codex"), window, cx)
        });
        self.directory_input.update(cx, |state, cx| {
            state.set_placeholder(crate::i18n::tr("/absolute/path/to/project"), window, cx)
        });
        self.model_profile_name.update(cx, |state, cx| {
            state.set_placeholder(crate::i18n::tr("Profile name"), window, cx)
        });
        self.project_brief.update(cx, |state, cx| {
            state.set_placeholder(
                crate::i18n::tr(
                    "e.g. Analyze Intel’s latest earnings and market reaction, with links to sources.",
                ),
                window,
                cx,
            )
        });
        self.research_title.update(cx, |state, cx| {
            state.set_placeholder(
                crate::i18n::tr("e.g. What changed in US technology stocks this week?"),
                window,
                cx,
            )
        });
        self.research_brief.update(cx, |state, cx| {
            state.set_placeholder(
                crate::i18n::tr("Market or ticker, time range, and research question…"),
                window,
                cx,
            )
        });
        self.automation_ui
            .refresh_localized_placeholders(window, cx);
        self.refresh_publishing_inputs(window, cx);
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
        let fallback = self.store.fallback_model().to_owned();
        self.fallback_model
            .update(cx, |s, cx| s.set_value(fallback, w, cx));
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
                    .rounded(px(crate::appearance::GROUP_RADIUS))
                    .border_1()
                    .border_color(rgb(colors.subtle_line))
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

    /// About this build: version, license, direct dependencies and updates.
    fn about_panel(&self, cx: &mut Context<Self>) -> Div {
        let colors = palette(cx);
        let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
        let updates_available = crate::updater::available();
        let check_label = match &self.update_available {
            Some(available) => tr(format!("Update to v{available}")),
            None => tr("Check for Updates…"),
        };
        let direct_dependencies: &[(&str, &str)] = &[
            ("gpui-kit", "0.9.1"),
            ("markdown", "1.0.0"),
            ("rusqlite (bundled)", "0.39"),
            ("serde / serde_json", "1"),
            ("chrono / chrono-tz", "0.4 / 0.10"),
            ("mermaid-rs-renderer", "0.3.1"),
            ("beautiful-mermaid-rs", "local"),
            ("latex-rust", "3.0.1"),
            ("usvg", "0.46"),
            ("sha2 / hmac", "0.10 / 0.12"),
            ("libc", "0.2"),
            ("objc2 family (macOS)", "0.6 / 0.3 / 0.3"),
            ("raw-window-handle (macOS)", "0.6"),
        ];
        let card = |title: gpui_kit::SharedString| -> Div {
            div()
                .w_full()
                .p_4()
                .gap_2()
                .rounded(px(crate::appearance::GROUP_RADIUS))
                .border_1()
                .border_color(rgb(colors.subtle_line))
                .bg(rgb(colors.panel))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(selectable(title)),
                )
        };
        col()
            .gap_4()
            .child(
                card(tr("Version"))
                    .child(div().text_sm().child(selectable(version)))
                    .child(label(
                        tr("Release builds take the version from the release tag; source builds show the Cargo package version."),
                        cx,
                    )),
            )
            .child(
                card(tr("App updates"))
                    .child(
                        button("about-check-for-updates", check_label)
                            .disabled(!updates_available || !crate::updater::can_check())
                            .on_click(cx.listener(|app, _, _, cx| app.check_for_updates(cx))),
                    )
                    .child(label(
                        if updates_available {
                            tr("Updates are checked once at launch and every 12 hours. Release notes and installation use Sparkle's standard window.")
                        } else {
                            tr("Updates are unavailable in this build.")
                        },
                        cx,
                    )),
            )
            .child(
                card(tr("License"))
                    .child(label(tr("GNU General Public License v3.0"), cx))
                    .child(
                        link_button("about-license-link", tr("View the license"))
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.open_url("https://www.gnu.org/licenses/gpl-3.0.html");
                            })),
                    )
                    .child(label(
                        tr("The full license text is bundled with the app as LICENSE.txt."),
                        cx,
                    )),
            )
            .child(
                card(tr("Open-source dependencies"))
                    .child(label(tr("Direct dependencies"), cx))
                    .child(
                        col().gap_1().children(
                            direct_dependencies
                                .iter()
                                .map(|(name, version)| {
                                    div()
                                        .text_sm()
                                        .child(selectable(format!("{name} · {version}")))
                                })
                                .collect::<Vec<_>>(),
                        ),
                    )
                    .child(label(tr("Third-party components"), cx))
                    .child(label(tr("Sparkle 2.9.3 (MIT) · Lucide icons (ISC)"), cx))
                    .child(label(
                        tr("Transitive dependencies and their license terms are recorded in Cargo.lock and in each upstream crate."),
                        cx,
                    )),
            )
    }

    pub fn model_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(feature = "qa")]
        crate::brand::note_settings_render();
        let build_general = || {
            #[cfg(feature = "qa")]
            crate::brand::note_general_render();
            let interface_language = self.store.interface_language();
            let interface_language_entity = cx.entity().downgrade();
            let language = self.store.output_language();
            let language_entity = cx.entity().downgrade();
            let interface_language_settings = col()
                .gap_2()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(selectable(tr("Interface language"))),
                )
                .child(label(
                    tr("Changes the language of controls, statuses and messages."),
                    cx,
                ))
                .child(
                    row()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .w(px(140.))
                                .child(selectable(tr("Display language"))),
                        )
                        .child(
                            button("interface-language-picker", tr(interface_language.label()))
                                .w(px(240.))
                                .dropdown_caret(true)
                                .tooltip(tr("Choose the language shown throughout the app"))
                                .dropdown_menu(move |mut menu, _, _| {
                                    for option in InterfaceLanguage::ALL {
                                        let entity = interface_language_entity.clone();
                                        menu = menu.item(
                                            PopupMenuItem::new(tr(option.label())).on_click(
                                                move |_, w, cx| {
                                                    let _ = entity.update(cx, |app, cx| {
                                                        app.set_interface_language(option, w, cx);
                                                    });
                                                },
                                            ),
                                        );
                                    }
                                    menu
                                }),
                        ),
                );
            let content = col()
                .gap_2()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(selectable("Content")),
                )
                .child(label(
                    "Applies to generated research and automation output.",
                    cx,
                ))
                .child(
                    row()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .w(px(140.))
                                .child(selectable("Output language")),
                        )
                        .child(
                            button("output-language-picker", language.label())
                                .w(px(240.))
                                .dropdown_caret(true)
                                .tooltip(crate::i18n::tr(
                                    "Choose the language for generated research output",
                                ))
                                .dropdown_menu(move |mut menu, _, _| {
                                    for option in OutputLanguage::ALL {
                                        let entity = language_entity.clone();
                                        menu = menu.item(
                                            PopupMenuItem::new(crate::i18n::tr(option.label()))
                                                .checked(option == language)
                                                .on_click(move |_, _, cx| {
                                                    let _ = entity.update(cx, |app, cx| {
                                                        app.set_output_language(option, cx);
                                                    });
                                                }),
                                        );
                                    }
                                    menu
                                }),
                        ),
                );
            let appearance = col()
                .gap_2()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(selectable("Appearance")),
                )
                .child(
                    row().flex_wrap().children(
                        [
                            (
                                "appearance-system",
                                "Follow system",
                                assets::IconName::Monitor,
                                crate::appearance::Appearance::System,
                            ),
                            (
                                "appearance-light",
                                "Light",
                                assets::IconName::Sun,
                                crate::appearance::Appearance::Light,
                            ),
                            (
                                "appearance-dark",
                                "Dark",
                                assets::IconName::Moon,
                                crate::appearance::Appearance::Dark,
                            ),
                        ]
                        .into_iter()
                        .map(|(id, name, icon, mode)| {
                            let selected = self.store.appearance() == mode;
                            let colors = palette(cx);
                            tool_icon(id, icon, name)
                                .custom(
                                    ButtonCustomVariant::new(cx)
                                        .foreground(
                                            rgb(if selected {
                                                colors.accent
                                            } else {
                                                colors.muted
                                            })
                                            .into(),
                                        )
                                        .color(transparent_black())
                                        .hover(rgb(colors.hover).into())
                                        .active(rgb(colors.accent_surface).into()),
                                )
                                .selected(selected)
                                .toggled(selected)
                                .on_click(
                                    cx.listener(move |a, _, w, cx| a.set_appearance(mode, w, cx)),
                                )
                        }),
                    ),
                );
            let updates_available = crate::updater::available();
            let check_label = match &self.update_available {
                Some(version) => crate::i18n::tr_string(format!("Update to v{version}")),
                None => crate::i18n::tr_string("Check for Updates…"),
            };
            let updates = col()
                .gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("App updates")))
                .child(row().items_center().justify_between().gap_2().w_full().pr_2()
                    .child(div().child(selectable("Automatically check for updates")))
                    .child(
                        Switch::new("settings-automatic-updates")
                            .checked(crate::updater::automatic())
                            .disabled(!updates_available)
                            .accessibility_label(crate::i18n::tr("Automatically check for updates"))
                            .on_click(cx.listener(|app, enabled, _, cx| {
                                app.set_automatic_updates(*enabled, cx);
                            })),
                    ),
                )
                .child(button("check-for-updates", check_label)
                    .disabled(!updates_available || !crate::updater::can_check())
                    .on_click(cx.listener(|app, _, _, cx| app.check_for_updates(cx))))
                .child(label(if updates_available {
                    "Updates are checked every 12 hours. Installation requires your confirmation."
                } else {
                    "Updates are unavailable in this build."
                }, cx));
            col().gap_6()
            .child(interface_language_settings)
            .child(content)
            .child(appearance)
            .child(updates)
            .child(col().gap_2()
                .child(Switch::new("settings-liquid-glass")
                    .label(crate::i18n::tr("Liquid Glass"))
                    .checked(self.store.liquid_glass())
                    .disabled(self.liquid_glass_unavailable_reason().is_some())
                    .on_click(cx.listener(|app, enabled, window, cx| {
                        app.set_liquid_glass(*enabled, window, cx);
                    })))
                .child(label(self.liquid_glass_unavailable_reason().unwrap_or(
                    "Native translucency."
                ), cx)))
            .child(col().gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Execution")))
                .child(div().text_sm().child(selectable("Research tasks")))
                .child(row()
                    .child(tool_icon(
                            "concurrency-decrease",
                            assets::IconName::Minus,
                            "Decrease research task limit",
                        )
                        .tooltip(crate::i18n::tr(if self.store.concurrency_limit() <= 1 {
                            "Minimum is 1."
                        } else {
                            "Decrease the limit. Running research tasks continue."
                        }))
                        .disabled(self.store.concurrency_limit() <= 1)
                        .on_click(cx.listener(|a, _, _, cx| a.set_execution_concurrency(a.store.concurrency_limit().saturating_sub(1), cx))))
                    .child(div().w_8().text_center().child(selectable(self.store.concurrency_limit().to_string())))
                    .child(tool_icon(
                            "concurrency-increase",
                            assets::IconName::Plus,
                            "Increase research task limit",
                        )
                        .tooltip(crate::i18n::tr(if self.store.concurrency_limit()
                            >= crate::scheduler::MAX_CONCURRENT_RUNS
                        {
                            "Maximum is 10."
                        } else {
                            "Increase the limit."
                        }))
                        .disabled(self.store.concurrency_limit() >= crate::scheduler::MAX_CONCURRENT_RUNS)
                        .on_click(cx.listener(|a, _, _, cx| a.set_execution_concurrency(a.store.concurrency_limit() + 1, cx)))))
                .child(label("1–10 · Recommended 3 · Higher limits can increase rate-limit errors.", cx))
                .child(div().text_sm().pt_2().child(selectable("Automation periods")))
                .child(row()
                    .child(tool_icon(
                            "automation-concurrency-decrease",
                            assets::IconName::Minus,
                            "Decrease automation period limit",
                        )
                        .tooltip(crate::i18n::tr(if self.store.automation_concurrency_limit() <= 1 {
                            "Minimum is 1."
                        } else {
                            "Decrease the limit. Running automation periods continue."
                        }))
                        .disabled(self.store.automation_concurrency_limit() <= 1)
                        .on_click(cx.listener(|a, _, _, cx| a.set_automation_concurrency(a.store.automation_concurrency_limit().saturating_sub(1), cx))))
                    .child(div().w_8().text_center().child(selectable(self.store.automation_concurrency_limit().to_string())))
                    .child(tool_icon(
                            "automation-concurrency-increase",
                            assets::IconName::Plus,
                            "Increase automation period limit",
                        )
                        .tooltip(crate::i18n::tr(if self.store.automation_concurrency_limit()
                            >= crate::scheduler::MAX_CONCURRENT_RUNS
                        {
                            "Maximum is 10."
                        } else {
                            "Increase the limit."
                        }))
                        .disabled(self.store.automation_concurrency_limit() >= crate::scheduler::MAX_CONCURRENT_RUNS)
                        .on_click(cx.listener(|a, _, _, cx| a.set_automation_concurrency(a.store.automation_concurrency_limit() + 1, cx)))))
                .child(label("1–10 · Recommended 2 · Only running periods use a slot.", cx)))
            .when_some(self.workspace_root.as_ref(), |view, root| view.child(col().gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Workspace")))
                .child(div().text_sm().child(selectable(root.to_string_lossy().into_owned())))))
        };
        let panel = match self.settings_tab() {
            SettingsTab::General => build_general(),
            SettingsTab::Publishing => self.publishing_settings_panel(cx),
            SettingsTab::Models => self.research_settings_panel(cx),
            SettingsTab::About => self.about_panel(cx),
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
                            .child(Tab::new().label(crate::i18n::tr("General")))
                            .child(Tab::new().label(crate::i18n::tr("Agent & models")))
                            .child(Tab::new().label(crate::i18n::tr("Publishing")))
                            .child(Tab::new().label(crate::i18n::tr("About")))
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
