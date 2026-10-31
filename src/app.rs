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
type AttachmentImport = (u64, Vec<Result<crate::attachments::Attachment, String>>);
type Discovery = Result<(String, Vec<String>), String>;

#[derive(Clone, Debug, PartialEq)]
pub enum Scope {
    All,
    Project(u64),
    Topic(u64),
    Cycle(String),
    Archive,
    Workflow,
    Runs,
    Recovery,
    Research,
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
}
impl AllStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All statuses",
            Self::Open => "Open",
            Self::Stage(stage) => stage.label(),
        }
    }
    pub fn matches(self, stage: Stage) -> bool {
        match self {
            Self::All => true,
            Self::Open => !stage.is_closed(),
            Self::Stage(value) => stage == value,
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
    pub id: Option<u64>,
    pub parent: Option<u64>,
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
    pub toast: String,
    pub error: String,
    pub task_start_feedback: Option<(u64, String)>,
    pub save_feedback: Option<Result<String, String>>,
    pub show_display: bool,
    pub show_done: bool,
    pub sort_priority: bool,
    pub compact: bool,
    pub collapsed: Vec<u64>,
    pub policy_stage: Stage,
    pub policy_agent: String,
    pub policy_model: Entity<InputState>,
    pub policy_prompt: Entity<TextareaState>,
    pub policy_auto: bool,
    pub undo: Vec<Store>,
    pub focus: FocusHandle,
    pub modal_focus: FocusHandle,
    pub new_project: Option<Entity<InputState>>,
    pub topic_editor: Option<(u64, Option<u64>, Entity<InputState>)>,
    pub collapsed_sidebar_topics: Vec<u64>,
    pub research_topic: Option<u64>,
    pub planning_project: Option<u64>,
    pub planning_topic: Option<u64>,
    pub project_brief: Entity<TextareaState>,
    pub storage: Option<Storage>,
    pub workspace_required: bool,
    pub workspace_choice: Option<PathBuf>,
    pub workspace_root: Option<PathBuf>,
    pub workspace_setup: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    pub storage_error: String,
    last_saved: Store,
    last_save: Instant,
    _storage_lock: Option<std::fs::File>,
    pub executable_input: Entity<InputState>,
    pub directory_input: Entity<InputState>,
    pub directory_project: Option<u64>,
    pub connection_project: u64,
    pub connection_status: String,
    pub available_models: Vec<String>,
    pub global_model: Entity<InputState>,
    pub global_model_filter: Entity<InputState>,
    pub show_global_models: bool,
    pub use_global_for_all: bool,
    pub show_models: bool,
    discovery: Option<mpsc::Receiver<Discovery>>,
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
    pub research_model_picker: Option<usize>,
    pub research_sequences: std::collections::BTreeSet<u64>,
    pub todo_batch: Option<crate::batch::TodoBatch>,
    pub(crate) recovery: crate::recovery::Recovery,
    pub attachment_import: Option<mpsc::Receiver<AttachmentImport>>,
    pub show_run_result: bool,
    pub show_run_details: bool,
    pub show_task_details: bool,
    pub expanded_request_brief: Option<u64>,
    pub goal_steps_tab: Option<u64>,
    pub steps_layout: Layout,
    pub show_task_activity: bool,
    pub show_research_settings: bool,
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
                app.attach_storage(
                    Storage::new(root.join("workspace/workspace.sqlite3")),
                    window,
                    cx,
                );
                app.workspace_required = app.storage.is_none() || !app.storage_error.is_empty();
            }
            Ok(None) => {}
            Err(e) => app.storage_error = e,
        }
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
    fn check_workspace_directory(&self, path: &std::path::Path) -> Result<(), String> {
        if self.workspace_required {
            return Err("Set up your workspace before running tasks.".into());
        }
        if let Some(root) = &self.workspace_root {
            crate::workspace::check_directory(&root.join("workspace/projects"), path)?;
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
        let first = store
            .policies
            .iter()
            .find(|p| p.stage == Stage::Planned)
            .cloned();
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
        let global_model_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search provider or model…"));
        let global_changes = cx.subscribe(&global_model, |_, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let filter_changes = cx.subscribe(&global_model_filter, |_, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let policy_model = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(first.as_ref().map(|p| p.model.clone()).unwrap_or_default())
                .placeholder("Leave blank to use global model")
        });
        let research_models: [_; 4] = std::array::from_fn(|_| {
            cx.new(|cx| InputState::new(window, cx).placeholder("Leave blank to use global model"))
        });
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
            global_changes,
            filter_changes,
            system_appearance,
        ];
        for input in std::iter::once(&policy_model).chain(research_models.iter()) {
            subscriptions.push(cx.subscribe(input, |_, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }));
        }
        Self {
            last_saved: store.clone(),
            storage: None,
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
            directory_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("/absolute/path/to/project")),
            directory_project: None,
            connection_project: 1,
            connection_status: "Refresh models to connect to local OpenCode".into(),
            available_models: vec![],
            global_model,
            global_model_filter,
            show_global_models: false,
            use_global_for_all: false,
            show_models: false,
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
            toast: "Local workspace".into(),
            error: String::new(),
            task_start_feedback: None,
            save_feedback: None,
            show_display: false,
            show_done: true,
            sort_priority: false,
            compact: false,
            collapsed: vec![],
            policy_stage: Stage::Planned,
            policy_agent: first
                .as_ref()
                .map(|p| p.agent.clone())
                .unwrap_or("Codex".into()),
            policy_model,
            policy_prompt: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .default_value(first.as_ref().map(|p| p.prompt.clone()).unwrap_or_default())
                    .rows(7)
            }),
            policy_auto: false,
            undo: vec![],
            focus,
            modal_focus: cx.focus_handle(),
            new_project: None,
            topic_editor: None,
            collapsed_sidebar_topics: vec![],
            research_topic: None,
            planning_project: None,
            planning_topic: None,
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
            ego_input: cx.new(|cx| {
                InputState::new(window, cx).default_value(crate::research::default_ego_path())
            }),
            ego_status: "Not checked · install ego(lite) and finish its first launch".into(),
            ego_probe: None,
            research_model_picker: None,
            research_sequences: Default::default(),
            todo_batch: None,
            recovery: Default::default(),
            attachment_import: None,
            show_run_result: true,
            show_run_details: false,
            show_task_details: false,
            expanded_request_brief: None,
            goal_steps_tab: None,
            steps_layout: Layout::Board,
            show_task_activity: false,
            show_research_settings: false,
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
            self.reveal_selected_run();
        }
        self.save_feedback = None;
        self.show_run_details = false;
        self.show_run_result = true;
        self.show_research_settings = false;
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
    pub(crate) fn reveal_task_project(&mut self, id: u64) {
        if let Some(topic) = self.store.task_topic(id) {
            self.collapsed_sidebar_topics.retain(|t| *t != topic.id);
        }
        if let Some(research) = self.store.task(id).and_then(|t| t.research.as_ref()) {
            self.collapsed_sidebar_goals
                .retain(|goal| *goal != research.goal_id);
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
            Scope::All => "All".into(),
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
            Scope::Workflow => "Stage workflow".into(),
            Scope::Runs => "Progress & results".into(),
            Scope::Recovery => "Paused projects".into(),
            Scope::Research => "Market research".into(),
            Scope::Settings => "Settings".into(),
        }
    }
    pub fn matches_scope(&self, t: &Task) -> bool {
        if self.store.task_is_archived(t.id) != (self.scope == Scope::Archive) {
            return false;
        }
        match &self.scope {
            Scope::All => self.all_status.matches(t.stage),
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
                    && (self.scope == Scope::All || self.show_done || !t.stage.is_closed())
            })
            .cloned()
            .collect();
        if self.sort_priority {
            list.sort_by_key(|t| std::cmp::Reverse(t.priority as u8));
        }
        list
    }
    pub fn change_stage(&mut self, id: u64, stage: Stage, cx: &mut Context<Self>) {
        if self
            .store
            .task(id)
            .is_some_and(|t| t.parent_id.is_some() && t.research.is_some())
        {
            self.error = "Review and approve the complete goal; steps inherit its approval.".into();
            cx.notify();
            return;
        }
        if self.store.task(id).is_some_and(|t| t.stage == stage) {
            return;
        }
        self.snapshot();
        let result = self.store.set_stage(id, stage);
        self.finish(result, "Task stage updated", cx);
    }
    pub fn begin_draft(
        &mut self,
        id: Option<u64>,
        parent: Option<u64>,
        stage: Stage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = id.and_then(|id| self.store.task(id)).cloned();
        let project = task
            .as_ref()
            .map(|t| t.project_id)
            .or_else(|| parent.and_then(|id| self.store.task(id).map(|t| t.project_id)))
            .unwrap_or(match self.scope {
                Scope::Project(id) => id,
                Scope::Topic(id) => self
                    .store
                    .topics
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.project_id)
                    .unwrap_or(0),
                _ => self.store.projects.first().map(|p| p.id).unwrap_or(0),
            });
        let title = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("What needs to be done?")
                .default_value(task.as_ref().map(|t| t.title.clone()).unwrap_or_default())
        });
        self.draft = Some(Draft {
            id,
            cycle: if let Some(t) = &task {
                t.cycle.clone()
            } else {
                match &self.scope {
                    Scope::Cycle(c) => Some(c.clone()),
                    _ => None,
                }
            },
            parent: task.as_ref().and_then(|t| t.parent_id).or(parent),
            project,
            stage: task.as_ref().map(|t| t.stage).unwrap_or(stage),
            title: title.clone(),
            description: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Describe the outcome, context, and acceptance criteria…")
                    .default_value(
                        task.as_ref()
                            .map(|t| t.description.clone())
                            .unwrap_or_default(),
                    )
                    .rows(6)
            }),
            labels: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Feature, UI")
                    .default_value(
                        task.as_ref()
                            .map(|t| t.labels.join(", "))
                            .unwrap_or_default(),
                    )
            }),
            due: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("YYYY-MM-DD")
                    .default_value(
                        task.as_ref()
                            .map(|t| t.due_date.clone())
                            .unwrap_or_default(),
                    )
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
        let (id, project, stage, parent, cycle) =
            (d.id, d.project, d.stage, d.parent, d.cycle.clone());
        self.snapshot();
        let result = if let Some(id) = id {
            Ok(id)
        } else {
            self.store
                .create_task_in_topic(title.clone(), project, stage, parent, match self.scope { Scope::Topic(id) => Some(id), _ => None })
        };
        match result {
            Ok(id) => {
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
            }
            Err(e) => {
                self.undo.pop();
                self.show_save_error(e, cx);
            }
        }
        cx.notify();
    }
    pub fn select_policy(&mut self, stage: Stage, window: &mut Window, cx: &mut Context<Self>) {
        self.policy_stage = stage;
        if let Some(p) = self.store.policies.iter().find(|p| p.stage == stage) {
            self.policy_agent = p.agent.clone();
            self.policy_auto = p.auto_start;
            let model = p.model.clone();
            let prompt = p.prompt.clone();
            self.policy_model
                .update(cx, |s, cx| s.set_value(model, window, cx));
            self.policy_prompt
                .update(cx, |s, cx| s.set_value(prompt, window, cx));
        }
        cx.notify();
    }
    pub fn save_policy(&mut self, cx: &mut Context<Self>) {
        self.snapshot();
        if let Some(p) = self
            .store
            .policies
            .iter_mut()
            .find(|p| p.stage == self.policy_stage)
        {
            p.agent = self.policy_agent.clone();
            p.model = self.policy_model.read(cx).value().trim().to_string();
            p.prompt = self.policy_prompt.read(cx).value().to_string();
            p.auto_start = self.policy_auto;
        }
        self.confirm_save("Stage configuration saved to local workspace", cx);
    }
    pub fn undo_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.active_runs.is_empty() || !self.queued_runs.is_empty() {
            self.error =
                "Wait for queued and running tasks to finish before undoing changes.".into();
            cx.notify();
            return;
        }
        if let Some(mut store) = self.undo.pop() {
            store.runs = self.store.runs.clone();
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
            self.select_policy(self.policy_stage, window, cx);
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
    pub fn attach_storage(
        &mut self,
        storage: Storage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let loaded = storage
            .acquire_lock()
            .and_then(|lock| storage.load().map(|store| (lock, store)));
        match loaded {
            Ok((lock, store)) => {
                self._storage_lock = Some(lock);
                self.store = store.unwrap_or_else(Store::empty);
                self.store.recover_interrupted_runs();
                self.store.reconcile_goal_approvals();
                let project = self.store.projects.first().map(|p| p.id).unwrap_or(0);
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
                self.select_policy(self.policy_stage, window, cx);
                self.refresh_research_inputs(window, cx);
                self.refresh_global_model_input(window, cx);
                match storage.save(&self.store) {
                    Ok(()) => {
                        self.last_saved = self.store.clone();
                        self.toast = "Workspace saved locally".into();
                    }
                    Err(e) => self.storage_error = e,
                }
                self.storage = Some(storage);
                self.check_interrupted_sessions(cx);
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
                had_error
            }
            Err(e) => {
                let changed = self.storage_error != e;
                self.storage_error = e;
                changed
            }
        }
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
            let _ = tx.send(opencode::discover(&executable, &directory));
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
        } else {
            run.session_id = Some(opencode::new_session_id());
        }
        let legacy_source = run.workspace_source.clone().or_else(|| {
            std::fs::read_to_string(PathBuf::from(&run.directory).join(".workspace-migrated")).ok()
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
            self.scope = Scope::Runs;
            self.selected_run = Some(run_id);
            self.reveal_selected_run();
        }
        self.follow_logs = true;
        self.show_run_result = true;
        self.show_run_details = false;
        self.toast = "Task queued · waiting for an execution slot".into();
        self.start_pump(cx);
        cx.notify();
    }
    fn dispatch_runs(&mut self, cx: &mut Context<Self>) {
        if !self.storage_error.is_empty() {
            return;
        }
        while self.active_runs.len()
            + self
                .store
                .runs
                .iter()
                .filter(|r| r.status == "Recovering" && self.store.is_recovery_candidate(r))
                .count()
            < crate::scheduler::MAX_CONCURRENT_RUNS
        {
            let next = self.queued_runs.iter().position(|(id, _)| {
                let candidate = self.store.runs.iter().find(|r| r.id == *id).unwrap();
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
            let valid = if let Some(request) = self.store.request_for_run(run.id) {
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
                self.store
                    .task(run.task_id)
                    .is_some_and(|t| !t.archived && t.stage == run.stage)
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
            let run = &self.store.runs[index];
            let request = opencode::Request {
                files,
                session_id: run.session_id.clone(),
                executable: PathBuf::from(&self.store.opencode_path),
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
            match opencode::start(request) {
                Ok(handle) => {
                    self.store.mark_research_running(id);
                    self.active_runs.push(ActiveRun {
                        id,
                        handle,
                        had_error: false,
                    });
                    self.toast = format!(
                        "{} tasks running · {} queued",
                        self.active_runs.len(),
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
        self.poll_research_io(cx);
        self.poll_recovery(cx);
        let discovery = self.discovery.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = discovery {
            self.discovery = None;
            match result {
                Ok((version, models)) => {
                    self.connection_status = format!("{version} · {} models", models.len());
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
        let mut stopped_goals = vec![];
        for active in &mut self.active_runs {
            // Bound per-worker draining so a noisy session cannot starve others or the UI.
            let events: Vec<_> = active.handle.events.try_iter().take(128).collect();
            changed |= !events.is_empty();
            if let Some(message) = events.iter().find_map(|event| match event {
                opencode::Event::BrowserBlocked(message) => Some(message.clone()),
                _ => None,
            }) {
                active.handle.cancel();
                stopped_goals.push(active.id);
                self.error = message;
            }
            if let Some(run) = self.store.runs.iter_mut().find(|r| r.id == active.id) {
                if record_events(run, &mut active.had_error, events) {
                    completed.push(active.id);
                }
            }
        }
        for id in stopped_goals {
            if let Some(goal) = self.run_goal(id) {
                self.research_sequences.remove(&goal);
            }
        }
        self.active_runs.retain(|a| !completed.contains(&a.id));
        for id in completed {
            self.complete_research_run(id, cx);
        }
        self.dispatch_runs(cx);
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
            || self.discovery.is_some()
            || self.ego_probe.is_some()
            || self.attachment_import.is_some()
            || self.recovery_pending();
        if !keep {
            self.pumping = false;
        }
        keep
    }
    fn shutdown(&mut self) {
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
                run.logs.push(format!("[blocked] {message}"));
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
                    "[run] {} · exit {}",
                    run.status,
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
