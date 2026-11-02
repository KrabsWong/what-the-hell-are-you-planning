//! Startup requirements are checked off the UI thread, without running a model.
use crate::{
    app::TaskApp,
    appearance::palette,
    controls::*,
    views::{col, label, row},
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
}
pub(crate) enum WorkspaceOpenEvent {
    Progress(&'static str),
    Complete(Result<PreparedWorkspace, String>),
}

type Checks = (Result<String, String>, Result<String, String>);
#[derive(Default)]
pub struct Startup {
    pub required: bool,
    pub silent: bool,
    pub pending: Option<mpsc::Receiver<Checks>>,
    pub(crate) workspace_pending: Option<mpsc::Receiver<WorkspaceOpenEvent>>,
    pub(crate) workspace_phase: Option<&'static str>,
    pub(crate) upgrade_backup: Option<std::path::PathBuf>,
    paths: (String, String),
    checks: Option<Checks>,
}
fn probe_opencode(path: &str) -> Result<String, String> {
    if path.trim().is_empty() {
        return Err("Enter the OpenCode executable path.".into());
    }
    let version = crate::opencode::capture(Path::new(path), &std::env::temp_dir(), &["--version"])?;
    if version.trim().is_empty() {
        return Err("OpenCode returned no version. Check the executable path.".into());
    }
    Ok(format!("Ready · {}", version.trim()))
}
impl TaskApp {
    pub(crate) fn begin_workspace_open(&mut self, storage: crate::persistence::Storage, cx: &mut Context<Self>) {
        if self.startup.workspace_pending.is_some() { return; }
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
                let lock = storage.acquire_lock()?;
                let backup = storage.upgrade_paths(|phase| { let _ = tx.send(WorkspaceOpenEvent::Progress(phase)); })?;
                let store = storage.load()?;
                Ok(PreparedWorkspace { storage, lock, store, backup })
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
                            self.attach_loaded_storage(prepared.storage, Ok((prepared.lock, prepared.store)), window, cx);
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
        let paths = (
            self.executable_input.read(cx).value().trim().to_string(),
            self.ego_input.read(cx).value().trim().to_string(),
        );
        self.startup.paths = paths.clone();
        self.startup.checks = None;
        let (tx, rx) = mpsc::channel();
        self.startup.pending = Some(rx);
        std::thread::spawn(move || {
            let ego =
                std::thread::spawn(move || crate::ego::probe(&paths.1, &std::env::temp_dir()));
            let open = probe_opencode(&paths.0);
            let _ = tx.send((
                open,
                ego.join()
                    .unwrap_or_else(|_| Err("Browser check stopped. Please retry.".into())),
            ));
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
                Err(mpsc::TryRecvError::Disconnected) => Some((
                    Err("Check stopped. Please retry.".into()),
                    Err("Check stopped. Please retry.".into()),
                )),
            });
        if let Some(checks) = result {
            self.startup.pending = None;
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
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Preparing your workspace"))
                    .child(div().id("workspace-upgrade-phase").text_sm().text_color(rgb(colors.muted)).child(phase)))
                .child(label("Your saved work stays here. Setup continues when this is finished.", cx))
                .into_any_element();
        }
        if self.startup.silent {
            return col().id("startup-checking").size_full()
                .bg(rgb(colors.canvas)).items_center().justify_center()
                .child(label("Starting…", cx)).into_any_element();
        }
        let dependency = |index: usize| {
            let current = if index == 0 {
                self.executable_input.read(cx).value()
            } else {
                self.ego_input.read(cx).value()
            };
            let checked = if index == 0 {
                &self.startup.paths.0
            } else {
                &self.startup.paths.1
            };
            if checked.is_empty() && self.startup.checks.is_none() && self.startup.pending.is_none()
            {
                return ("○ Not checked", colors.muted, String::new(), false);
            }
            if current.trim() != checked {
                return (
                    "↻ Check again",
                    colors.warning,
                    "Path changed. Run the check again.".into(),
                    false,
                );
            }
            match self
                .startup
                .checks
                .as_ref()
                .map(|(a, b)| if index == 0 { a } else { b })
            {
                Some(Ok(message)) => ("✓ Ready", colors.success, message.clone(), true),
                Some(Err(message)) => ("! Needs attention", colors.danger, message.clone(), false),
                None => ("◌ Checking…", colors.accent, String::new(), false),
            }
        };
        let open = dependency(0);
        let ego = dependency(1);
        let workspace = if self.workspace_setup.is_some() {
            ("◌ Preparing…", colors.accent)
        } else if !self.storage_error.is_empty() {
            ("! Needs attention", colors.danger)
        } else if self.workspace_required {
            ("○ Choose folder", colors.warning)
        } else {
            ("✓ Ready", colors.success)
        };
        let ready =
            usize::from(!self.workspace_required) + usize::from(open.3) + usize::from(ego.3);
        let badge = |text: &'static str, color: u32| {
            div()
                .flex_shrink_0()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(colors.tag_surface))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(color))
                .child(text)
        };
        let heading = |title: &'static str, state: &'static str, color: u32| {
            row()
                .justify_between()
                .gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                .child(badge(state, color))
        };
        let card = || {
            col()
                .gap_2()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(colors.line))
                .bg(rgb(colors.panel))
        };
        col().id("startup").size_full().overflow_y_scroll().bg(rgb(colors.canvas)).text_color(rgb(colors.text)).text_sm().items_center()
            .child(col().w_full().max_w(px(840.)).p_6().gap_3().flex_shrink_0()
                .child(row().justify_between()
                    .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child("Set up your app"))
                    .child(div().text_sm().text_color(rgb(colors.muted)).child(format!("{ready} / 3 ready"))))
                .child(label("Check these essentials to get started.", cx))
                .child(card()
                    .child(heading("1 · Workspace", workspace.0, workspace.1))
                    .child(label("Stores projects, reports and local data. Existing files are preserved.", cx))
                    .when(self.workspace_required, |view| view.child(row().gap_2()
                        .child(div().flex_1().min_w_0().child(Input::new(&self.directory_input).id("workspace-root")))
                        .child(button("browse-workspace", "Browse…").disabled(self.workspace_setup.is_some()).on_click(cx.listener(|this,_,_,cx| this.choose_workspace(cx))))
                        .child(button("setup-workspace", "Set workspace").disabled(self.workspace_setup.is_some()).on_click(cx.listener(|this,_,_,cx| this.setup_workspace(cx))))))
                    .when(!self.workspace_required, |view| view.child(single_line_text("startup-workspace-path", self.workspace_root.as_ref().map(|p| p.display().to_string()).unwrap_or_default())))
                    .when_some(self.startup.upgrade_backup.as_ref(), |view, backup| view.child(col().gap_1()
                        .child(label("Workspace data upgraded. The previous database was backed up.", cx))
                        .child(single_line_text("workspace-upgrade-backup", backup.display().to_string()))))
                    .when(!self.storage_error.is_empty(), |view| view.child(div().text_xs().text_color(rgb(colors.danger)).child(self.storage_error.clone())))
                    .when(self.workspace_required && self.workspace_root.is_some() && !self.storage_error.is_empty(), |view| view.child(
                        button("retry-workspace-upgrade", "Retry workspace preparation").on_click(cx.listener(|this,_,_,cx| {
                            if let Some(root) = &this.workspace_root { this.begin_workspace_open(crate::persistence::Storage::new(root.join("workspace/workspace.sqlite3")), cx); }
                        })))))
                .child(card()
                    .child(heading("2 · OpenCode", open.0, open.1))
                    .child(row().justify_between().gap_2().child(label("Runs your AI tasks.", cx))
                        .child(link_button("install-opencode", "Install ↗").on_click(|_,_,cx| cx.open_url("https://opencode.ai"))))
                    .child(Input::new(&self.executable_input).id("startup-opencode"))
                    .when(!open.2.is_empty(), |view| view.child(div().text_xs().text_color(rgb(if open.3 { colors.muted } else { open.1 })).child(open.2.clone()))))
                .child(card()
                    .child(heading("3 · ego(lite)", ego.0, ego.1))
                    .child(row().justify_between().gap_2().child(label("Browser access · finish its first launch after installing.", cx))
                        .child(row().gap_3()
                            .child(link_button("startup-install-ego", "Install ↗").on_click(|_,_,cx| cx.open_url("https://lite.ego.app")))
                            .child(link_button("startup-open-ego", "Open ↗").on_click(cx.listener(|this,_,_,cx| this.open_ego(cx))))))
                    .child(Input::new(&self.ego_input).id("startup-ego"))
                    .when(!ego.2.is_empty(), |view| view.child(div().text_xs().text_color(rgb(if ego.3 { colors.muted } else { ego.1 })).child(ego.2.clone())))
                    .when(!self.error.is_empty(), |view| view.child(div().text_xs().text_color(rgb(colors.danger)).child(self.error.clone()))))
                .child(row().justify_between().gap_3()
                    .child(div().flex_1().min_w_0().text_xs().text_color(rgb(colors.muted)).child("Next: configure your model and provider in Settings."))
                    .child(row().gap_2()
                        .child(button("startup-recheck", "Check again").disabled(self.workspace_required || self.startup.pending.is_some()).on_click(cx.listener(|this,_,_,cx| this.check_startup(cx))))
                        .child(button("startup-continue", "Continue").primary().disabled(!self.startup_ready(cx)).on_click(cx.listener(|this,_,_,cx| {
                            if !this.startup_ready(cx) { return; }
                            this.store.opencode_path = this.startup.paths.0.clone();
                            this.store.research_config.ego_path = this.startup.paths.1.clone();
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
    fn ready_for(&self, workspace_ready: bool, opencode: &str, ego: &str) -> bool {
        workspace_ready
            && self.workspace_pending.is_none()
            && self.pending.is_none()
            && self.paths.0 == opencode
            && self.paths.1 == ego
            && self
                .checks
                .as_ref()
                .is_some_and(|(a, b)| a.is_ok() && b.is_ok())
    }
}
#[cfg(test)]
mod tests {
    use super::{Startup, probe_opencode};
    use std::sync::mpsc;
    #[test]
    fn all_requirements_and_current_paths_must_pass() {
        let mut startup = Startup {
            paths: ("open".into(), "ego".into()),
            ..Default::default()
        };
        assert!(!startup.ready_for(true, "open", "ego"));
        startup.checks = Some((Ok("ready".into()), Err("missing".into())));
        assert!(!startup.ready_for(true, "open", "ego"));
        startup.checks = Some((Ok("ready".into()), Ok("ready".into())));
        assert!(!startup.ready_for(false, "open", "ego"));
        assert!(!startup.ready_for(true, "changed", "ego"));
        assert!(!startup.ready_for(true, "open", "changed"));
        assert!(startup.ready_for(true, "open", "ego"));
        let (_, rx) = mpsc::channel();
        startup.pending = Some(rx);
        assert!(!startup.ready_for(true, "open", "ego"));
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
        assert!(probe_opencode(cli.to_str().unwrap()).is_err());
        for (script, valid) in [("exit 0", false), ("exit 1", false), ("echo 1.2.3", true)] {
            std::fs::write(&cli, format!("#!/bin/sh\n{script}\n")).unwrap();
            std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
            assert_eq!(probe_opencode(cli.to_str().unwrap()).is_ok(), valid);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
