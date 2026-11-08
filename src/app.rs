use crate::domain::Task;
use crate::domain::*;
use crate::{opencode, persistence::Storage};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

pub struct ActiveRun {
    pub id: u64,
    handle: opencode::Running,
    had_error: bool,
}
pub enum AttachmentImport {
    Progress(String),
    Complete(u64, Vec<Result<crate::attachments::Attachment, String>>),
}
type Discovery = Result<(String, Vec<String>), String>;

#[derive(Clone, Debug, PartialEq)]
pub enum Scope {
    All,
    Project(u64),
    Topic(u64),
    Cycle(String),
    Archive,
    Runs,
    Recovery,
    Research,
    Automations,
    AutomationRuns,
    Settings,
}
#[derive(Clone, Copy, PartialEq)]
pub enum Layout {
    List,
    Board,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AllStatus {
    All,
    Open,
    Stage(Stage),
    Execution(crate::workflow::ExecutionGroup),
}
impl AllStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All statuses",
            Self::Open => "Open",
            Self::Stage(stage) => crate::workflow::phase_label(stage),
            Self::Execution(group) => group.label(),
        }
    }
    pub fn matches(self, status: &crate::workflow::TaskStatus) -> bool {
        let stage = status.phase;
        match self {
            Self::All => true,
            Self::Open => !stage.is_closed(),
            Self::Stage(value) => stage == value,
            Self::Execution(group) => status.group == Some(group),
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
pub enum Property {
    Stage,
    Priority,
    Project,
    Cycle,
    Bucket,
    Parent,
}
#[derive(Clone)]
pub struct DragTask {
    pub id: u64,
    pub title: String,
}
impl Render for DragTask {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(280.))
            .p_4()
            .rounded_md()
            .bg(rgb(crate::appearance::palette(cx).panel))
            .text_color(rgb(crate::appearance::palette(cx).text))
            .child(self.title.clone())
    }
}
pub struct Draft {
    pub id: u64,
    pub project: u64,
    pub stage: Stage,
    pub cycle: Option<String>,
    pub title: Entity<InputState>,
    pub description: Entity<TextareaState>,
    pub labels: Entity<InputState>,
    pub due: Entity<InputState>,
}
pub struct TaskApp {
    pub store: Store,
    pub scope: Scope,
    pub layout: Layout,
    pub all_status: AllStatus,
    pub sidebar_scroll: ScrollHandle,
    pub sidebar_reveal: Option<u64>,
    pub sidebar_selected_row: ScrollHandle,
    pub detail_scroll: ScrollHandle,
    pub result_scroll: ScrollHandle,
    pub selected: Option<u64>,
    pub task_result_context: Option<u64>,
    pub tabs: Vec<u64>,
    pub expanded_sidebar_projects: Vec<u64>,
    pub collapsed_sidebar_goals: Vec<u64>,
    pub sidebar_split: Entity<gpui_kit::base::ResizableState>,
    pub results_split: Entity<gpui_kit::base::ResizableState>,
    pub search: Entity<InputState>,
    pub query: String,
    pub draft: Option<Draft>,
    pub property: Option<Property>,
    pub editing: bool,
    pub comment: Entity<TextareaState>,
    pub report_regeneration: Option<(u64, Entity<TextareaState>)>,
    pub toast: String,
    pub error: String,
    pub task_start_feedback: Option<(u64, String)>,
    pub save_feedback: Option<Result<String, String>>,
    pub show_display: bool,
    pub show_done: bool,
    pub compact: bool,
    pub collapsed: Vec<u64>,
    pub undo: Vec<Store>,
    pub focus: FocusHandle,
    pub modal_focus: FocusHandle,
    pub new_project: Option<Entity<InputState>>,
    pub(crate) topic_move: Option<crate::topic_views::TopicMoveDialog>,
    pub(crate) project_rename: Option<(u64, Entity<InputState>)>,
    pub topic_editor: Option<(u64, Option<u64>, Entity<InputState>)>,
    pub collapsed_sidebar_topics: Vec<u64>,
    pub research_topic: Option<u64>,
    pub planning_project: Option<u64>,
    pub planning_topic: Option<u64>,
    pub planning_replan_source: Option<u64>,
    pub project_brief: Entity<TextareaState>,
    pub storage: Option<Storage>,
    pub startup: crate::startup::Startup,
    pub workspace_required: bool,
    pub workspace_choice: Option<PathBuf>,
    pub workspace_root: Option<PathBuf>,
    pub workspace_setup: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    pub storage_error: String,
    last_saved: Store,
    last_save: Instant,
    _storage_lock: Option<std::fs::File>,
    pub executable_input: Entity<InputState>,
    pub codex_executable_input: Entity<InputState>,
    pub directory_input: Entity<InputState>,
    pub directory_project: Option<u64>,
    pub connection_project: u64,
    pub connection_status: String,
    pub opencode_version: Option<(String, String)>,
    pub available_models: Vec<String>,
    pub codex_models: Vec<String>,
    pub codex_status: String,
    pub codex_version: Option<String>,
    pub codex_discovery: Option<mpsc::Receiver<Result<crate::codex::Discovery, String>>>,
    pub settings_scroll: ScrollHandle,
    pub settings_tab: crate::settings_views::SettingsTab,
    pub global_model: Entity<InputState>,
    pub model_profile_name: Entity<InputState>,
    pub creating_model_profile: bool,
    pub publishing_inputs: [Entity<InputState>; 3],
    pub publishing_scan_requested: bool,
    pub publishing_job: Option<(String, mpsc::Receiver<crate::report_publish::PublishEvent>)>,
    discovery: Option<mpsc::Receiver<(PathBuf, Discovery)>>,
    pub active_runs: Vec<ActiveRun>,
    pub queued_runs: Vec<(u64, Vec<PathBuf>)>,
    pub selected_run: Option<u64>,
    pub expanded_run_sources: Option<u64>,
    pub show_planning_activity: bool,
    pub log_scroll: ScrollHandle,
    pub follow_logs: bool,
    pumping: bool,
    pub research_title: Entity<InputState>,
    pub research_brief: Entity<TextareaState>,
    pub research_project: u64,
    pub research_models: [Entity<InputState>; 4],
    pub ego_input: Entity<InputState>,
    pub ego_status: String,
    pub ego_probe: Option<mpsc::Receiver<Result<String, String>>>,
    pub research_model_picker: Option<EntityId>,
    pub research_model_initial: SharedString,
    pub research_sequences: std::collections::BTreeSet<u64>,
    pub todo_batch: Option<crate::batch::TodoBatch>,
    pub(crate) recovery: crate::recovery::Recovery,
    pub sidebar_logo: Entity<crate::brand::SidebarLogo>,
    pub materials_focus: FocusHandle,
    pub attachment_status: String,
    pub attachment_started: Option<Instant>,
    pub attachment_import: Option<mpsc::Receiver<AttachmentImport>>,
    pub show_run_result: bool,
    pub show_run_details: bool,
    pub show_task_details: bool,
    pub expanded_request_brief: Option<u64>,
    pub goal_steps_tab: Option<u64>,
    pub steps_layout: Layout,
    pub render_statuses: std::cell::RefCell<std::collections::HashMap<u64, crate::workflow::TaskStatus>>,
    pub board_execution_filter: Option<crate::workflow::ExecutionGroup>,
    pub show_task_activity: bool,
    pub automation_ui: crate::automation_views::AutomationUi,
    pub automation_generation: u64,
    pub automation_timer_armed: bool,
    pub automation_fake_mode: bool,
    pub automation_fake_hold: bool,
    pub automation_backoff_until: Option<Instant>,
    pub automation_started: std::collections::HashMap<u64, Instant>,
    pub automation_fake: Option<crate::automation_runtime::FakeAutomationRunner>,
    pub _subscriptions: Vec<Subscription>,
}
impl TaskApp {
    pub fn discovering_models(&self) -> bool {
        self.discovery.is_some()
    }
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut app = Self::with_store(Store::empty(), window, cx);
        app.workspace_required = true;
        match crate::workspace::configured() {
            Ok(Some(root)) => {
                app.workspace_root = Some(root.clone());
                app.begin_workspace_open(Storage::new(root.join("workspace/workspace.sqlite3")), cx);
            }
            Ok(None) => {}
            Err(e) => app.storage_error = e,
        }
        for input in [&app.executable_input, &app.codex_executable_input, &app.ego_input] {
            app._subscriptions.push(cx.subscribe(input, |_, _, event, cx| {
                if matches!(event, InputEvent::Change) { cx.notify(); }
            }));
        }
        app.startup.required = true;
        app.startup.silent = !app.workspace_required;
        // Dependency probes begin only after the workspace has been upgraded and loaded.
        app
    }
    pub fn choose_workspace(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose workspace".into()),
        });
        cx.spawn(async move |this, cx| match paths.await {
            Ok(Ok(Some(paths))) => {
                let _ = this.update(cx, |this, cx| {
                    this.workspace_choice = paths.into_iter().next();
                    cx.notify();
                });
            }
            Ok(Ok(None)) => {}
            other => {
                let _ = this.update(cx, |this, cx| {
                    this.storage_error = format!("Cannot select workspace: {other:?}");
                    cx.notify();
                });
            }
        })
        .detach();
    }
    pub fn setup_workspace(&mut self, cx: &mut Context<Self>) {
        if self.workspace_setup.is_some() {
            return;
        }
        let root = PathBuf::from(self.directory_input.read(cx).value().trim());
        self.storage_error.clear();
        let (tx, rx) = mpsc::channel();
        self.workspace_setup = Some(rx);
        std::thread::spawn(move || {
            let result = (|| {
                let legacy = crate::workspace::legacy_directory()?;
                crate::workspace::initialize(&root, legacy.as_deref())?;
                let root = root.canonicalize().map_err(|e| e.to_string())?;
                crate::workspace::save_location(&root)?;
                Ok(root)
            })();
            let _ = tx.send(result);
        });
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let pending = this
                    .update(cx, |this, cx| {
                        cx.notify();
                        this.workspace_setup.is_some()
                    })
                    .unwrap_or(false);
                if !pending {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }
    pub fn managed_project_directory(&self, id: u64) -> Result<String, String> {
        match &self.workspace_root {
            Some(root) => crate::workspace::project_directory(&root.join("workspace"), id),
            None if self.workspace_required => {
                Err("Set up a workspace before creating projects.".into())
            }
            None => Ok(String::new()), // Isolated QA stores configure their own directories.
        }
    }
    /// The generated automation project keeps every automation result out of the
    /// user's projects. It is created on demand and never shown in navigation.
    pub fn ensure_automation_project(&mut self) -> Result<u64, String> {
        let directory = match &self.workspace_root {
            Some(root) => crate::workspace::automation_directory(&root.join("workspace"))?,
            None if self.workspace_required => {
                return Err("Set up a workspace before creating automations.".into());
            }
            // Isolated QA stores have no workspace root; they still need a real,
            // writable folder because every run validates its working directory.
            None => {
                let fallback =
                    std::env::temp_dir().join("what-the-hell-are-you-planning-automations");
                std::fs::create_dir_all(&fallback).map_err(|e| e.to_string())?;
                fallback.to_string_lossy().into_owned()
            }
        };
        self.store.ensure_automation_project(directory)
    }
    fn check_workspace_directory(&self, path: &std::path::Path) -> Result<(), String> {
        if self.workspace_required {
            return Err("Set up your workspace before running tasks.".into());
        }
        if let Some(root) = &self.workspace_root {
            let workspace = root.join("workspace");
            let projects = workspace.join("projects");
            let automations = workspace.join("automations");
            if crate::workspace::check_directory(&projects, path).is_err()
                && crate::workspace::check_directory(&automations, path).is_err()
            {
                // Report the project rule; both folders are inside the workspace.
                crate::workspace::check_directory(&projects, path)?;
            }
        }
        Ok(())
    }
    #[cfg(feature = "qa")]
    pub fn new_mock(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_store(Store::mock(), window, cx)
    }
    fn with_store(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search tasks…"));
        let sub = cx.subscribe(&search, |this, state, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.query = state.read(cx).value().to_string();
                cx.notify();
            }
        });
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let autosave = cx.observe_self(|this, cx| {
            if this.persist_if_changed(false) {
                cx.notify();
            }
        });
        let quit = cx.on_app_quit(|this, _| {
            this.shutdown();
            async {}
        });
        let release = cx.on_release(|this, _| this.shutdown());
        let global_model = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(store.default_model.clone())
                .placeholder("provider/model")
        });
        let research_models: [_; 4] = std::array::from_fn(|_| {
            cx.new(|cx| InputState::new(window, cx).placeholder("Global default"))
        });
        let automation_ui = crate::automation_views::AutomationUi::new(window, cx);
        let system_appearance = cx.observe_window_appearance(window, |this, w, cx| {
            if this.store.appearance == crate::appearance::Appearance::System {
                crate::appearance::apply(this.store.appearance, Some(w), cx);
                cx.notify();
            }
        });
        let mut subscriptions = vec![
            sub,
            autosave,
            quit,
            release,
            system_appearance,
        ];
        for input in [
            &automation_ui.name,
            &automation_ui.interval,
            &automation_ui.hour,
            &automation_ui.minute,
            &automation_ui.timezone,
            &automation_ui.delay,
            &automation_ui.timeout,
            &automation_ui.failures,
            &automation_ui.retention,
        ] {
            subscriptions.push(cx.subscribe(input, |_, _, event, cx| {
                if matches!(event, InputEvent::Change) { cx.notify(); }
            }));
        }
        for input in [&automation_ui.objective, &automation_ui.expected] {
            subscriptions.push(cx.subscribe(input, |_, _, event, cx| {
                if matches!(event, InputEvent::Change) { cx.notify(); }
            }));
        }
        for input in research_models.iter().chain([&global_model]) {
            subscriptions.push(cx.subscribe_in(input, window, move |this, input, event, w, cx| {
                match event {
                    InputEvent::Focus if this.scope == Scope::Settings && this.settings_tab == crate::settings_views::SettingsTab::Models => this.open_research_model_picker(&input, cx),
                    InputEvent::PressEnter { .. } => this.finish_model_edit(input, true, w, cx),
                    InputEvent::Blur => this.finish_model_search(input, w, cx),
                    _ => {}
                }
                cx.notify();
            }));
        }
        let ego_input = cx.new(|cx| InputState::new(window, cx).default_value(crate::research::default_ego_path()));
        subscriptions.push(cx.subscribe(&ego_input, |this, input, event, cx| {
            if matches!(event, InputEvent::Change) && this.scope == Scope::Settings
                && this.settings_tab == crate::settings_views::SettingsTab::Models && !this.startup.required {
                let value = input.read(cx).value();
                if value.trim() != this.store.research_config.ego_path {
                    this.store.research_config.ego_path = value.trim().to_owned();
                    this.ego_status = "Runtime not checked".into();
                    cx.notify();
                }
            }
        }));
        Self {
            last_saved: store.clone(),
            storage: None,
            startup: Default::default(),
            workspace_required: false,
            workspace_choice: None,
            workspace_root: None,
            workspace_setup: None,
            storage_error: String::new(),
            last_save: Instant::now(),
            _storage_lock: None,
            executable_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(opencode::detect_executable())
                    .placeholder("Path to opencode")
            }),
            codex_executable_input: cx.new(|cx| InputState::new(window, cx)
                .default_value(crate::codex::detect_executable()).placeholder("Path to codex")),
            directory_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("/absolute/path/to/project")),
            directory_project: None,
            connection_project: 1,
            connection_status: "Model catalog has not been loaded.".into(),
            opencode_version: None,
            codex_models: vec![],
            codex_status: "Not checked".into(),
            codex_version: None,
            codex_discovery: None,
            available_models: vec![],
            settings_scroll: ScrollHandle::new(),
            settings_tab: Default::default(),
            publishing_inputs: std::array::from_fn(|_| cx.new(|cx| InputState::new(window, cx))),
            publishing_job: None,
            publishing_scan_requested: true,
            global_model,
            creating_model_profile: false,
            model_profile_name: cx.new(|cx| InputState::new(window, cx).placeholder("Profile name")),
            discovery: None,
            active_runs: vec![],
            queued_runs: vec![],
            selected_run: None,
            expanded_run_sources: None,
            show_planning_activity: false,
            log_scroll: ScrollHandle::new(),
            follow_logs: true,
            pumping: false,
            store,
            scope: Scope::Project(1),
            layout: Layout::List,
            all_status: AllStatus::All,
            sidebar_scroll: ScrollHandle::new(),
            sidebar_reveal: None,
            sidebar_selected_row: ScrollHandle::new(),
            detail_scroll: ScrollHandle::new(),
            result_scroll: ScrollHandle::new(),
            selected: None,
            task_result_context: None,
            tabs: vec![],
            expanded_sidebar_projects: vec![],
            collapsed_sidebar_goals: vec![],
            sidebar_split: cx.new(|_| gpui_kit::base::ResizableState::default()),
            results_split: cx.new(|_| gpui_kit::base::ResizableState::default()),
            search,
            query: String::new(),
            draft: None,
            property: None,
            editing: false,
            comment: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Leave a comment…")
                    .rows(3)
            }),
            report_regeneration: None,
            toast: "All changes saved".into(),
            error: String::new(),
            task_start_feedback: None,
            save_feedback: None,
            show_display: false,
            show_done: true,
            compact: false,
            collapsed: vec![],
            undo: vec![],
            focus,
            modal_focus: cx.focus_handle(),
            new_project: None,
            topic_editor: None,
            project_rename: None,
            topic_move: None,
            collapsed_sidebar_topics: vec![],
            research_topic: None,
            planning_project: None,
            planning_topic: None,
            planning_replan_source: None,
            project_brief: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(5)
                    .placeholder("Required: what do you want to research? One sentence is enough…")
            }),
            research_title: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("e.g. 本周美股科技板块有哪些值得关注的变化？")
            }),
            research_brief: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("市场／股票代码、时间范围和研究问题…")
                    .rows(6)
            }),
            research_project: 1,
            research_models,
            ego_input,
            ego_status: "Runtime not checked".into(),
            ego_probe: None,
            research_model_picker: None,
            research_model_initial: SharedString::default(),
            research_sequences: Default::default(),
            todo_batch: None,
            recovery: Default::default(),
            sidebar_logo: cx.new(|_| crate::brand::SidebarLogo),
            materials_focus: cx.focus_handle(),
            attachment_status: String::new(),
            attachment_started: None,
            attachment_import: None,
            show_run_result: true,
            show_run_details: false,
            show_task_details: false,
            expanded_request_brief: None,
            goal_steps_tab: None,
            steps_layout: Layout::Board,
            render_statuses: Default::default(),
            board_execution_filter: None,
            show_task_activity: false,
            automation_ui,
            automation_generation: 0,
            automation_timer_armed: false,
            automation_fake_mode: false,
            automation_fake_hold: false,
            automation_backoff_until: None,
            automation_started: Default::default(),
            automation_fake: None,
            _subscriptions: subscriptions,
        }
    }
    pub fn snapshot(&mut self) {
        self.undo.push(self.store.clone());
        if self.undo.len() > 30 {
            self.undo.remove(0);
        }
    }
    pub fn show_save_error(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.error = message.into();
        self.save_feedback = Some(Err(self.error.clone()));
        cx.notify();
    }
    pub fn confirm_save(&mut self, message: &str, cx: &mut Context<Self>) {
        self.persist_if_changed(true);
        if !self.storage_error.is_empty() {
            self.show_save_error(self.storage_error.clone(), cx);
        } else if self.storage.is_none() {
            self.show_save_error(
                "Changes are only in memory. Workspace storage is unavailable.",
                cx,
            );
        } else {
            self.error.clear();
            self.toast = message.into();
            self.save_feedback = Some(Ok(message.into()));
            cx.notify();
        }
    }
    pub fn finish(&mut self, result: Result<(), String>, message: &str, cx: &mut Context<Self>) {
        if self
            .undo
            .last()
            .is_some_and(|previous| previous == &self.store)
        {
            self.undo.pop();
        }
        match result {
            Ok(()) => {
                self.toast = message.into();
                self.error.clear();
            }
            Err(e) => {
                self.error = e;
            }
        }
        cx.notify();
    }
    pub fn navigate(&mut self, scope: Scope, window: &mut Window, cx: &mut Context<Self>) {
        self.task_result_context = None;
        self.clear_navigation_overlays();
        if scope == Scope::Runs {
            // A period step is not part of Research results; do not keep it as
            // the selected run when the user enters the ordinary results view.
            if let Some(id) = self.selected_run
                && self
                    .store
                    .runs
                    .iter()
                    .find(|run| run.id == id)
                    .is_some_and(|run| run.automation_occurrence_id.is_some())
            {
                self.selected_run = None;
            }
            self.reveal_selected_run();
        }
        self.save_feedback = None;
        self.show_run_details = false;
        self.show_run_result = true;
        self.research_model_picker = None;
        let project = match &scope {
            Scope::Project(id) => Some(*id),
            Scope::Topic(id) => self.store.topics.iter().find(|t| t.id == *id).map(|t| t.project_id),
            _ => None,
        };
        if let Some(project) = project {
            self.remember_active_project(project);
        }
        if scope == Scope::Automations {
            self.automation_ui.selected = None;
            self.automation_ui.creating = false;
            self.automation_ui.feedback = None;
        }
        self.scope = scope;
        self.selected = None;
        self.editing = false;
        self.draft = None;
        self.property = None;
        self.error.clear();
        self.query.clear();
        self.search.update(cx, |s, cx| s.set_value("", window, cx));
        if matches!(self.scope, Scope::Topic(_)) {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }
    pub fn open_collection_project(
        &mut self,
        project: u64,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate(Scope::Project(project), w, cx);
    }
    pub(crate) fn sidebar_scope(&self) -> Scope {
        self.selected
            .or(self.task_result_context)
            .and_then(|id| self.store.task(id))
            .map(|task| Scope::Project(task.project_id))
            .unwrap_or_else(|| self.scope.clone())
    }
    pub(crate) fn remember_active_project(&mut self, project: u64) {
        if self.store.projects.iter().any(|p| p.id == project) {
            self.store.last_active_project = Some(project);
        }
    }
    pub(crate) fn restore_sidebar(&mut self) {
        self.expanded_sidebar_projects = self.store.last_active_project
            .filter(|id| self.store.projects.iter().any(|p| p.id == *id))
            .into_iter().collect();
        self.collapsed_sidebar_topics = self.store.topics.iter().map(|t| t.id).collect();
        self.collapsed_sidebar_goals = self.store.tasks.iter().map(|t| t.id).collect();
    }
    pub(crate) fn reveal_task_project(&mut self, id: u64) {
        if let Some(task) = self.store.task(id) {
            self.remember_active_project(task.project_id);
        }
        if let Some(topic) = self.store.task_topic(id) {
            self.collapsed_sidebar_topics.retain(|t| *t != topic.id);
        }
        if let Some(research) = self.store.task(id).and_then(|t| t.research.as_ref()) {
            self.collapsed_sidebar_goals
                .retain(|goal| *goal != research.goal_id);
        }
        let mut ancestor = self.store.task(id).and_then(|task| task.parent_id);
        let mut visited = std::collections::BTreeSet::new();
        while let Some(parent) = ancestor {
            if !visited.insert(parent) {
                break;
            }
            self.collapsed_sidebar_goals.retain(|goal| *goal != parent);
            ancestor = self.store.task(parent).and_then(|task| task.parent_id);
        }
        if let Some(task) = self.store.task(id)
            && !self.expanded_sidebar_projects.contains(&task.project_id)
        {
            self.expanded_sidebar_projects.push(task.project_id);
        }
    }
    pub fn open_task(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.task(id).is_none() {
            return;
        }
        if self.selected != Some(id) && self.task_result_context != Some(id) {
            self.detail_scroll.set_offset(point(px(0.), px(0.)));
        }
        self.task_result_context = None;
        self.clear_navigation_overlays();
        self.tabs.retain(|task| *task != id);
        self.tabs.push(id);
        self.show_task_details = false;
        self.expanded_request_brief = None;
        self.show_task_activity = false;
        self.selected = Some(id);
        self.reveal_task_project(id);
        self.sidebar_reveal = (!self.store.task_is_archived(id)).then_some(id);
        self.editing = false;
        self.draft = None;
        self.property = None;
        self.error.clear();
        self.comment.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }
    pub fn title(&self) -> String {
        self.scope_title(&self.scope)
    }
    pub(crate) fn scope_title(&self, scope: &Scope) -> String {
        match scope {
            Scope::All => "All research".into(),
            Scope::Project(id) => self
                .store
                .projects
                .iter()
                .find(|p| p.id == *id)
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            Scope::Topic(id) => self
                .store
                .topics
                .iter()
                .find(|t| t.id == *id)
                .map(|t| t.display_name())
                .unwrap_or_default(),
            Scope::Cycle(c) => c.clone(),
            Scope::Archive => "Archive".into(),
            Scope::Runs => "Research results".into(),
            Scope::Recovery => "Paused projects".into(),
            Scope::Research => "Market research".into(),
            Scope::Automations => "Automations".into(),
            Scope::AutomationRuns => "Automation results".into(),
            Scope::Settings => "Settings".into(),
        }
    }
    pub fn matches_scope(&self, t: &Task) -> bool {
        // Automation research is reached from its own history, never the normal board.
        if t.automation_id.is_some() {
            return false;
        }
        if self.store.task_is_archived(t.id) != (self.scope == Scope::Archive) {
            return false;
        }
        match &self.scope {
            Scope::All => self.all_status.matches(&self.store.task_status(t.id, self.todo_batch.as_ref())),
            Scope::Project(id) => t.project_id == *id,
            Scope::Topic(id) => self.store.task_topic(t.id).is_some_and(|topic| topic.id == *id),
            Scope::Cycle(c) => t.cycle.as_ref() == Some(c),
            _ => true,
        }
    }
    pub fn visible_tasks(&self) -> Vec<Task> {
        let mut list: Vec<_> = self
            .store
            .tasks
            .iter()
            .filter(|t| {
                t.parent_id.is_none()
                    && self.matches_scope(t)
                    && self.store.matches_query(t, &self.query)
                    && (self.scope == Scope::All || self.show_done || !self.store.task_status(t.id, self.todo_batch.as_ref()).phase.is_closed())
            })
            .cloned()
            .collect();
        list.sort_by(|a, b| self.store.compare_display_tasks(a, b));
        list
    }
    pub fn change_stage(&mut self, id: u64, stage: Stage, cx: &mut Context<Self>) {
        if let Err(error) = self.store.validate_manual_transition(id, stage, self.todo_batch.as_ref()) {
            self.error = error;
            cx.notify();
            return;
        }
        if stage == Stage::InProgress {
            // Submit through the existing scheduler; never manufacture Running.
            let scope = self.scope.clone();
            let selected = self.selected;
            self.start_goal_batch(id, cx);
            self.scope = scope;
            self.selected = selected;
            if self.error.is_empty() && self.store.task_status(id, self.todo_batch.as_ref()).phase == Stage::InProgress {
                self.undo.clear();
                self.toast = format!("Submitted · {}", self.store.task_status(id, self.todo_batch.as_ref()).label());
            }
            cx.notify();
            return;
        }
        if self.store.task(id).is_some_and(|t| t.stage == stage) { return; }
        self.snapshot();
        let result = self.store.set_stage(id, stage);
        self.finish(result, "Task stage updated", cx);
    }
    pub fn begin_draft(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task) = self.store.task(id) else { return; };
        let title = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("What needs to be done?")
                .default_value(task.title.clone())
        });
        self.draft = Some(Draft {
            id,
            cycle: task.cycle.clone(),
            project: task.project_id,
            stage: task.stage,
            title: title.clone(),
            description: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Describe the outcome, context, and acceptance criteria…")
                    .default_value(task.description.clone())
                    .rows(6)
            }),
            labels: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Feature, UI")
                    .default_value(task.labels.join(", "))
            }),
            due: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("YYYY-MM-DD")
                    .default_value(task.due_date.clone())
            }),
        });
        self.editing = true;
        self.error.clear();
        title.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }
    pub fn save_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = &self.draft else {
            return;
        };
        let title = d.title.read(cx).value().to_string();
        let desc = d.description.read(cx).value().to_string();
        let due = d.due.read(cx).value().to_string();
        let labels: Vec<String> = d
            .labels
            .read(cx)
            .value()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if title.trim().is_empty() {
            self.show_save_error("Add a task title before saving.", cx);
            return;
        }
        if !due.is_empty() && !valid_date(&due) {
            self.show_save_error("Use a valid date in YYYY-MM-DD format.", cx);
            return;
        }
        let (id, project, stage, cycle) = (d.id, d.project, d.stage, d.cycle.clone());
        if self.store.task(id).is_some_and(|t| t.project_id != project) {
            self.show_save_error("Move the entire topic using its Project property or topic menu, then edit task details.", cx);
            return;
        }
        if self.store.task(id).is_some_and(|t| t.stage != stage) {
            if stage == Stage::InProgress {
                self.show_save_error("Save task details first, then submit execution from Board or the status menu.", cx);
                return;
            }
            if let Err(error) = self.store.validate_manual_transition(id, stage, self.todo_batch.as_ref()) {
                self.show_save_error(&error, cx);
                return;
            }
        }
        self.snapshot();
        if let Err(e) = self
            .store
            .move_project(id, project)
            .and_then(|_| self.store.set_stage(id, stage))
        {
            self.store = self.undo.pop().unwrap();
            self.show_save_error(e, cx);
            return;
        }
        if let Some(t) = self.store.task_mut(id) {
            t.cycle = cycle;
            t.title = title.trim().into();
            t.description = desc;
            t.labels = labels;
            t.due_date = due;
            t.activity
                .push(crate::domain::activity_event("Task details updated"));
        }
        self.editing = false;
        self.draft = None;
        self.open_task(id, window, cx);
        self.confirm_save("Task saved to local workspace", cx);
        cx.notify();
    }
    pub fn undo_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.active_runs.is_empty() || !self.queued_runs.is_empty()
            || self.todo_batch.as_ref().is_some_and(|b| b.items.iter().any(|id| b.contains_pending(*id))) {
            self.error =
                "Stop or finish submitted work before undoing changes.".into();
            cx.notify();
            return;
        }
        if let Some(mut store) = self.undo.pop() {
            store.last_active_project = self.store.last_active_project;
            store.runs = self.store.runs.clone();
            store.publications = self.store.publications.clone();
            store.report_publishing = self.store.report_publishing.clone();
            store.next_id = store.next_id.max(self.store.next_id);
            self.store = store;
            crate::appearance::apply(self.store.appearance, Some(window), cx);
            self.tabs.retain(|id| self.store.task(*id).is_some());
            if self
                .selected
                .is_some_and(|id| self.store.task(id).is_none())
            {
                self.selected = None;
            }
            if let Scope::Project(id) = self.scope
                && !self.store.projects.iter().any(|p| p.id == id)
            {
                self.scope = Scope::All;
            }
            if let Scope::Topic(id) = self.scope {
                if !self.store.topics.iter().any(|t| t.id == id) {
                    self.scope = Scope::All;
                }
            }
            self.draft = None;
            self.editing = false;
            self.property = None;
            self.refresh_research_inputs(window, cx);
            self.refresh_global_model_input(window, cx);
            self.toast = "Last change undone".into();
            self.error.clear();
            cx.notify();
        }
    }
    pub fn archive_task(&mut self, id: u64, w: &mut Window, cx: &mut Context<Self>) {
        self.request_archive(crate::archive_app::ArchiveTarget::Task(id), w, cx);
    }
    #[cfg(feature = "qa")]
    pub fn attach_storage(
        &mut self,
        storage: Storage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let loaded = storage
            .acquire_lock()
            .and_then(|lock| {
                storage.upgrade_paths(|_| {})?;
                storage.load().map(|store| (lock, store))
            });
        self.attach_loaded_storage(storage, loaded, window, cx);
        if self.storage_error.is_empty() { self.check_interrupted_sessions(cx); }
    }
    pub(crate) fn attach_loaded_storage(
        &mut self,
        storage: Storage,
        loaded: Result<(std::fs::File, Option<Store>), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match loaded {
            Ok((lock, store)) => {
                self._storage_lock = Some(lock);
                self.store = store.unwrap_or_else(Store::empty);
                self.restore_sidebar();
                self.store.recover_interrupted_runs();
                self.recover_publications();
                self.store.reconcile_goal_approvals();
                let project = self
                    .store
                    .visible_projects()
                    .map(|p| p.id)
                    .next()
                    .unwrap_or(0);
                self.connection_project = project;
                self.research_project = project;
                self.scope = Scope::All;
                crate::appearance::apply(self.store.appearance, Some(window), cx);
                if self.store.opencode_path.is_empty() {
                    self.store.opencode_path = opencode::detect_executable();
                }
                let path = self.store.opencode_path.clone();
                self.executable_input
                    .update(cx, |s, cx| s.set_value(path, window, cx));
                let codex_path = self.store.research_config.codex_path.clone();
                self.codex_executable_input.update(cx, |s, cx| s.set_value(codex_path, window, cx));
                self.refresh_research_inputs(window, cx);
                self.refresh_global_model_input(window, cx);
                match storage.save(&self.store) {
                    Ok(()) => {
                        self.last_saved = self.store.clone();
                        self.toast = "All changes saved".into();
                    }
                    Err(e) => {
                        self.storage_error = e;
                        self.storage = None;
                        self._storage_lock = None;
                        return;
                    }
                }
                self.storage = Some(storage);
                self.recover_automations(cx);
            }
            Err(e) => {
                self.storage_error = e;
                self.toast = "Storage unavailable · changes are not saved".into();
            }
        }
    }
    // Every notified domain mutation is saved. Streaming runs are coalesced to two writes/second.
    pub(crate) fn persist_if_changed(&mut self, force: bool) -> bool {
        self.store.reconcile_goal_approvals();
        let Some(storage) = &self.storage else {
            return false;
        };
        if self.store == self.last_saved {
            return false;
        }
        if !force
            && !self.active_runs.is_empty()
            && self.last_save.elapsed() < Duration::from_millis(500)
        {
            return false;
        }
        self.last_save = Instant::now();
        match storage.save(&self.store) {
            Ok(()) => {
                self.last_saved = self.store.clone();
                let had_error = !self.storage_error.is_empty();
                self.storage_error.clear();
                if had_error {
                    self.toast = "Saved · local storage recovered".into();
                }
                had_error
            }
            Err(e) => {
                let changed = self.storage_error != e;
                self.storage_error = e;
                changed
            }
        }
    }
    /// True while the in-memory workspace differs from the last confirmed write.
    pub fn has_unsaved_changes(&self) -> bool {
        self.store != self.last_saved
    }
    /// Explicit retry for the footer's "Not saved" state. A plain save is not
    /// enough because the in-memory store already equals the last confirmed
    /// snapshot, so `persist_if_changed` would skip the write.
    pub fn retry_save(&mut self, cx: &mut Context<Self>) {
        let result = self
            .storage
            .as_ref()
            .map(|storage| storage.save(&self.store));
        match result {
            Some(Ok(())) => {
                self.last_saved = self.store.clone();
                self.storage_error.clear();
                self.error.clear();
                self.toast = "Saved to local workspace".into();
            }
            Some(Err(error)) => {
                self.storage_error = error;
            }
            None => {
                self.storage_error = "Workspace storage is unavailable. Reopen the workspace to reconnect.".into();
            }
        }
        cx.notify();
    }
    pub fn task_running(&self, id: u64) -> bool {
        self.store.runs.iter().any(|r| {
            r.project_plan.is_none()
                && r.task_id == id
                && matches!(
                    r.status.as_str(),
                    "Queued" | "Running" | "Cancelling" | "Recovering"
                )
        })
    }
    pub fn goal_busy(&self, goal: u64) -> bool {
        self.store.tasks.iter().any(|t| {
            t.research.as_ref().is_some_and(|r| r.goal_id == goal) && self.task_running(t.id)
        })
    }
    pub fn run_goal(&self, run_id: u64) -> Option<u64> {
        self.store
            .runs
            .iter()
            .find(|r| r.id == run_id)
            .and_then(|r| self.store.task(r.task_id))
            .and_then(|t| t.research.as_ref())
            .map(|r| r.goal_id)
    }
    pub fn open_directory(&mut self, project: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.directory_project = Some(project);
        self.error.clear();
        let path = self
            .store
            .projects
            .iter()
            .find(|p| p.id == project)
            .map(|p| p.directory.clone())
            .unwrap_or_default();
        self.directory_input.update(cx, |s, cx| {
            s.set_value(path, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }
    pub fn save_directory(&mut self, cx: &mut Context<Self>) {
        let path = PathBuf::from(self.directory_input.read(cx).value().trim());
        if !path.is_absolute() || !path.is_dir() {
            self.show_save_error("Enter an existing absolute folder path.", cx);
            return;
        }
        let Ok(path) = path.canonicalize() else {
            return;
        };
        if let Err(error) = self.check_workspace_directory(&path) {
            self.show_save_error(&error, cx);
            return;
        }
        self.snapshot();
        if let Some(project) = self
            .store
            .projects
            .iter_mut()
            .find(|p| Some(p.id) == self.directory_project)
        {
            project.directory = path.to_string_lossy().into_owned();
        }
        self.directory_project = None;
        self.error.clear();
        self.confirm_save("Project folder saved to local workspace", cx);
    }
    pub fn discovering_codex_models(&self) -> bool { self.codex_discovery.is_some() }
    pub fn discover_codex_models(&mut self, cx: &mut Context<Self>) {
        if self.codex_discovery.is_some() { return; }
        let executable = PathBuf::from(&self.store.research_config.codex_path);
        let (tx, rx) = mpsc::channel();
        self.codex_discovery = Some(rx);
        self.codex_status = "Checking Codex…".into();
        std::thread::spawn(move || { let _ = tx.send(crate::codex::discover(&executable)); });
        self.start_pump(cx);
        cx.notify();
    }
    pub fn discover_models(&mut self, cx: &mut Context<Self>) {
        if self.discovery.is_some() {
            return;
        }
        let executable = PathBuf::from(self.executable_input.read(cx).value().trim());
        if executable.as_os_str().is_empty() {
            self.error = "Set the path to your OpenCode executable.".into();
            cx.notify();
            return;
        }
        self.store.opencode_path = executable.to_string_lossy().into_owned();
        let directory = self
            .store
            .projects
            .iter()
            .find(|p| p.id == self.connection_project)
            .map(|p| PathBuf::from(&p.directory))
            .filter(|p| p.is_dir())
            .or_else(|| self.workspace_root.as_ref().map(|p| p.join("workspace")))
            .unwrap_or_else(|| std::env::temp_dir());
        let (tx, rx) = mpsc::channel();
        self.discovery = Some(rx);
        self.connection_status = "Connecting to OpenCode…".into();
        std::thread::spawn(move || {
            let result = opencode::discover(&executable, &directory);
            let _ = tx.send((executable, result));
        });
        self.start_pump(cx);
        cx.notify();
    }
    pub(crate) fn ready_to_run(&mut self, cx: &mut Context<Self>) -> bool {
        if self.workspace_required {
            self.error = "Set up your workspace first.".into();
            cx.notify();
            return false;
        }
        if self.attachment_import.is_some() {
            self.error = "Wait for materials to finish importing before running.".into();
            cx.notify();
            return false;
        }

        if self.storage.is_none() || !self.storage_error.is_empty() {
            self.error = "Local storage must be available before starting a real run.".into();
            cx.notify();
            return false;
        }
        true
    }
    pub fn start_run(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.route_automation_task(id, cx) {
            return;
        }
        self.store.reconcile_goal_approvals();
        self.error.clear();
        self.task_start_feedback = None;
        if self.store.task_is_archived(id) {
            self.error = "Restore the archived topic or goal before executing.".into();
        } else if self.recovery_blocks_task(id) {
            self.error =
                "Check this task’s previous session in Paused projects before starting again."
                    .into();
        } else if self.task_running(id) {
            self.error =
                "This task is already running or queued. Open View progress to follow it.".into();
        } else {
            self.start_run_inner(id, cx);
        }
        if !self.error.is_empty() {
            self.task_start_feedback = Some((id, self.error.clone()));
        }
        cx.notify();
    }
    fn start_run_inner(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.ready_to_run(cx) || self.task_running(id) {
            return;
        }
        let mut run = match self.store.prepare_run(id) {
            Ok(run) => run,
            Err(e) => {
                self.error = e;
                cx.notify();
                return;
            }
        };
        let files = match self.prepare_run_materials(id, &mut run) {
            Ok(files) => files,
            Err(e) => {
                self.error = e;
                cx.notify();
                return;
            }
        };
        self.launch_run(run, files, cx);
    }
    pub(crate) fn launch_run(
        &mut self,
        mut run: AgentRun,
        files: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if run.automation_occurrence_id.is_none()
            && self
                .store
                .task(run.task_id)
                .is_some_and(|task| task.automation_id.is_some())
        {
            let error = "An automation-owned step cannot start through the ordinary research queue. Use its automation period.".to_string();
            self.error = error.clone();
            self.task_start_feedback = Some((run.task_id, error));
            cx.notify();
            return;
        }
        if self.store.run_is_archived(&run) {
            self.error = "Restore the archived topic or goal before executing.".into();
            cx.notify();
            return;
        }
        if self.recovery_blocks_run(&run) {
            self.error =
                "Check the previous session in Paused projects before starting again.".into();
            cx.notify();
            return;
        }
        if let Err(error) = self.check_workspace_directory(std::path::Path::new(&run.directory)) {
            self.error = error.clone();
            self.task_start_feedback = Some((run.task_id, error));
            cx.notify();
            return;
        }
        if self.store.runs.iter().any(|r| {
            r.task_id == run.task_id
                && r.project_plan == run.project_plan
                && (run.project_plan.is_none()
                    || self.store.request_for_run(r.id).map(|q| q.id)
                        == self.store.request_for_run(run.id).map(|q| q.id))
                && matches!(r.status.as_str(), "Queued" | "Running" | "Cancelling")
        }) {
            return;
        }
        if run.project_plan.is_none() {
            run.workspace_source = match self.store.workspace_source(run.task_id) {
                Ok(source) => source,
                Err(error) => {
                    self.error = error;
                    cx.notify();
                    return;
                }
            };
            match self.store.execution_directory(run.task_id) {
                Ok(path) => {
                    if let Err(error) = self.check_workspace_directory(&path) {
                        self.error = error;
                        cx.notify();
                        return;
                    }
                    if let Err(e) = if run.workspace_source.is_none() {
                        std::fs::create_dir_all(&path)
                    } else {
                        Ok(())
                    } {
                        self.error = format!("Cannot create task working folder: {e}");
                        cx.notify();
                        return;
                    }
                    let directory = path
                        .canonicalize()
                        .unwrap_or(path)
                        .to_string_lossy()
                        .into_owned();
                    run.prompt = run.prompt.replace(
                        &format!("Project directory: {}\n", run.directory),
                        &format!("Project directory: {directory}\n"),
                    );
                    run.directory = directory;
                }
                Err(e) => {
                    self.error = e;
                    cx.notify();
                    return;
                }
            }
        }
        if let Some(previous) = self.store.resume_candidate(&run) {
            run.session_id = previous.session_id.clone();
            run.prompt = format!(
                "Continue this task in the existing session. The previous attempt did not finish. Inspect existing work and reuse verified evidence; do not repeat completed external actions unnecessarily. The current instructions and inputs below supersede earlier ones. Return the complete required result, not just a delta.\n\n{}",
                run.prompt
            );
            run.logs.push(format!(
                "[session] Will resume session from run {} when dispatched",
                previous.id
            ));
        } else if run.agent == "OpenCode" {
            run.session_id = Some(opencode::new_session_id());
        }
        let legacy_source = run.workspace_source.clone().or_else(|| {
            crate::scheduler::migrated_workspace_source(std::path::Path::new(&run.directory))
        });
        if let Some(source) = &legacy_source {
            run.prompt = format!(
                "Workspace migration: prior files have been copied from {source} to {}. Your current working directory is {}. Continue the existing session here. Treat old absolute paths in conversation history as legacy references: use the corresponding copied files in the current directory. Do not write to the legacy folder or other goals' folders.\n\n{}",
                run.directory, run.directory, run.prompt
            );
        }
        if self.workspace_root.is_some() {
            run.prompt = format!(
                "Workspace rule: the approved working directory is {}. Store all generated files here. Previous absolute paths in session history may refer to the old workspace; use the copied files in this directory. Do not write outputs outside this workspace.\n\n{}",
                run.directory, run.prompt
            );
        }
        run.queued_at = opencode::timestamp();
        run.started_at.clear();
        run.status = "Queued".into();
        let run_id = run.id;
        let automation_owned = run.automation_occurrence_id.is_some();
        self.store.runs.push(run);
        self.persist_if_changed(true);
        if !self.storage_error.is_empty() {
            self.store.runs.pop();
            cx.notify();
            return;
        }
        self.queued_runs.push((run_id, files));
        self.undo.clear();
        self.error.clear();
        // Keep the recovery overview visible while its queue submits each next step.
        if self.scope != Scope::Recovery || self.selected.is_some() {
            self.selected = None;
            if automation_owned {
                // Automation periods belong to Automations, not to Research's
                // results. Follow the period to its own results list.
                self.scope = Scope::AutomationRuns;
                self.selected_run = None;
            } else {
                self.scope = Scope::Runs;
                self.selected_run = Some(run_id);
                self.reveal_selected_run();
            }
        }
        self.follow_logs = true;
        self.show_run_result = true;
        self.show_run_details = false;
        self.toast = if automation_owned {
            "Automation period queued · waiting for an automation slot".into()
        } else {
            "Task queued · waiting for a research execution slot".into()
        };
        self.start_pump(cx);
        cx.notify();
    }
    fn run_is_automation_owned(&self, run_id: u64) -> bool {
        self.store
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .is_some_and(|run| run.automation_occurrence_id.is_some())
    }
    /// Research occupancy. Automation steps keep only their own automation pool
    /// busy, even though both kinds share the same physical queue and resources.
    pub(crate) fn research_slots_used(&self) -> usize {
        self.active_runs
            .iter()
            .filter(|active| !self.run_is_automation_owned(active.id))
            .count()
            + self
                .store
                .runs
                .iter()
                .filter(|run| {
                    run.automation_occurrence_id.is_none()
                        && run.status == "Recovering"
                        && self.store.is_recovery_candidate(run)
                })
                .count()
    }
    /// Automation occupancy: running periods plus any automation process still
    /// recovering. Queued periods and periods waiting for a retry do not consume
    /// a slot. Count once per occurrence so a period that is between steps still
    /// holds the slot it already owns.
    pub(crate) fn automation_slots_used(&self) -> usize {
        let mut occupied = std::collections::BTreeSet::new();
        for occurrence in &self.store.automation_occurrences {
            if occurrence.status == crate::automation::OccurrenceStatus::Running {
                occupied.insert(occurrence.id);
            }
        }
        for active in &self.active_runs {
            if let Some(occurrence) = self
                .store
                .runs
                .iter()
                .find(|run| run.id == active.id)
                .and_then(|run| run.automation_occurrence_id)
            {
                occupied.insert(occurrence);
            }
        }
        for run in &self.store.runs {
            if run.status == "Recovering"
                && self.store.is_recovery_candidate(run)
                && let Some(occurrence) = run.automation_occurrence_id
            {
                occupied.insert(occurrence);
            }
        }
        occupied.len()
    }
    fn dispatch_runs(&mut self, cx: &mut Context<Self>) {
        if !self.storage_error.is_empty() {
            return;
        }
        loop {
            let research_full =
                self.research_slots_used() >= self.store.concurrency_limit();
            let automation_full =
                self.automation_slots_used() >= self.store.automation_concurrency_limit();
            if research_full && automation_full {
                break;
            }
            let next = self.queued_runs.iter().position(|(id, _)| {
                let candidate = self.store.runs.iter().find(|r| r.id == *id).unwrap();
                let automation_owned = candidate.automation_occurrence_id.is_some();
                if automation_owned {
                    if automation_full {
                        return false;
                    }
                    // At most one running period per automation; a second request waits.
                    let automation = candidate
                        .automation_occurrence_id
                        .and_then(|id| self.store.occurrence(id))
                        .map(|occurrence| occurrence.automation_id);
                    if automation.is_some_and(|automation| {
                        self.store.automation_occurrences.iter().any(|occurrence| {
                            occurrence.automation_id == automation
                                && occurrence.id != candidate.automation_occurrence_id.unwrap_or(0)
                                && occurrence.status == crate::automation::OccurrenceStatus::Running
                        })
                    }) {
                        return false;
                    }
                } else if research_full {
                    return false;
                }
                // Research topic FIFO never counts automation periods: their
                // own periods are isolated and gated only by the automation pool.
                if candidate.project_plan.is_none() && !automation_owned {
                    let topic = self.store.execution_topic(candidate.task_id);
                    if self.active_runs.iter().filter_map(|active| self.store.runs.iter().find(|run| run.id == active.id))
                        .chain(self.store.runs.iter().filter(|run| run.status == "Recovering" && self.store.is_recovery_candidate(run)))
                        .filter(|run| run.automation_occurrence_id.is_none())
                        .any(|run| run.project_plan.is_none() && self.store.execution_topic(run.task_id) != topic)
                    { return false; }
                    let front = self.todo_batch.as_ref().and_then(|batch| batch.pending_topics(&self.store).first().copied())
                        .or_else(|| self.queued_runs.iter().filter_map(|(id, _)| self.store.runs.iter().find(|run| run.id == *id && run.project_plan.is_none() && run.automation_occurrence_id.is_none()))
                            .next().map(|run| self.store.execution_topic(run.task_id)));
                    if front.is_some_and(|front| front != topic) { return false; }
                }
                if candidate.auto_retry_attempt > 0 {
                    if candidate.retry_delay_remaining(chrono::Utc::now().timestamp()) > 0 {
                        return false;
                    }
                    if self.todo_batch.as_ref().is_some_and(|batch|
                        batch.run_ids.contains_key(id) && batch.state != crate::batch::BatchState::Running)
                    {
                        return false;
                    }
                }
                !self
                    .store
                    .runs
                    .iter()
                    .filter(|r| {
                        self.store.is_recovery_candidate(r)
                            && !matches!(
                                self.recovery.checks.get(&r.id),
                                Some(crate::recovery::SessionCheck::Stopped)
                            )
                    })
                    .any(|running| self.store.run_conflicts(candidate, running))
                    && !self.active_runs.iter().any(|a| {
                        self.store
                            .runs
                            .iter()
                            .find(|r| r.id == a.id)
                            .is_some_and(|running| self.store.run_conflicts(candidate, running))
                    })
            });
            let Some(index) = next else {
                break;
            };
            let (id, files) = self.queued_runs.remove(index);
            let index = self.store.runs.iter().position(|r| r.id == id).unwrap();
            let run = &self.store.runs[index];
            // Queued requests are snapshots. Never execute stale approvals or evidence.
            let automation_owned_without_occurrence = run.automation_occurrence_id.is_none()
                && self
                    .store
                    .task(run.task_id)
                    .is_some_and(|task| task.automation_id.is_some());
            let valid = !automation_owned_without_occurrence
                && if let Some(request) = self.store.request_for_run(run.id) {
                self.store.request_plan_signature(request.id).ok().as_ref()
                    == Some(&run.input_signature)
            } else if let Some(project) = run.project_plan {
                self.store.project_plan_signature(project).ok().as_ref()
                    == Some(&run.input_signature)
            } else if run.research_kind.is_some() {
                self.store.research_signature(run.task_id).as_ref() == Some(&run.input_signature)
                    && !self.store.research_pending_approval(run.task_id)
                    && self.store.task(run.task_id).is_some_and(|t| {
                        !t.archived && !matches!(t.stage, Stage::Backlog | Stage::Cancelled)
                    })
            } else {
                false
            };
            if !valid || self.store.run_is_archived(run) {
                self.store.runs[index].status = "Stale".into();
                self.store.runs[index].ended_at = Some(opencode::timestamp());
                self.store.runs[index].logs.push(
                    "[queue] Inputs or approval changed before execution; review and start again."
                        .into(),
                );
                self.complete_research_run(id, cx);
                continue;
            }
            if let Err(error) = self.check_workspace_directory(std::path::Path::new(&run.directory))
            {
                let run = &mut self.store.runs[index];
                run.status = "Blocked".into();
                run.ended_at = Some(opencode::timestamp());
                run.logs.push(format!("[workspace] {error}"));
                self.task_start_feedback = Some((run.task_id, error));
                self.complete_research_run(id, cx);
                continue;
            }
            if let Some(source) = run.workspace_source.clone() {
                if let Err(error) = crate::scheduler::migrate_workspace(&source, &run.directory) {
                    let run = &mut self.store.runs[index];
                    run.status = "Blocked".into();
                    run.ended_at = Some(opencode::timestamp());
                    run.logs.push(format!("[workspace] {error}"));
                    self.error = error.clone();
                    self.task_start_feedback = Some((run.task_id, error));
                    self.persist_if_changed(true);
                    self.complete_research_run(id, cx);
                    continue;
                }
                self.store.runs[index].workspace_source = None;
                self.store.runs[index].logs.push("[workspace] Legacy research files copied to the isolated goal folder; original files preserved.".into());
            }
            if let Err(error) = crate::research::write_dependency_files(&self.store, &self.store.runs[index]) {
                let run = &mut self.store.runs[index];
                run.status = "Blocked".into();
                run.ended_at = Some(opencode::timestamp());
                run.logs.push(format!("[workspace] {error}"));
                self.task_start_feedback = Some((run.task_id, error));
                self.persist_if_changed(true);
                self.complete_research_run(id, cx);
                continue;
            }
            self.store.refresh_queued_browser_control(index);
            let automation_occurrence = self.store.runs[index].automation_occurrence_id;
            let automation_research_kind = self.store.runs[index].research_kind;
            if let Some(occurrence_id) = automation_occurrence {
                let Some(occurrence) = self.store.occurrence(occurrence_id).cloned() else {
                    let run = &mut self.store.runs[index];
                    run.status = "Blocked".into();
                    run.ended_at = Some(opencode::timestamp());
                    run.logs.push("[automation] The occurrence is missing; nothing was started.".into());
                    self.persist_if_changed(true);
                    self.complete_research_run(id, cx);
                    continue;
                };
                let now = Self::automation_now();
                let regeneration = occurrence.status == crate::automation::OccurrenceStatus::Succeeded
                    && automation_research_kind == Some(crate::research::ResearchKind::Advise);
                if self.store.runs[index]
                    .deadline_at
                    .is_some_and(|deadline| now >= deadline)
                {
                    let run = &mut self.store.runs[index];
                    run.status = "Stale".into();
                    run.ended_at = Some(opencode::timestamp());
                    run.logs.push(
                        "[automation] Report regeneration exceeded its own time limit before starting; the previous report is preserved."
                            .into(),
                    );
                    self.persist_if_changed(true);
                    self.complete_research_run(id, cx);
                    continue;
                }
                if !regeneration && occurrence.first_start_expired(now) {
                    let run = &mut self.store.runs[index];
                    run.status = "Stale".into();
                    run.ended_at = Some(opencode::timestamp());
                    run.logs.push("[automation] The queued request exceeded its allowed delay and was skipped.".into());
                    let _ = self.store.occurrence_mut(occurrence_id).map(|value| value.transition(
                        crate::automation::OccurrenceStatus::Skipped,
                        Some(crate::automation::OccurrenceReason::new(crate::automation::SkipCode::Expired, "The queued request exceeded its allowed delay before a slot was available.")),
                    ));
                    self.persist_if_changed(true);
                    self.complete_research_run(id, cx);
                    continue;
                }
                if !regeneration && occurrence.deadline_at.is_some_and(|deadline| now >= deadline) {
                    let run = &mut self.store.runs[index];
                    run.status = "Stale".into();
                    run.ended_at = Some(opencode::timestamp());
                    run.logs.push("[automation] The occurrence's total time limit was reached before this step started.".into());
                    let _ = self.store.occurrence_mut(occurrence_id).map(|value| value.transition(
                        crate::automation::OccurrenceStatus::Failed,
                        Some(crate::automation::OccurrenceReason::new(crate::automation::SkipCode::Expired, "The automation's total time limit was reached.")),
                    ));
                    self.store.note_automation_failure(occurrence.automation_id, now);
                    self.persist_if_changed(true);
                    self.complete_research_run(id, cx);
                    continue;
                }
                if !regeneration
                    && occurrence.status != crate::automation::OccurrenceStatus::Running
                {
                    let first_start = occurrence.actual_started_at.is_none();
                    if let Some(value) = self.store.occurrence_mut(occurrence_id) {
                        if let Err(error) = value.mark_running(now) {
                            let run = &mut self.store.runs[index];
                            run.status = "Blocked".into();
                            run.ended_at = Some(opencode::timestamp());
                            run.logs.push(format!("[automation] {error}"));
                            self.store.mark_automation_attention(occurrence.automation_id, error);
                            self.persist_if_changed(true);
                            self.complete_research_run(id, cx);
                            continue;
                        }
                    }
                    if first_start {
                        self.automation_started
                            .insert(occurrence_id, std::time::Instant::now());
                    }
                }
            }
            let run = &self.store.runs[index];
            let request = opencode::Request { agent: None, browser_executable: (run.agent == "Codex" && run.research_kind == Some(crate::research::ResearchKind::Collect) && !self.store.owning_research_goal(run.task_id).is_some_and(|goal| self.store.research_browser_handed_off(goal))).then(|| PathBuf::from(&self.store.research_config.ego_path)),
                files,
                session_id: run.session_id.clone(),
                executable: PathBuf::from(self.store.agent_path(&run.agent)),
                directory: PathBuf::from(&run.directory),
                model: run.model.clone(),
                prompt: run.prompt.clone(),
                title: run.task_title.clone(),
            };
            self.store.runs[index].status = "Running".into();
            self.store.runs[index].started_at = opencode::timestamp();
            self.persist_if_changed(true);
            if !self.storage_error.is_empty() {
                self.store.runs[index].status = "Queued".into();
                self.store.runs[index].started_at.clear();
                self.queued_runs.insert(0, (id, request.files));
                break;
            }
            let started = match self.store.runs[index].agent.as_str() {
                "Codex" => crate::codex::start(request),
                "OpenCode" => opencode::start(request),
                agent => Err(format!("Unsupported agent: {agent}")),
            };
            match started {
                Ok(handle) => {
                    self.store.mark_research_running(id);
                    self.active_runs.push(ActiveRun {
                        id,
                        handle,
                        had_error: false,
                    });
                    self.toast = format!(
                        "{} research running · {} automation running · {} queued",
                        self.research_slots_used(),
                        self.automation_slots_used(),
                        self.queued_runs.len()
                    );
                }
                Err(e) => {
                    let run = &mut self.store.runs[index];
                    run.status = "Failed".into();
                    run.ended_at = Some(opencode::timestamp());
                    run.logs.push(format!("[error] {e}"));
                    self.error = e;
                    self.complete_research_run(id, cx);
                }
            }
            self.persist_if_changed(true);
        }
    }
    pub fn cancel_run_id(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(goal) = self.run_goal(id) {
            self.research_sequences.remove(&goal);
        }
        if let Some(active) = self.active_runs.iter().find(|a| a.id == id) {
            active.handle.cancel();
            if let Some(run) = self.store.runs.iter_mut().find(|r| r.id == id) {
                run.status = "Cancelling".into();
                run.logs.push("[run] Cancellation requested".into());
            }
        } else if let Some(index) = self.queued_runs.iter().position(|(run, _)| *run == id) {
            self.queued_runs.remove(index);
            if let Some(run) = self.store.runs.iter_mut().find(|r| r.id == id) {
                run.status = "Cancelled".into();
                run.ended_at = Some(opencode::timestamp());
            }
            self.complete_research_run(id, cx);
        }
        self.persist_if_changed(true);
        cx.notify();
    }
    pub fn cancel_run(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.displayed_run_id() {
            self.cancel_run_id(id, cx);
        }
    }
    pub(crate) fn start_pump(&mut self, cx: &mut Context<Self>) {
        if self.pumping {
            return;
        }
        self.pumping = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                let keep = this
                    .update(cx, |this, cx| this.poll_runtime(cx))
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }
    pub fn poll_runtime(&mut self, cx: &mut Context<Self>) -> bool {
        if self.startup.workspace_pending.is_some() { cx.notify(); }
        self.poll_startup(cx);
        self.enforce_active_run_deadlines(Self::automation_now(), cx);
        self.poll_research_io(cx);
        self.poll_recovery(cx);
        if let Some(result) = self.codex_discovery.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.codex_discovery = None;
            match result {
                Ok(result) => {
                    self.codex_version = Some(result.version);
                    self.codex_models = result.models;
                    self.codex_status = result.status;
                }
                Err(error) => { self.codex_status = error; self.codex_models.clear(); }
            }
            cx.notify();
        }
        let discovery = self.discovery.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some((executable, result)) = discovery {
            self.discovery = None;
            match result {
                Ok((version, models)) => {
                    self.opencode_version = Some((executable.to_string_lossy().into_owned(), version));
                    self.connection_status = format!("{} local models available", models.len());
                    self.available_models = models;
                    self.error.clear();
                }
                Err(e) => {
                    self.connection_status = e;
                    self.available_models.clear();
                }
            }
            cx.notify();
        }
        let mut completed = vec![];
        let mut changed = false;
        for active in &mut self.active_runs {
            // Bound per-worker draining so a noisy session cannot starve others or the UI.
            let events: Vec<_> = active.handle.events.try_iter().take(128).collect();
            changed |= !events.is_empty();
            if let Some(message) = events.iter().find_map(|event| match event {
                opencode::Event::BrowserBlocked(message) => Some(message.clone()),
                _ => None,
            }) {
                active.handle.cancel();
                self.toast = format!("Browser paused; research will continue with evidence gaps. {message}");
            }
            if let Some(run) = self.store.runs.iter_mut().find(|r| r.id == active.id) {
                if record_events(run, &mut active.had_error, events) {
                    completed.push(active.id);
                }
            }
        }
        self.active_runs.retain(|a| !completed.contains(&a.id));
        for id in completed {
            self.complete_research_run(id, cx);
        }
        self.advance_todo_batch(cx);
        self.dispatch_runs(cx);
        self.poll_report_publishing(cx);
        if !self.automation_timer_armed {
            self.poll_automations(cx);
        }
        if self.queued_runs.iter().any(|(id, _)| self.store.runs.iter()
            .any(|run| run.id == *id && run.auto_retry_attempt > 0)) {
            cx.notify();
        }
        if changed {
            if self.follow_logs {
                self.log_scroll.scroll_to_bottom();
            }
            cx.notify();
        }
        if self.persist_if_changed(false) {
            cx.notify();
        }
        let keep = !self.active_runs.is_empty()
            || !self.queued_runs.is_empty()
            || self.startup.pending.is_some()
            || self.startup.workspace_pending.is_some()
            || self.discovery.is_some()
            || self.codex_discovery.is_some()
            || self.ego_probe.is_some()
            || self.attachment_import.is_some()
            || self.publishing_job.is_some()
            || self.recovery_pending();
        if !keep {
            self.pumping = false;
        }
        keep
    }
    fn shutdown(&mut self) {
        self.cancel_automation_timer();
        self.research_sequences.clear();
        for mut active in std::mem::take(&mut self.active_runs) {
            let (stopped, events) = active.handle.stop_and_wait(Duration::from_secs(5));
            if let Some(run) = self.store.runs.iter_mut().find(|r| r.id == active.id) {
                let finished = record_events(run, &mut active.had_error, events);
                if !stopped {
                    run.logs.push(
                        "[warning] Process cleanup timed out after requesting termination.".into(),
                    );
                }
                if (!finished || run.status == "Cancelled") && run.status != "Blocked" {
                    run.status = "Interrupted".into();
                    run.ended_at = Some(opencode::timestamp());
                    run.logs.push("[interrupted] Application closed before a final result was received; process termination was requested.".into());
                }
            }
            self.finalize_research_run(active.id);
        }
        for (id, _) in self.queued_runs.drain(..) {
            if let Some(run) = self.store.runs.iter_mut().find(|r| r.id == id) {
                run.status = "Interrupted".into();
                run.ended_at = Some(opencode::timestamp());
            }
        }
        self.persist_if_changed(true);
    }
}
fn record_events(run: &mut AgentRun, had_error: &mut bool, events: Vec<opencode::Event>) -> bool {
    let mut finished = false;
    for event in events {
        match event {
            opencode::Event::Log(text) => {
                if text.starts_with("[error]") || text.starts_with("[stream error]") {
                    *had_error = true;
                }
                run.logs.push(text);
                if run.logs.len() > 2000 {
                    run.logs.remove(0);
                    run.logs[0] = "[log] Earlier output omitted (2,000 event limit).".into();
                }
            }
            opencode::Event::Text(text) => {
                run.output = text;
            }
            opencode::Event::Session(id) => run.session_id = Some(id),
            opencode::Event::BrowserBlocked(message) => {
                run.status = "Blocked".into();
                run.logs.push(format!("[browser] Access paused · {message}"));
                run.logs.push(crate::research::BROWSER_HANDOFF_LOG.into());
            }
            opencode::Event::Finished {
                code,
                cancelled,
                error,
            } => {
                run.exit_code = code;
                run.ended_at = Some(opencode::timestamp());
                run.status = if run.status == "Blocked" {
                    "Blocked"
                } else if cancelled || run.status == "Cancelling" {
                    "Cancelled"
                } else if code == Some(0) && error.is_none() && !*had_error {
                    "Succeeded"
                } else {
                    "Failed"
                }
                .into();
                if let Some(e) = error {
                    run.logs.push(format!("[error] {e}"));
                }
                run.logs.push(format!(
                    "[process] {} · exit {}",
                    if run.status == "Succeeded" { "Exited normally (process only)" }
                    else if run.status == "Blocked" && run.logs.iter().any(|line| line == crate::research::BROWSER_HANDOFF_LOG) { "Stopped for browser handoff (research result pending)" }
                    else { &run.status },
                    code.map(|c| c.to_string())
                        .unwrap_or("signal / unavailable".into())
                ));
                finished = true;
            }
        }
    }
    finished
}
pub fn valid_date(s: &str) -> bool {
    let p: Vec<_> = s.split('-').collect();
    if p.len() != 3 || p[0].len() != 4 || p[1].len() != 2 || p[2].len() != 2 {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (
        p[0].parse::<u32>(),
        p[1].parse::<u32>(),
        p[2].parse::<u32>(),
    ) else {
        return false;
    };
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    };
    y > 0 && d > 0 && d <= days
}

#[cfg(test)]
mod tests {
    use super::valid_date;
    #[test]
    fn requested_cancellation_is_not_a_retryable_failure() {
        let mut store = crate::domain::Store::mock();
        store.simulate_run(1).unwrap();
        let run = &mut store.runs[0];
        run.status = "Cancelling".into();
        super::record_events(run, &mut false, vec![crate::opencode::Event::Finished {
            code: Some(1), cancelled: false, error: None,
        }]);
        assert_eq!(run.status, "Cancelled");
    }
    #[test]
    fn browser_handoff_stays_blocked_after_process_cleanup() {
        let mut store = crate::domain::Store::mock();
        store.simulate_run(1).unwrap();
        let run = &mut store.runs[0];
        assert!(super::record_events(
            run,
            &mut false,
            vec![
                crate::opencode::Event::BrowserBlocked("User confirmation required".into()),
                crate::opencode::Event::Finished {
                    code: None,
                    cancelled: true,
                    error: None
                },
            ]
        ));
        assert_eq!(run.status, "Blocked");
        assert!(
            run.logs
                .iter()
                .any(|l| l.contains("User confirmation required"))
        );
    }
    #[test]
    fn shutdown_tail_events_preserve_session_output_and_final_status() {
        let mut store = crate::domain::Store::mock();
        store.simulate_run(1).unwrap();
        let run = &mut store.runs[0];
        run.status = "Running".into();
        assert!(super::record_events(
            run,
            &mut false,
            vec![
                crate::opencode::Event::Session("session".into()),
                crate::opencode::Event::Log("last output".into()),
                crate::opencode::Event::Finished {
                    code: Some(0),
                    cancelled: false,
                    error: None
                }
            ]
        ));
        assert_eq!(run.status, "Succeeded");
        assert_eq!(run.session_id.as_deref(), Some("session"));
        assert!(run.logs.iter().any(|s| s == "last output"));
        assert!(run.logs.iter().any(|s| s == "[process] Exited normally (process only) · exit 0"));
        assert!(!run.logs.iter().any(|s| s.starts_with("[run] Succeeded")));
    }
    #[test]
    fn dates_validate_calendar_days_not_only_shape() {
        for value in ["2024-02-29", "2000-02-29", "2026-10-28"] {
            assert!(valid_date(value));
        }
        for value in [
            "1900-02-29",
            "2026-02-29",
            "2026-04-31",
            "0000-01-01",
            "2026-00-10",
            "2026-01-00",
            "2026-1-1",
            "invalid",
        ] {
            assert!(!valid_date(value));
        }
    }
}
