//! Startup requirements are checked off the UI thread, without running a model.
use crate::{
    app::TaskApp,
    appearance::palette,
    controls::*,
    views::{col, label, observed, row, selectable},
};
use gpui_kit::{
    component::{Disableable, button::ButtonVariants, input::Input},
    prelude::FluentBuilder,
    *,
};
use std::{path::Path, sync::mpsc};

pub(crate) struct PreparedWorkspace {
    storage: crate::persistence::Storage,
    lock: std::fs::File,
    store: Option<crate::domain::Store>,
    backup: Option<std::path::PathBuf>,
    live: Option<crate::persistence_live::LiveStartup>,
}
pub(crate) enum WorkspaceOpenEvent {
    Progress(&'static str),
    Complete(Result<PreparedWorkspace, String>),
}

type Checks = [Result<String, String>; 3];
#[derive(Default)]
pub struct Startup {
    pub required: bool,
    pub silent: bool,
    pub pending: Option<mpsc::Receiver<Checks>>,
    pub(crate) workspace_pending: Option<mpsc::Receiver<WorkspaceOpenEvent>>,
    pub(crate) workspace_phase: Option<&'static str>,
    pub(crate) upgrade_backup: Option<std::path::PathBuf>,
    paths: [String; 3],
    checks: Option<Checks>,
}
fn probe_agent(agent: &str, path: &str) -> Result<String, String> {
    if path.trim().is_empty() {
        return Err(format!("{agent} is not configured."));
    }
    let version = crate::opencode::capture(Path::new(path), &std::env::temp_dir(), &["--version"])?;
    if version.trim().is_empty() {
        return Err(format!("{agent} returned no version. Check the executable path."));
    }
    Ok(version.trim().to_owned())
}
impl TaskApp {
    pub(crate) fn begin_workspace_open(&mut self, storage: crate::persistence::Storage, cx: &mut Context<Self>) {
        if self.startup.workspace_pending.is_some() { return; }
        if self.pending_workspace_switch.is_some() { return; }
        // A clean retry of the current workspace must not unlink/reopen its
        // database while the old worker/read threads may still be closing it.
        // Reuse the attached store and only retry the failed worker/reader.
        let same_open_workspace = self
            .storage
            .as_ref()
            .is_some_and(|current| current.path() == storage.path());
        if same_open_workspace
            && self.storage.is_some()
            && self.live_clean_for_quit()
        {
            self.storage_error.clear();
            if self.live_required && !self.live_persistence_active()
                && let Err(error) = self.retry_live_attachment(cx)
            {
                self.storage_error = error;
                self.startup.silent = false;
                cx.notify();
                return;
            }
            if self.history_reader.is_none()
                && let Some((path, root, incarnation)) = self.live_attach.clone()
            {
                self.start_history_reader(&path, &root, incarnation);
            }
            self.workspace_required = false;
            self.startup.required = true;
            self.startup.silent = true;
            self.check_startup(cx);
            cx.notify();
            return;
        }
        // A workspace switch owns the old worker until every accepted write is
        // durable. An unavailable incremental backend with dirty edits cannot
        // be abandoned either: that would silently drop the only copy.
        if self.live_required && !self.live_clean_for_quit() {
            if !self.live_persistence_active() {
                self.storage_error =
                    "Not switched: the previous workspace still has unsaved changes and persistence is unavailable. Retry saving or reopen the same workspace."
                        .into();
                self.toast = "Not switched · previous changes are not saved".into();
                cx.notify();
                return;
            }
            self.pending_workspace_switch = Some(storage);
            let _ = self.persist_live_followup();
            if let Some(pending) = self.live_primary.as_ref()
                && pending.generation == 0
            {
                let _ = self.submit_primary_group_in_persist();
            }
            self.start_pump(cx);
            self.toast = "Saving before switching workspace…".into();
            cx.notify();
            return;
        }
        // One lock/worker handoff protocol. For the same path, reuse the lock
        // this process already owns; dropping it would let a second process
        // acquire the workspace while the old worker is still shutting down.
        // For a different path, release it only after the clean check above.
        let same_workspace = self
            .storage
            .as_ref()
            .is_some_and(|current| current.path() == storage.path());
        let existing_lock = if same_workspace {
            self._storage_lock.take()
        } else {
            self._storage_lock = None;
            None
        };
        self.reset_live_state();
        self.storage = None;
        self.begin_workspace_open_now(storage, existing_lock, cx);
    }

    pub(crate) fn poll_pending_workspace_switch(&mut self, cx: &mut Context<Self>) {
        if self.startup.workspace_pending.is_some() { return; }
        let Some(storage) = self.pending_workspace_switch.take() else { return; };
        let same_open_workspace = self
            .storage
            .as_ref()
            .is_some_and(|current| current.path() == storage.path());
        if same_open_workspace && self.storage.is_some() && self.live_clean_for_quit() {
            // Already attached to this exact workspace; a deferred "switch" was
            // only waiting for the last write. Do not close/reopen the file.
            self.pending_workspace_switch = None;
            self.storage_error.clear();
            self.toast = "Workspace saved".into();
            self.workspace_required = false;
            cx.notify();
            return;
        }
        if self.live_required && !self.live_clean_for_quit() {
            if !self.live_persistence_active() {
                self.pending_workspace_switch = None;
                self.storage_error =
                    "Workspace switch cancelled: the previous workspace still has unsaved changes and persistence is unavailable."
                        .into();
                return;
            }
            self.pending_workspace_switch = Some(storage);
            return;
        }
        let same_workspace = self
            .storage
            .as_ref()
            .is_some_and(|current| current.path() == storage.path());
        let existing_lock = if same_workspace {
            self._storage_lock.take()
        } else {
            self._storage_lock = None;
            None
        };
        self.reset_live_state();
        self.storage = None;
        self.begin_workspace_open_now(storage, existing_lock, cx);
    }

    fn begin_workspace_open_now(
        &mut self,
        storage: crate::persistence::Storage,
        existing_lock: Option<std::fs::File>,
        cx: &mut Context<Self>,
    ) {
        // An old workspace's clock must never dispatch into the new store.
        self.cancel_automation_timer();
        self.workspace_required = true;
        self.startup.required = true;
        self.storage_error.clear();
        self.startup.workspace_phase = Some("Checking workspace data…");
        self.startup.pending = None;
        self.startup.checks = None;
        let (tx, rx) = mpsc::channel();
        self.startup.workspace_pending = Some(rx);
        std::thread::spawn(move || {
            let result = (|| {
                let lock = match existing_lock {
                    Some(lock) => lock,
                    None => storage.acquire_lock()?,
                };
                let backup = storage.upgrade_paths(|phase| { let _ = tx.send(WorkspaceOpenEvent::Progress(phase)); })?;
                let mut live = crate::persistence_live::select_existing(&storage)?;
                if live.is_none() {
                    live = crate::persistence_live::create_new(&storage)?;
                }
                let store = if let Some(live) = &mut live {
                    live.migrated_store.take()
                } else {
                    storage.load()?
                };
                Ok(PreparedWorkspace { storage, lock, store, backup, live })
            })();
            let _ = tx.send(WorkspaceOpenEvent::Complete(result));
        });
        self.start_pump(cx);
        cx.notify();
    }
    pub(crate) fn poll_workspace_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        loop {
            let event = match self.startup.workspace_pending.as_ref().map(|rx| rx.try_recv()) {
                Some(Ok(event)) => event,
                Some(Err(mpsc::TryRecvError::Disconnected)) => WorkspaceOpenEvent::Complete(Err("Workspace preparation stopped. Please retry.".into())),
                _ => break,
            };
            match event {
                WorkspaceOpenEvent::Progress(phase) => self.startup.workspace_phase = Some(phase),
                WorkspaceOpenEvent::Complete(result) => {
                    self.startup.workspace_pending = None;
                    self.startup.workspace_phase = None;
                    match result {
                        Ok(prepared) => {
                            self.startup.upgrade_backup = prepared.backup;
                            self.attach_loaded_storage_live(
                                prepared.storage,
                                Ok((prepared.lock, prepared.store)),
                                prepared.live,
                                window,
                                cx,
                            );
                            self.workspace_required = self.storage.is_none() || !self.storage_error.is_empty();
                            if !self.workspace_required {
                                self.startup.silent = self.startup.upgrade_backup.is_none();
                                self.check_startup(cx);
                            } else { self.startup.silent = false; }
                        }
                        Err(error) => {
                            self.storage_error = error;
                            self.startup.silent = false;
                        }
                    }
                    cx.notify();
                    break;
                }
            }
        }
    }
    pub fn check_startup(&mut self, cx: &mut Context<Self>) {
        if self.workspace_required || self.startup.workspace_pending.is_some() || !self.storage_error.is_empty() { return; }
        let paths = [
            self.executable_input.read(cx).value().trim().to_string(),
            self.codex_executable_input.read(cx).value().trim().to_string(),
            self.ego_input.read(cx).value().trim().to_string(),
        ];
        self.startup.paths = paths.clone();
        self.startup.checks = None;
        let (tx, rx) = mpsc::channel();
        self.startup.pending = Some(rx);
        std::thread::spawn(move || {
            let checks = std::thread::scope(|scope| {
                let open = scope.spawn(|| probe_agent("OpenCode", &paths[0]));
                let codex = scope.spawn(|| probe_agent("Codex", &paths[1]));
                let ego = scope.spawn(|| crate::ego::probe(&paths[2], &std::env::temp_dir()));
                [open.join(), codex.join(), ego.join()].map(|result| {
                    result.unwrap_or_else(|_| Err("Check stopped. Please retry.".into()))
                })
            });
            let _ = tx.send(checks);
        });
        self.start_pump(cx);
        cx.notify();
    }
    pub fn poll_startup(&mut self, cx: &mut Context<Self>) {
        let result = self
            .startup
            .pending
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(checks) => Some(checks),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(std::array::from_fn(|_| Err("Check stopped. Please retry.".into()))),
            });
        if let Some(checks) = result {
            self.startup.pending = None;
            self.opencode_version = checks[0].as_ref().ok()
                .map(|version| (self.startup.paths[0].clone(), version.clone()));
            self.codex_version = checks[1].as_ref().ok().cloned();
            self.startup.checks = Some(checks);
            let ready = self.startup_ready(cx)
                && self.storage.is_some() && self.storage_error.is_empty();
            self.startup.finish_initial_check(ready);
            if ready && !self.startup.required { self.check_interrupted_sessions(cx); }
            cx.notify();
        }
    }
    fn startup_ready(&self, cx: &Context<Self>) -> bool {
        self.startup.ready_for(
            !self.workspace_required,
            self.executable_input.read(cx).value().trim(),
            self.codex_executable_input.read(cx).value().trim(),
            self.ego_input.read(cx).value().trim(),
        )
    }
    pub fn startup_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        if let Some(phase) = self.startup.workspace_phase.or(self.workspace_setup.as_ref().map(|_| "Preparing workspace files…")) {
            return col().id("workspace-upgrading").size_full()
                .bg(rgb(colors.canvas)).text_color(rgb(colors.text))
                .items_center().justify_center().gap_4()
                .child(self.sidebar_logo.clone())
                .child(col().items_center().gap_2()
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(selectable("Preparing your workspace")))
                    .child(div().id("workspace-upgrade-phase").text_sm().text_color(rgb(colors.muted)).child(selectable(phase))))
                .child(label("Your saved work stays here. Setup continues when this is finished.", cx))
                .into_any_element();
        }
        if self.startup.silent {
            return observed(
                col()
                    .id("startup-checking")
                    .size_full()
                    .bg(rgb(colors.canvas))
                    .items_center()
                    .justify_center()
                    .child(label("Starting…", cx)),
            );
        }
        let dependency = |index: usize| {
            let current = match index {
                0 => self.executable_input.read(cx).value(),
                1 => self.codex_executable_input.read(cx).value(),
                _ => self.ego_input.read(cx).value(),
            };
            let checked = &self.startup.paths[index];
            if checked.is_empty() && self.startup.checks.is_none() && self.startup.pending.is_none()
            {
                return ("Not checked", colors.muted, String::new(), false);
            }
            if current.trim() != checked {
                return (
                    "Check again",
                    colors.warning,
                    "Path changed. Run the check again.".into(),
                    false,
                );
            }
            match self
                .startup
                .checks
                .as_ref()
                .map(|checks| &checks[index])
            {
                Some(Ok(message)) => ("Ready", colors.success, message.clone(), true),
                Some(Err(message)) if index < 2 => ("Unavailable", colors.muted, message.clone(), false),
                Some(Err(message)) => ("Needs attention", colors.danger, message.clone(), false),
                None => ("Checking…", colors.accent, String::new(), false),
            }
        };
        let open = dependency(0);
        let codex = dependency(1);
        let ego = dependency(2);
        let agent_ready = open.3 || codex.3;
        let workspace = if self.workspace_setup.is_some() {
            ("Preparing…", colors.accent)
        } else if !self.storage_error.is_empty() {
            ("Needs attention", colors.danger)
        } else if self.workspace_required {
            ("Choose folder", colors.warning)
        } else {
            ("Ready", colors.success)
        };
        let ready =
            usize::from(!self.workspace_required) + usize::from(agent_ready) + usize::from(ego.3);
        let badge = |text: &'static str, color: u32| {
            let icon = match text {
                "Ready" => assets::IconName::Check,
                "Needs attention" => assets::IconName::CircleAlert,
                "Check again" => assets::IconName::RefreshCw,
                "Checking…" | "Preparing…" => assets::IconName::Loader,
                _ => assets::IconName::Circle,
            };
            row().gap_1().child(gpui_kit::component::Icon::new(icon).size_4())
                .flex_shrink_0()
                .px_2()
                .py_1()
                .rounded(px(crate::appearance::CONTROL_RADIUS))
                .bg(rgb(colors.tag_surface))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(color))
                .child(selectable(text))
        };
        let heading = |title: &'static str, state: &'static str, color: u32| {
            row()
                .justify_between()
                .gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable(title)))
                .child(badge(state, color))
        };
        let card = || {
            col()
                .gap_2()
                .p_3()
                .rounded(px(crate::appearance::SURFACE_RADIUS))
                .border_1()
                .border_color(rgb(colors.subtle_line))
                .bg(rgb(colors.panel))
        };
        let chosen = self.directory_input.read(cx).value().trim().to_string();
        let chosen_path = Path::new(&chosen);
        let data_folder = (!chosen.is_empty()).then(|| chosen_path.join("workspace"));
        let existing_workspace = data_folder
            .as_ref()
            .is_some_and(|path| path.join("workspace.sqlite3").is_file());
        // Selecting the data folder itself would nest another `workspace` level.
        let selects_data_folder =
            !chosen.is_empty() && chosen_path.join("workspace.sqlite3").is_file();
        let data_preview = data_folder
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Choose a folder above to preview its data folder.".into());
        col().id("startup").size_full().overflow_y_scroll().bg(rgb(colors.canvas)).text_color(rgb(colors.text)).text_sm().items_center()
            .child(col().w_full().max_w(px(840.)).p_6().gap_3().flex_shrink_0()
                .child(row().justify_between()
                    .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child(selectable("Set up your app")))
                    .child(div().text_sm().text_color(rgb(colors.muted)).child(selectable(format!("{ready} / 3 ready")))))
                .child(label("Check these essentials to get started.", cx))
                .child(card()
                    .child(heading("1 · Workspace", workspace.0, workspace.1))
                    .child(label("Stores projects, reports and local data. Existing files are preserved.", cx))
                    .child(div().text_xs().text_color(rgb(colors.muted)).child(selectable(
                        "The app always creates a `workspace` subfolder inside the folder you choose. Everything it creates lives there; the folder you pick stays otherwise untouched.",
                    )))
                    .when(self.workspace_required, |view| view.child(row().gap_2()
                        .child(div().flex_1().min_w_0().child(Input::new(&self.directory_input).id("workspace-root")))
                        .child(button("browse-workspace", "Browse…").disabled(self.workspace_setup.is_some()).on_click(cx.listener(|this,_,_,cx| this.choose_workspace(cx))))
                        .child(button("setup-workspace", "Set workspace").disabled(self.workspace_setup.is_some()).on_click(cx.listener(|this,_,_,cx| this.setup_workspace(cx))))))
                    .when(self.workspace_required, |view| view.child(
                        col().gap_1().p_2().rounded(px(crate::appearance::CONTROL_RADIUS)).bg(rgb(colors.tag_surface))
                            .child(
                                row().gap_1().items_center()
                                    .child(gpui_kit::component::Icon::new(assets::IconName::Folder).size_4().text_color(rgb(colors.muted)))
                                    .child(div().text_xs().font_weight(FontWeight::MEDIUM).child(selectable("Data folder")))
                            )
                            .child(row().w_full().child(
                                single_line_text("workspace-data-preview", data_preview.clone())
                                    .text_sm()
                                    .text_color(rgb(colors.text)),
                            ))
                            .when(existing_workspace, |view| view.child(label(
                                "Existing workspace data found here. The app will reuse it.",
                                cx,
                            )))
                            .when(selects_data_folder, |view| view.child(crate::views::notice(
                                crate::views::NoticeTone::Warning,
                                "This folder already contains workspace.sqlite3. It looks like an app data folder; choose its parent folder instead, otherwise data will be nested one level deeper at workspace/workspace.",
                                cx,
                            )))
                    ))
                    .when(!self.workspace_required, |view| view.child(single_line_text("startup-workspace-path", self.workspace_root.as_ref().map(|p| p.display().to_string()).unwrap_or_default())))
                    .when_some(self.startup.upgrade_backup.as_ref(), |view, backup| view.child(col().gap_1()
                        .child(label("Workspace data upgraded. The previous database was backed up.", cx))
                        .child(single_line_text("workspace-upgrade-backup", backup.display().to_string()))))
                    .when(!self.storage_error.is_empty(), |view| view.child(crate::views::notice(
                        crate::views::NoticeTone::Danger,
                        self.storage_error.clone(),
                        cx,
                    )))
                    .when(self.workspace_required && self.workspace_root.is_some() && !self.storage_error.is_empty(), |view| view.child(
                        button("retry-workspace-upgrade", "Retry workspace preparation").on_click(cx.listener(|this,_,_,cx| {
                            if let Some(root) = &this.workspace_root { this.begin_workspace_open(crate::persistence::Storage::new(root.join("workspace/workspace.sqlite3")), cx); }
                        })))))
                .child(card()
                    .child(heading("2 · AI agents", if agent_ready { "Ready" } else { "At least one required" }, if agent_ready { colors.success } else { colors.warning }))
                    .child(label("Install OpenCode or Codex. Either one is enough to start; models and login are configured separately.", cx))
                    .child(heading("OpenCode", open.0, open.1))
                    .child(row().justify_between().gap_2().child(label("OpenCode CLI", cx))
                        .child(link_button("install-opencode", "Install").icon(assets::IconName::ExternalLink).on_click(|_,_,cx| cx.open_url("https://opencode.ai"))))
                    .child(Input::new(&self.executable_input).id("startup-opencode"))
                    .when(!open.2.is_empty(), |view| view.child(div().text_xs().text_color(rgb(colors.muted)).child(selectable(open.2.clone()))))
                    .child(heading("Codex", codex.0, codex.1))
                    .child(row().justify_between().gap_2().child(label("Codex CLI", cx))
                        .child(link_button("install-codex", "Install").icon(assets::IconName::ExternalLink).on_click(|_,_,cx| cx.open_url("https://developers.openai.com/codex/cli"))))
                    .child(Input::new(&self.codex_executable_input).id("startup-codex"))
                    .when(!codex.2.is_empty(), |view| view.child(div().text_xs().text_color(rgb(colors.muted)).child(selectable(codex.2.clone())))))
                .child(card()
                    .child(heading("3 · ego(lite)", ego.0, ego.1))
                    .child(row().justify_between().gap_2().child(label("Browser access · finish its first launch after installing.", cx))
                        .child(row().gap_3()
                            .when(!crate::ego::is_installed(&self.ego_input.read(cx).value()), |d| d.child(link_button("startup-install-ego", "Install").icon(assets::IconName::ExternalLink).on_click(|_,_,cx| cx.open_url("https://lite.ego.app"))))
                            .child(link_button("startup-open-ego", "Open").icon(assets::IconName::ExternalLink).on_click(cx.listener(|this,_,_,cx| this.open_ego(cx))))))
                    .child(Input::new(&self.ego_input).id("startup-ego"))
                    .when(!ego.2.is_empty(), |view| view.child(div().text_xs().text_color(rgb(if ego.3 { colors.muted } else { ego.1 })).child(selectable(ego.2.clone()))))
                    .when(!self.error.is_empty(), |view| view.child(crate::views::notice(
                        crate::views::NoticeTone::Danger,
                        self.error.clone(),
                        cx,
                    ))))
                .child(row().justify_between().gap_3()
                    .child(div().flex_1().min_w_0().text_xs().text_color(rgb(colors.muted)).child(selectable("Next: configure your model and provider in Settings.")))
                    .child(row().gap_2()
                        .child(button("startup-recheck", "Check again").disabled(self.workspace_required || self.startup.pending.is_some()).on_click(cx.listener(|this,_,_,cx| this.check_startup(cx))))
                        .child(button("startup-continue", "Continue").primary().disabled(!self.startup_ready(cx)).on_click(cx.listener(|this,_,_,cx| {
                            if !this.startup_ready(cx) { return; }
                            this.store.set_opencode_path(this.executable_input.read(cx).value().trim().into());
                            this.store.research_config_mut().codex_path = this.codex_executable_input.read(cx).value().trim().into();
                            this.store.research_config_mut().ego_path = this.startup.paths[2].clone();
                            this.persist_if_changed(true);
                            if this.storage.is_some() && this.storage_error.is_empty() {
                                this.startup.required = false;
                                this.check_interrupted_sessions(cx);
                            }
                            cx.notify();
                        }))))))
            .into_any_element()
    }
}

impl Startup {
    fn finish_initial_check(&mut self, ready: bool) {
        if self.silent {
            self.silent = false;
            self.required = !ready;
        }
    }
    fn ready_for(&self, workspace_ready: bool, opencode: &str, codex: &str, ego: &str) -> bool {
        workspace_ready
            && self.workspace_pending.is_none()
            && self.pending.is_none()
            && self.paths[2] == ego
            && self.checks.as_ref().is_some_and(|checks| {
                checks[2].is_ok()
                    && ((checks[0].is_ok() && self.paths[0] == opencode)
                        || (checks[1].is_ok() && self.paths[1] == codex))
            })
    }

}
#[cfg(test)]
mod tests {
    use super::{Startup, probe_agent};
    use std::sync::mpsc;
    #[test]
    fn all_requirements_and_current_paths_must_pass() {
        let mut startup = Startup {
            paths: ["open".into(), "codex".into(), "ego".into()],
            ..Default::default()
        };
        assert!(!startup.ready_for(true, "open", "codex", "ego"));
        startup.checks = Some([Ok("ready".into()), Err("missing".into()), Err("missing".into())]);
        assert!(!startup.ready_for(true, "open", "codex", "ego"));
        startup.checks = Some([Ok("ready".into()), Err("missing".into()), Ok("ready".into())]);
        assert!(!startup.ready_for(false, "open", "codex", "ego"));
        assert!(!startup.ready_for(true, "changed", "codex", "ego"));
        assert!(!startup.ready_for(true, "open", "codex", "changed"));
        assert!(startup.ready_for(true, "open", "codex", "ego"));
        let (_, rx) = mpsc::channel();
        startup.pending = Some(rx);
        assert!(!startup.ready_for(true, "open", "codex", "ego"));
    }
    #[test]
    fn either_agent_is_sufficient_but_browser_and_current_success_path_are_required() {
        for open in [false, true] {
            for codex in [false, true] {
                for browser in [false, true] {
                    let result = |ready| if ready { Ok("version".into()) } else { Err("missing".into()) };
                    let mut startup = Startup {
                        paths: ["open".into(), "codex".into(), "ego".into()],
                        checks: Some([result(open), result(codex), result(browser)]),
                        ..Default::default()
                    };
                    assert_eq!(startup.ready_for(true, "open", "codex", "ego"), (open || codex) && browser);
                    assert_eq!(startup.ready_for(true, "changed", "codex", "ego"), codex && browser);
                    assert_eq!(startup.ready_for(true, "open", "changed", "ego"), open && browser);
                    assert!(!startup.ready_for(true, "changed", "changed", "ego"));
                    assert!(!startup.ready_for(true, "open", "codex", "changed"));
                    startup.silent = true;
                    let ready = startup.ready_for(true, "open", "codex", "ego");
                    startup.finish_initial_check(ready);
                    assert_eq!(startup.required, !((open || codex) && browser));
                }
            }
        }
    }

    #[test]
    fn silent_start_skips_setup_only_when_ready() {
        for ready in [true, false] {
            let mut startup = Startup { required: true, silent: true, ..Default::default() };
            startup.finish_initial_check(ready);
            assert!(!startup.silent);
            assert_eq!(startup.required, !ready);
        }
        // A manual retry must keep setup open so edited paths can be saved.
        let mut startup = Startup { required: true, ..Default::default() };
        startup.finish_initial_check(true);
        assert!(startup.required);
    }
    #[test]
    fn executable_must_run_and_return_a_version() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(crate::opencode::new_session_id());
        std::fs::create_dir(&root).unwrap();
        let cli = root.join("opencode");
        assert!(probe_agent("OpenCode", cli.to_str().unwrap()).is_err());
        for (script, valid) in [("exit 0", false), ("exit 1", false), ("echo 1.2.3", true)] {
            std::fs::write(&cli, format!("#!/bin/sh\n{script}\n")).unwrap();
            std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
            for agent in ["OpenCode", "Codex"] {
                assert_eq!(probe_agent(agent, cli.to_str().unwrap()).is_ok(), valid);
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
