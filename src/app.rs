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
    pub(crate) handle: opencode::Running,
    pub(crate) had_error: bool,
}
pub enum AttachmentImport {
    Progress(String),
    /// Completion carries the workspace root captured when the import started.
    /// Later events must clean up or attach against that original root, never a
    /// root derived after the user switched workspaces.
    Complete(
        u64,
        PathBuf,
        Vec<Result<crate::attachments::Attachment, String>>,
    ),
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    List,
    Board,
}

/// Result of admitting one run to the execution queue. Navigation is separate:
/// background dispatch returns this value to its caller, while an explicit user
/// command decides whether to open a progress surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RunDispatch {
    pub run_id: u64,
    pub automation_owned: bool,
}

/// Result of a background run submission. `PendingAdmission` means the run is
/// accepted but its run record is not durable yet; it must not be treated as a
/// queue failure or a launched attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunSubmission {
    Dispatched(RunDispatch),
    PendingAdmission {
        run_id: u64,
        automation_owned: bool,
    },
    Rejected,
}

/// A batch step whose run was accepted by durable admission but whose run
/// record is not durable yet. The batch generation pins it to the batch that
/// requested it so a receipt can never bind into a later, unrelated batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PendingBatchRun {
    pub run_id: u64,
    pub goal: u64,
    pub step: u64,
    pub batch_generation: u64,
}

/// Ordered retirement phase for a cancelled pre-launch dispatch. The durable
/// dispatch intent is removed first; only its acknowledgement allows the
/// cancellation marker to be removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CancelRetirePhase {
    Intent,
    Marker,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingCancelRetire {
    pub operation_key: String,
    pub run_id: u64,
    pub phase: CancelRetirePhase,
    /// The current phase request was enqueued; do not enqueue it again.
    pub requested: bool,
}

pub(crate) struct PreparedRunDispatch {
    pub run: AgentRun,
    pub files: Vec<PathBuf>,
}

/// A failed dispatch commit retains only the intent, never the old run id or
/// full staged store. Retry rebuilds the run from the current store.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LaunchIntent {
    Occurrence(u64),
    Task(u64),
    RequestPlan {
        request_id: u64,
    },
    ProjectPlan {
        project_id: u64,
    },
    ReportRegeneration {
        source_id: u64,
        style: String,
        occurrence: Option<u64>,
    },
}

#[derive(Clone)]
pub(crate) struct PendingDispatch {
    pub intent: LaunchIntent,
    pub files: Vec<PathBuf>,
}

/// One chronological Undo entry. Every entry is a typed scoped inverse; there
/// is no whole-Store snapshot path and no parallel legacy mirror.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UndoRecord {
    TaskEdit(crate::scoped_edit::TaskEditUndo),
    Comment(crate::scoped_edit::CommentEditUndo),
    Material(crate::scoped_material::MaterialUndo),
    Settings(crate::scoped_settings::SettingsUndo),
    Domain(crate::scoped_domain::DomainUndo),
}

fn undo_entry_references_reserved(entry: &UndoRecord, reserved: &[String]) -> bool {
    if reserved.is_empty() {
        return false;
    }
    let references = match entry {
        UndoRecord::Material(undo) => undo.reference_paths(),
        UndoRecord::TaskEdit(_)
        | UndoRecord::Comment(_)
        | UndoRecord::Settings(_)
        | UndoRecord::Domain(_) => Vec::new(),
    };
    references.iter().any(|path| reserved.contains(path))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct UndoStack {
    entries: Vec<UndoRecord>,
}

impl UndoStack {
    fn push(&mut self, entry: UndoRecord) {
        self.entries.push(entry);
        if self.entries.len() > 30 {
            self.entries.remove(0);
        }
    }

    pub(crate) fn push_domain(&mut self, undo: crate::scoped_domain::DomainUndo) {
        self.push(UndoRecord::Domain(undo));
    }

    pub(crate) fn push_task_edit(&mut self, undo: crate::scoped_edit::TaskEditUndo) {
        self.push(UndoRecord::TaskEdit(undo));
    }

    pub(crate) fn push_comment(&mut self, undo: crate::scoped_edit::CommentEditUndo) {
        self.push(UndoRecord::Comment(undo));
    }

    pub(crate) fn push_material(&mut self, undo: crate::scoped_material::MaterialUndo) {
        self.push(UndoRecord::Material(undo));
    }

    pub(crate) fn push_settings(&mut self, undo: crate::scoped_settings::SettingsUndo) {
        self.push(UndoRecord::Settings(undo));
    }

    pub(crate) fn pop(&mut self) -> Option<UndoRecord> {
        self.entries.pop()
    }

    pub(crate) fn peek(&self) -> Option<&UndoRecord> {
        self.entries.last()
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn material_reference_paths(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                UndoRecord::Material(undo) => Some(undo.reference_paths()),
                _ => None,
            })
            .flatten()
            .collect()
    }

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
#[derive(Clone, Debug)]
pub(crate) struct DispatchStatusRetirement {
    pub key: String,
    pub run_id: u64,
    pub rerun: bool,
    pub replacement: Option<u64>,
    pub had_claim: bool,
    /// No claim remains (never existed or its removal was acknowledged).
    pub claim_done: bool,
    pub claim_enqueued: bool,
    pub decision_enqueued: bool,
    pub decision_started: bool,
}

pub struct TaskApp {
    pub store: Store,
    /// T12 S2: typed route identity. `scope()` is a read-only projection; no
    /// writable `scope` field exists on the app.
    nav: crate::navigation_route::NavState,
    pub layout: Layout,
    pub all_status: AllStatus,
    pub sidebar_scroll: ScrollHandle,
    pub sidebar_reveal: Option<u64>,
    pub sidebar_selected_row: ScrollHandle,
    pub detail_scroll: ScrollHandle,
    pub result_scroll: ScrollHandle,
    pub tabs: Vec<u64>,
    pub expanded_sidebar_projects: Vec<u64>,
    pub collapsed_sidebar_goals: Vec<u64>,
    pub sidebar_split: Entity<gpui_kit::base::ResizableState>,
    pub results_split: Entity<gpui_kit::base::ResizableState>,
    pub search: Entity<InputState>,
    /// Global search overlay (footer Search / ⌘K), separate from the list-local
    /// `search` filter above.
    pub global_search: Entity<InputState>,
    pub global_search_ui: crate::global_search::GlobalSearchUi,
    pub global_search_focus: FocusHandle,
    pub draft: Option<Draft>,
    /// T21 Phase 2a: a Save that opened the blocking impact review. This is
    /// pending user input, never a save or durability claim.
    pub(crate) research_save_impact: Option<crate::research_input_app::PendingResearchSave>,
    /// T21 Phase 2c: task whose retained result versions are open, and the
    /// selected run.
    pub(crate) previous_versions_task: Option<u64>,
    pub(crate) previous_versions_run: Option<u64>,
    pub(crate) previous_versions_scroll: ScrollHandle,
    pub(crate) previous_version_text: Option<Entity<gpui_kit::base::text::TextViewState>>,
    /// Concrete first control focused when a T21 modal opens, so native focus
    /// lands on an actionable control rather than only the trap container.
    pub(crate) dialog_first_focus: FocusHandle,
    /// T19 opener for ordinary application dialogs. Restored on close/cancel.
    pub(crate) dialog_opener_focus: Option<gpui_kit::WeakFocusHandle>,
    pub(crate) research_save_opener: Option<FocusHandle>,
    pub(crate) previous_versions_opener: Option<FocusHandle>,
    pub property: Option<Property>,
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
    pub undo: UndoStack,
    /// A task-editor save whose durable write failed. The editor draft and T21
    /// review remain; retry re-prepares against the current store.
    pub(crate) pending_task_edit: Option<crate::task_edit_app::PendingTaskEdit>,
    pub(crate) pending_comment_edit: Option<crate::task_edit_app::PendingCommentEdit>,
    pub(crate) pending_material_edit: Option<crate::material_app::PendingMaterialEdit>,
    pub(crate) material_verifier: crate::material_verify::MaterialVerifier,
    pub(crate) pending_material_job: Option<crate::material_app::PendingMaterialJob>,
    pub(crate) material_cleanup_reserved: Vec<String>,
    pub(crate) pending_cleanup_retry: Vec<crate::scoped_material::CleanupObligation>,
    /// Set when a durable automation mutation discarded undo entries that could
    /// otherwise restore stale definitions, occurrences, cursors or approvals.
    /// The footer shows this instead of "Nothing to undo".
    pub undo_invalidation: Option<String>,
    /// Failed fail-closed command retained for reprepare against the current store.
    pub(crate) pending_commit: Option<crate::domain_command::PreparedCommand>,
    /// Failed emergency stop retained for retry; external work is already stopped.
    pub(crate) pending_stop: Option<crate::domain_command::PreparedCommand>,
    /// Failed dispatch retained as intent; retry rebuilds run/ids from current state.
    pub(crate) pending_dispatch: Option<PendingDispatch>,
    /// Exact reserved dispatch (run id included) retained across busy/backpressure
    /// retries; generic reprepare must never allocate a new identity.
    pub(crate) pending_dispatch_reserved: Option<(LaunchIntent, AgentRun, Vec<std::path::PathBuf>)>,
    /// L1b durable dispatch admission. None keeps the accepted synchronous path.
    pub(crate) dispatch_admission: Option<crate::dispatch_admission::DispatchAdmissionAdapter>,
    /// The admission awaiting a matching durable receipt; no queue/launch until set.
    pub(crate) dispatch_awaiting: Option<crate::dispatch_admission::AdmissionId>,
    /// Batch run bindings accepted but not yet present in the durable store.
    pub(crate) batch_pending_runs: Vec<PendingBatchRun>,
    /// Monotonic batch identity source. Every new/replaced batch gets a fresh
    /// generation so a late admission receipt cannot join a later batch.
    pub(crate) batch_generation_seq: u64,
    /// Runs cancelled while their dispatch was still pre-ACK. The durable
    /// receipt must retire them without claiming or spawning.
    pub(crate) dispatch_cancelled: std::collections::BTreeMap<u64, String>,
    /// Receipt-ordered retirement of a cancelled dispatch intent and marker.
    pub(crate) dispatch_cancel_retire: Vec<PendingCancelRetire>,
    /// The persistence worker cannot progress dispatch work until an explicit
    /// retry. The bounded pump must stop instead of polling a parked group.
    pub(crate) dispatch_parked: bool,
    /// QA-only one-shot hold after a durable receipt but before its launch claim.
    #[cfg(feature = "qa")]
    pub(crate) qa_hold_claim: bool,
    /// Files recomputed for the dispatch, kept until its durable receipt.
    pub(crate) dispatch_awaiting_files: Option<Vec<std::path::PathBuf>>,
    pub(crate) dispatch_admission_error: Option<String>,
    /// Operation keys whose durable payload was already merged in this session.
    pub(crate) dispatch_applied: std::collections::BTreeSet<String>,
    /// Operation keys already handed to the scheduler in this session.
    pub(crate) dispatch_launched: std::collections::BTreeSet<String>,
    /// Set while the worker owns an acknowledgement that has not been confirmed.
    pub(crate) dispatch_ack_pending: Option<crate::dispatch_admission::AdmissionId>,
    pub(crate) dispatch_resolution_pending: Option<(String, u64, bool, Option<u64>)>,
    pub(crate) dispatch_resolution_new: Option<(String, u64)>,
    /// Status receipt is durable; claim/marker retirement is retried until the
    /// matching `LaunchClaimReleased`/`ResolutionReleased` event, never merely
    /// on enqueue success.
    pub(crate) dispatch_status_durable: Option<DispatchStatusRetirement>,
    pub(crate) dispatch_resolution_launch_pending: Option<String>,
    pub(crate) dispatch_claim_pending: Option<(
        crate::dispatch_admission::AdmissionId,
        std::sync::Arc<crate::dispatch_intent::AdmissionParts>,
    )>,
    pub(crate) dispatch_recovery_done: bool,
    #[cfg(feature = "qa")]
    pub(crate) qa_dispatch_fake: Option<crate::dispatch_admission::FakeDispatchRunner>,
    #[cfg(feature = "qa")]
    pub(crate) qa_storage_writes: std::cell::Cell<usize>,
    /// Occurrences with a stop intent that must not be auto-restarted.
    pub(crate) stop_intents: Vec<crate::domain_command::StopIntent>,
    /// Malformed/unreadable/newer stop journal. Automation recovery and
    /// admission stay paused until the user resolves it.
    pub(crate) stop_journal_error: Option<String>,
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
    /// Visible, validated destination of the open research-request form.
    pub planning_destination: Option<crate::navigation_context::CreationMode>,
    /// Instance generation of the open planning composer. Reopening the same
    /// destination/text creates a new generation so a late save receipt cannot
    /// close or reset the new form.
    pub(crate) planning_form_epoch: u64,
    /// Why the current destination is invalid or was replaced; shown in the form.
    pub planning_destination_error: Option<String>,
    /// Control that opened the research-request form, restored when it closes.
    pub(crate) planning_opener_focus: Option<gpui_kit::FocusHandle>,
    pub project_brief: Entity<TextareaState>,
    pub storage: Option<Storage>,
    /// Ordinary and command writes are submitted as typed groups to the existing
    /// bounded persistence worker; the legacy writer is never used in this mode.
    pub(crate) live_save_enabled: bool,
    /// A v4 workspace was selected. Legacy writes must never be used, even if
    /// the worker failed to attach; the backend stays unavailable until retry.
    pub(crate) live_required: bool,
    pub(crate) live_attach: Option<(PathBuf, PathBuf, u64)>,
    pub(crate) live_save_inflight_epoch: Option<u64>,
    pub(crate) live_command_generation: Option<u64>,
    pub(crate) live_command_applying: bool,
    /// Exact ordered continuation for a form/command/runtime write. At most one
    /// primary continuation exists; a second form save is rejected instead of
    /// overwriting it.
    pub(crate) live_primary: Option<crate::live_save_app::LivePending>,
    /// Ordinary coalesced writes and runtime post-commit effects in submission
    /// order. Bounded; compatible effects are merged instead of dropped.
    pub(crate) live_followups: std::collections::VecDeque<crate::live_save_app::LiveGroup>,
    pub(crate) live_workspace: Option<u64>,
    /// Submission/worker failure kept distinct from the pending state. The dirty
    /// edits remain in memory and retry; no effects run.
    pub(crate) live_failure: Option<String>,
    /// Transient startup reason (worker not ready). Never a workspace failure.
    pub(crate) live_pending_reason: Option<String>,
    pub(crate) live_retry_at: Option<Instant>,
    /// One-shot runtime callbacks rejected by continuation backpressure. They
    /// retain only stable identities and are replayed from the bounded pump
    /// after acknowledgements free capacity; no user input or restart is needed.
    pub(crate) live_deferred_runs: std::collections::VecDeque<u64>,
    pub(crate) live_deferred_after_run: std::collections::VecDeque<(u64, u64)>,
    pub(crate) live_deferred_recovery: bool,
    /// Graceful quit/close: the worker and lock stay alive until every accepted
    /// mutation is durable. The window is not closed on failure.
    pub(crate) quit_requested: bool,
    pub(crate) quit_sender: Option<std::sync::mpsc::Sender<bool>>,
    pub(crate) quit_stopping: Option<
        std::sync::mpsc::Receiver<Vec<crate::live_save_app::StoppedRun>>,
    >,
    pub(crate) window_close_requested: bool,
    pub(crate) window_close_ready: bool,
    pub(crate) window_close_registered: bool,
    pub(crate) pending_workspace_switch: Option<Storage>,
    /// Sidecar that must be removed after its recovered delta is durable.
    pub(crate) live_recovery_path: Option<PathBuf>,
    /// T15 off-thread indexed history and on-demand report bodies.
    pub(crate) history_reader: Option<crate::history_reader::HistoryReader>,
    pub(crate) history_cache:
        std::collections::BTreeMap<u64, crate::history_reader::HistoryCache>,
    pub(crate) run_body_state:
        std::collections::BTreeMap<u64, crate::history_reader::BodyLoad>,
    pub(crate) history_request_seq: u64,
    /// Window-dependent acknowledgements (focus/navigation/draft cleanup).
    pub(crate) deferred_ui: Vec<crate::live_save_app::DeferredUiAction>,
    pub startup: crate::startup::Startup,
    pub workspace_required: bool,
    pub workspace_choice: Option<PathBuf>,
    pub workspace_root: Option<PathBuf>,
    pub workspace_setup: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    pub storage_error: String,
    pub(crate) last_saved: Store,
    pub(crate) last_save: Instant,
    pub(crate) _storage_lock: Option<std::fs::File>,
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
    pub global_model: Entity<InputState>,
    pub model_profile_name: Entity<InputState>,
    pub creating_model_profile: bool,
    pub publishing_inputs: [Entity<InputState>; 3],
    pub publishing_scan_requested: bool,
    pub publishing_job: Option<(String, mpsc::Receiver<crate::report_publish::PublishEvent>)>,
    discovery: Option<mpsc::Receiver<(PathBuf, Discovery)>>,
    pub active_runs: Vec<ActiveRun>,
    pub queued_runs: Vec<(u64, Vec<PathBuf>)>,
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
    pub(crate) sidebar_surface: Option<Entity<crate::shell_surfaces::SidebarSurface>>,
    pub(crate) brand_overlay: Option<Entity<crate::shell_surfaces::BrandOverlay>>,
    pub(crate) content_surface: Option<Entity<crate::shell_surfaces::ContentSurface>>,
    pub(crate) footer_bar: Option<Entity<crate::shell_surfaces::FooterBar>>,
    pub materials_focus: FocusHandle,
    pub attachment_status: String,
    pub attachment_started: Option<Instant>,
    pub attachment_import: Option<mpsc::Receiver<AttachmentImport>>,
    pub show_run_result: bool,
    pub show_run_details: bool,
    pub show_task_details: bool,
    pub expanded_request_brief: Option<u64>,
    pub render_statuses:
        std::cell::RefCell<std::collections::HashMap<u64, crate::workflow::TaskStatus>>,
    pub board_execution_filter: Option<crate::workflow::ExecutionGroup>,
    pub show_task_activity: bool,
    pub automation_ui: crate::automation_views::AutomationUi,
    /// T10 return context and recent-surface memory.
    pub(crate) navigation: crate::navigation_context_app::NavigationState,
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
                app.begin_workspace_open(
                    Storage::new(root.join("workspace/workspace.sqlite3")),
                    cx,
                );
            }
            Ok(None) => {}
            Err(e) => app.storage_error = e,
        }
        for input in [
            &app.executable_input,
            &app.codex_executable_input,
            &app.ego_input,
        ] {
            app._subscriptions
                .push(cx.subscribe(input, |_, _, event, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
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
            crate::workspace::check_project_roots(&root.join("workspace"), path)?;
        }
        Ok(())
    }
    #[cfg(any(test, feature = "qa"))]
    pub fn new_mock(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_store(Store::mock(), window, cx)
    }
    fn with_store(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        #[cfg(feature = "qa")]
        crate::brand::start_render_probe_logging();
        let nav = Self::initial_nav(&store);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Filter this list…"));
        let sub = cx.subscribe(&search, |this, state, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.set_query(state.read(cx).value().to_string());
                cx.notify();
            }
        });
        let global_search = cx
            .new(|cx| InputState::new(window, cx).placeholder("Search research and automations…"));
        let global_search_sub = cx.subscribe_in(
            &global_search,
            window,
            |this, state, event, window, cx| match event {
                InputEvent::Change if this.global_search_ui.open => {
                    this.global_search_ui.message = None;
                    let query = state.read(cx).value().to_string();
                    this.global_search_ui.refresh(&this.store, &query);
                    cx.notify();
                }
                // Enter belongs to the search field's own submit event, so the
                // selected result opens whether the key arrives through the
                // overlay's key handler or the focused input action.
                InputEvent::PressEnter { .. } => {
                    if this.global_search_ui.open {
                        this.activate_selected_global_search(window, cx);
                    }
                }
                _ => {}
            },
        );
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let autosave = cx.observe_self(|this, cx| {
            if this.persist_if_changed(false) {
                cx.notify();
            }
        });
        let quit = cx.on_app_quit(|this, _| {
            this.emergency_shutdown();
            async {}
        });
        let release = cx.on_release(|this, _| this.emergency_shutdown());
        let global_model = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(store.default_model().clone())
                .placeholder("provider/model")
        });
        let research_models: [_; 4] = std::array::from_fn(|_| {
            cx.new(|cx| InputState::new(window, cx).placeholder("Global default"))
        });
        let automation_ui = crate::automation_views::AutomationUi::new(window, cx);
        let system_appearance = cx.observe_window_appearance(window, |this, w, cx| {
            if this.store.appearance() == crate::appearance::Appearance::System {
                crate::appearance::apply(this.store.appearance(), Some(w), cx);
                cx.notify();
            }
        });
        let mut subscriptions = vec![
            sub,
            global_search_sub,
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
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }));
        }
        for input in [&automation_ui.objective, &automation_ui.expected] {
            subscriptions.push(cx.subscribe(input, |_, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }));
        }
        for input in research_models.iter().chain([&global_model]) {
            subscriptions.push(
                cx.subscribe_in(input, window, move |this, input, event, w, cx| {
                    match event {
                        InputEvent::Focus
                            if this.scope() == Scope::Settings
                                && this.settings_tab()
                                    == crate::settings_views::SettingsTab::Models =>
                        {
                            this.open_research_model_picker(&input, cx)
                        }
                        InputEvent::PressEnter { .. } => this.finish_model_edit(input, true, w, cx),
                        InputEvent::Blur => this.finish_model_search(input, w, cx),
                        _ => {}
                    }
                    cx.notify();
                }),
            );
        }
        let ego_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(crate::research::default_ego_path())
        });
        subscriptions.push(cx.subscribe(&ego_input, |this, input, event, cx| {
            if matches!(event, InputEvent::Change)
                && this.scope() == Scope::Settings
                && this.settings_tab() == crate::settings_views::SettingsTab::Models
                && !this.startup.required
            {
                let value = input.read(cx).value();
                if value.trim() != this.store.research_config().ego_path {
                    this.store.research_config_mut().ego_path = value.trim().to_owned();
                    this.ego_status = "Runtime not checked".into();
                    cx.notify();
                }
            }
        }));
        Self {
            last_saved: store.clone(),
            storage: None,
            live_save_enabled: false,
            live_required: false,
            live_attach: None,
            live_save_inflight_epoch: None,
            live_command_generation: None,
            live_command_applying: false,
            live_primary: None,
            live_followups: std::collections::VecDeque::new(),
            live_workspace: None,
            live_failure: None,
            live_pending_reason: None,
            live_retry_at: None,
            live_deferred_runs: std::collections::VecDeque::new(),
            live_deferred_after_run: std::collections::VecDeque::new(),
            live_deferred_recovery: false,
            quit_requested: false,
            quit_sender: None,
            quit_stopping: None,
            window_close_requested: false,
            window_close_ready: false,
            window_close_registered: false,
            pending_workspace_switch: None,
            live_recovery_path: None,
            history_reader: None,
            history_cache: std::collections::BTreeMap::new(),
            run_body_state: std::collections::BTreeMap::new(),
            history_request_seq: 0,
            deferred_ui: Vec::new(),
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
            codex_executable_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(crate::codex::detect_executable())
                    .placeholder("Path to codex")
            }),
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
            publishing_inputs: std::array::from_fn(|_| cx.new(|cx| InputState::new(window, cx))),
            publishing_job: None,
            publishing_scan_requested: true,
            global_model,
            creating_model_profile: false,
            model_profile_name: cx
                .new(|cx| InputState::new(window, cx).placeholder("Profile name")),
            discovery: None,
            active_runs: vec![],
            queued_runs: vec![],
            expanded_run_sources: None,
            show_planning_activity: false,
            log_scroll: ScrollHandle::new(),
            follow_logs: true,
            pumping: false,
            store,
            nav,
            layout: Layout::List,
            all_status: AllStatus::All,
            sidebar_scroll: ScrollHandle::new(),
            sidebar_reveal: None,
            sidebar_selected_row: ScrollHandle::new(),
            detail_scroll: ScrollHandle::new(),
            result_scroll: ScrollHandle::new(),
            tabs: vec![],
            expanded_sidebar_projects: vec![],
            collapsed_sidebar_goals: vec![],
            sidebar_split: cx.new(|_| gpui_kit::base::ResizableState::default()),
            results_split: cx.new(|_| gpui_kit::base::ResizableState::default()),
            search,
            global_search,
            global_search_ui: Default::default(),
            global_search_focus: cx.focus_handle(),
            draft: None,
            research_save_impact: None,
            previous_versions_task: None,
            previous_versions_run: None,
            previous_versions_scroll: ScrollHandle::new(),
            previous_version_text: None,
            dialog_first_focus: cx.focus_handle(),
            dialog_opener_focus: None,
            research_save_opener: None,
            previous_versions_opener: None,
            property: None,
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
            undo: UndoStack::default(),
            pending_task_edit: None,
            pending_comment_edit: None,
            pending_material_edit: None,
            material_verifier: crate::material_verify::MaterialVerifier::new(),
            pending_material_job: None,
            material_cleanup_reserved: Vec::new(),
            pending_cleanup_retry: Vec::new(),
            undo_invalidation: None,
            pending_commit: None,
            pending_stop: None,
            pending_dispatch: None,
            pending_dispatch_reserved: None,
            dispatch_admission: None,
            dispatch_awaiting: None,
            batch_pending_runs: Vec::new(),
            batch_generation_seq: 0,
            dispatch_cancelled: std::collections::BTreeMap::new(),
            dispatch_cancel_retire: Vec::new(),
            dispatch_parked: false,
            #[cfg(feature = "qa")]
            qa_hold_claim: false,
            dispatch_awaiting_files: None,
            dispatch_admission_error: None,
            dispatch_applied: std::collections::BTreeSet::new(),
            dispatch_launched: std::collections::BTreeSet::new(),
            dispatch_ack_pending: None,
            dispatch_resolution_pending: None,
            dispatch_resolution_new: None,
            dispatch_status_durable: None,
            dispatch_resolution_launch_pending: None,
            dispatch_claim_pending: None,
            dispatch_recovery_done: false,
            #[cfg(feature = "qa")]
            qa_dispatch_fake: None,
            #[cfg(feature = "qa")]
            qa_storage_writes: std::cell::Cell::new(0),
            stop_intents: vec![],
            stop_journal_error: None,
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
            planning_destination: None,
            planning_form_epoch: 0,
            planning_destination_error: None,
            planning_opener_focus: None,
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
            sidebar_surface: None,
            brand_overlay: None,
            content_surface: None,
            footer_bar: None,
            materials_focus: cx.focus_handle(),
            attachment_status: String::new(),
            attachment_started: None,
            attachment_import: None,
            show_run_result: true,
            show_run_details: false,
            show_task_details: false,
            expanded_request_brief: None,
            render_statuses: Default::default(),
            board_execution_filter: None,
            show_task_activity: false,
            automation_ui,
            navigation: crate::navigation_context_app::NavigationState::new(),
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
    pub(crate) fn apply_scoped_settings(
        &mut self,
        change: crate::scoped_settings::SettingsChange,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut staged = self.store.clone();
        let undo = match crate::scoped_settings::apply_settings_change(&mut staged, change) {
            Ok(undo) => undo,
            Err(crate::scoped_settings::SettingsError::NoChange) => return true,
            Err(crate::scoped_settings::SettingsError::Rejected(message)) => {
                self.show_save_error(message, cx);
                return false;
            }
        };
        let acceptance = match self.save_staged_store(
            &staged,
            crate::live_save_app::LiveCompletion::Settings {
                undo: undo.clone(),
                message: "Settings saved · active work continues normally".into(),
            },
            cx,
        ) {
            Ok(acceptance) => acceptance,
            Err(error) => {
                self.storage_error = error.clone();
                self.show_save_error(error, cx);
                return false;
            }
        };
        // In live mode the staged value is already merged and queued; adopting
        // the clone again would overwrite the live store outside the journal.
        if !self.live_persistence_active() {
            self.adopt_saved_store(staged);
        }
        self.undo_invalidation = None;
        if acceptance.is_durable() && !self.live_persistence_active() {
            // The synchronous path already completed the continuation.
        }
        self.toast = if acceptance.is_durable() {
            "Settings saved · active work continues normally".into()
        } else {
            "Saving settings…".into()
        };
        self.error.clear();
        cx.notify();
        true
    }

    /// Execute a bounded domain command against the live store. The command
    /// records only its own write set; there is no whole-Store staging clone.
    /// A failed durable save rolls the command back through that same write set.
    pub(crate) fn apply_domain_command(
        &mut self,
        command: crate::scoped_domain::DomainCommand,
        message: &str,
        undoable: bool,
        cx: &mut Context<Self>,
    ) -> Option<crate::scoped_domain::DomainOutcome> {
        if let Err(error) = self.live_can_accept_auxiliary() {
            self.storage_error = error.clone();
            self.show_save_error(error, cx);
            return None;
        }
        let outcome = match command.execute(&mut self.store) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.show_save_error(error, cx);
                return None;
            }
        };
        // The continuation owns the inverse and the durable toast. A pending
        // receipt must not push Undo or claim saved state yet.
        let completion = crate::live_save_app::LiveCompletion::Domain {
            undo: (undoable && outcome.undo.changed_entities() > 0)
                .then(|| outcome.undo.clone()),
            message: message.into(),
        };
        let acceptance = match self.save_current_store_durably(completion, cx) {
            Ok(acceptance) => acceptance,
            Err(error) => {
                let _ = crate::scoped_domain::apply_undo(&mut self.store, &outcome.undo);
                self.storage_error = error.clone();
                self.show_save_error(error, cx);
                return None;
            }
        };
        self.undo_invalidation = None;
        if !acceptance.is_durable() {
            self.toast = "Saving…".into();
        }
        self.error.clear();
        cx.notify();
        Some(outcome)
    }

    pub(crate) fn apply_domain_command_undoable(
        &mut self,
        command: crate::scoped_domain::DomainCommand,
        message: &str,
        cx: &mut Context<Self>,
    ) -> Option<crate::scoped_domain::DomainOutcome> {
        self.apply_domain_command(command, message, true, cx)
    }

    /// Persist a domain mutation that is explicitly non-undoable, such as
    /// archiving a target whose related work had to be stopped.
    pub(crate) fn apply_domain_command_non_undoable(
        &mut self,
        command: crate::scoped_domain::DomainCommand,
        message: &str,
        cx: &mut Context<Self>,
    ) -> Option<crate::scoped_domain::DomainOutcome> {
        self.apply_domain_command(command, message, false, cx)
    }

    /// Automation definitions, occurrences, approvals and cursors are never
    /// recorded as Undo entries, so there is no whole-store snapshot to
    /// invalidate. The guard remains as the single lifecycle call point.
    pub(crate) fn invalidate_undo_for_automation(&mut self) {
        self.undo_invalidation = None;
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
    /// Read-only page-family projection. The typed `nav` route is the single
    /// stored authority; sidebar/collection code keeps consuming `Scope` until
    /// the remaining page-state families move in S5.
    pub fn scope(&self) -> Scope {
        self.nav.route().legacy_scope()
    }

    /// List-local filter text owned by the typed ResearchList page.
    pub(crate) fn query(&self) -> String {
        match self.nav.route() {
            crate::navigation_route::Route::ResearchList(page) => page.query().to_owned(),
            _ => String::new(),
        }
    }

    pub(crate) fn set_query(&mut self, query: String) {
        let _ = self.nav.set_research_query(query);
    }

    pub(crate) fn clear_query(&mut self) {
        self.set_query(String::new());
    }

    pub(crate) fn settings_page(&self) -> bool {
        matches!(
            self.nav.route(),
            crate::navigation_route::Route::Settings(_)
        )
    }

    /// Active Settings section, or the last preference when another page is
    /// visible. The active Settings page owns its tab in the typed route.
    pub(crate) fn settings_section(&self) -> crate::navigation_route::SettingsSection {        match self.nav.route() {
            crate::navigation_route::Route::Settings(page) => page.tab(),
            _ => self.navigation.settings_section,
        }
    }

    pub(crate) fn settings_tab(&self) -> crate::settings_views::SettingsTab {        match self.settings_section() {
            crate::navigation_route::SettingsSection::General => {
                crate::settings_views::SettingsTab::General
            }
            crate::navigation_route::SettingsSection::Models => {
                crate::settings_views::SettingsTab::Models
            }
            crate::navigation_route::SettingsSection::Publishing => {
                crate::settings_views::SettingsTab::Publishing
            }
        }
    }

    pub(crate) fn set_settings_tab(&mut self, tab: crate::settings_views::SettingsTab) {
        use crate::navigation_route::SettingsSection;
        let section = match tab {
            crate::settings_views::SettingsTab::General => SettingsSection::General,
            crate::settings_views::SettingsTab::Models => SettingsSection::Models,
            crate::settings_views::SettingsTab::Publishing => SettingsSection::Publishing,
        };
        self.navigation.settings_section = section;
        let _ = self.nav.set_settings_tab(section);
    }

    /// Goal whose Steps tab is active; this is the typed Task panel, not a
    /// second global selection.
    pub(crate) fn goal_steps_tab(&self) -> Option<u64> {
        match self.nav.route() {
            crate::navigation_route::Route::ResearchTask(page)
                if page.panel() == crate::navigation_route::TaskPanel::Steps =>
            {
                Some(page.task())
            }
            _ => None,
        }
    }

    pub(crate) fn steps_layout(&self) -> Layout {
        match self.nav.route() {
            crate::navigation_route::Route::ResearchTask(page) => page.steps_layout(),
            _ => self.navigation.steps_layout,
        }
    }

    pub(crate) fn set_goal_steps_tab(&mut self, goal: Option<u64>) {
        use crate::navigation_route::{Route, TaskPanel};
        let crate::navigation_route::Route::ResearchTask(page) = self.nav.route() else {
            return;
        };
        let task = page.task();
        let layout = page.steps_layout();
        let base = self.scope();
        let panel = if goal == Some(task) {
            TaskPanel::Steps
        } else {
            TaskPanel::Overview
        };
        self.install_route(Route::research_task_from(task, base, panel, layout));
    }

    pub(crate) fn set_steps_layout(&mut self, layout: Layout) {
        use crate::navigation_route::Route;
        self.navigation.steps_layout = layout;
        let Route::ResearchTask(page) = self.nav.route() else {
            return;
        };
        let task = page.task();
        let panel = page.panel();
        let base = self.scope();
        self.install_route(Route::research_task_from(task, base, panel, layout));
    }

    /// Task detail identity is the typed `ResearchTask` route; there is no
    /// separate writable `selected` field.
    pub(crate) fn selected(&self) -> Option<u64> {
        match self.nav.route() {
            crate::navigation_route::Route::ResearchTask(page) => Some(page.task()),
            _ => None,
        }
    }

    /// Task or Goal context of the displayed result page.
    pub(crate) fn task_result_context(&self) -> Option<u64> {
        let crate::navigation_route::Route::ResearchResult(page) = self.nav.route() else {
            return None;
        };
        match page.owner() {
            crate::navigation_route::ResultOwner::Task(task)
            | crate::navigation_route::ResultOwner::Goal { goal: task }
            | crate::navigation_route::ResultOwner::AutomationTask { task, .. } => Some(task),
            _ => None,
        }
    }

    pub(crate) fn selected_run(&self) -> Option<u64> {
        match self.nav.route() {
            crate::navigation_route::Route::ResearchResult(page) => page.run(),
            _ => self.navigation.selected_attempt,
        }
    }

    fn stage_index(stage: crate::domain::Stage) -> u8 {
        crate::domain::Stage::BOARD_ORDER
            .iter()
            .position(|value| *value == stage)
            .unwrap_or(0) as u8
    }

    /// Capture the complete live page for a typed Back frame: route, typed
    /// overlays, scroll bookmarks and the page-local filters/layout that are not
    /// yet part of route identity.
    pub(crate) fn capture_return_frame(&self) -> crate::navigation_route::ReturnFrame {
        use crate::navigation_route::{PageState, ReturnFrame, ScrollBookmark, ScrollKey};
        let mut scroll = ScrollBookmark::default();
        scroll.set(
            ScrollKey::TaskList,
            0.0,
            self.navigation.task_list_scroll.0.borrow().base_handle.offset().y.as_f32(),
        );
        scroll.set(
            ScrollKey::BoardX,
            self.navigation.board_scroll.offset().x.as_f32(),
            0.0,
        );
        for stage in crate::domain::Stage::BOARD_ORDER {
            scroll.set(
                ScrollKey::BoardColumn(Self::stage_index(stage)),
                0.0,
                self.navigation.board_column(stage).offset().y.as_f32(),
            );
        }
        scroll.set(
            ScrollKey::Topic,
            self.navigation.topic_scroll.offset().x.as_f32(),
            self.navigation.topic_scroll.offset().y.as_f32(),
        );
        scroll.set(
            ScrollKey::TaskDetail,
            self.detail_scroll.offset().x.as_f32(),
            self.detail_scroll.offset().y.as_f32(),
        );
        scroll.set(
            ScrollKey::Result,
            self.result_scroll.offset().x.as_f32(),
            self.result_scroll.offset().y.as_f32(),
        );
        scroll.set(
            ScrollKey::Settings,
            self.settings_scroll.offset().x.as_f32(),
            self.settings_scroll.offset().y.as_f32(),
        );
        scroll.set(
            ScrollKey::Dialog,
            self.automation_ui.dialog_scroll.offset().x.as_f32(),
            self.automation_ui.dialog_scroll.offset().y.as_f32(),
        );
        scroll.set(
            ScrollKey::PeriodBody,
            self.automation_ui.scroll.offset().x.as_f32(),
            self.automation_ui.scroll.offset().y.as_f32(),
        );
        ReturnFrame {
            route: self.nav.route().clone(),
            overlays: self.nav.overlays().to_vec(),
            scroll,
            result_fallback: None,
            page: PageState {
                query: self.query(),
                all_status: self.all_status,
                layout: self.layout,
                results_filter: self.automation_ui.result_filter,
                history_limit: self.automation_ui.history_limit,
                settings_tab: self.settings_section(),
                steps_layout: self.steps_layout(),
            },
        }
    }

    pub(crate) fn record_return_frame(&mut self, frame: crate::navigation_route::ReturnFrame) {
        self.nav.record_return(frame);
    }

    pub(crate) fn clear_return_frames(&mut self) {
        self.nav.clear_returns();
    }

    pub(crate) fn has_return_frames(&self) -> bool {
        !self.nav.returns().is_empty()
    }

    pub(crate) fn return_frames(&self) -> &crate::navigation_route::ReturnStack {
        self.nav.returns()
    }

    pub(crate) fn back_frame(
        &mut self,
    ) -> Result<crate::navigation_route::NavEffect, crate::navigation_route::NavReject> {
        self.nav.back(&self.store)
    }

    /// Apply one typed return frame after `NavState::back` restored its route.
    pub(crate) fn apply_return_frame(
        &mut self,
        frame: crate::navigation_route::ReturnFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::navigation_route::ScrollKey;
        self.all_status = frame.page.all_status;
        self.layout = frame.page.layout;
        self.navigation.settings_section = frame.page.settings_tab;
        self.navigation.steps_layout = frame.page.steps_layout;
        self.automation_ui.result_filter = frame.page.results_filter;
        self.automation_ui.history_limit = frame.page.history_limit;
        match &frame.route {
            crate::navigation_route::Route::AutomationList => {
                self.automation_ui.selected = None;
                self.automation_ui.creating = false;
                self.automation_ui.editing = None;
            }
            crate::navigation_route::Route::AutomationDetail(page) => {
                self.automation_ui.selected = Some(page.automation());
                self.automation_ui.creating = false;
                self.automation_ui.editing = None;
            }
            crate::navigation_route::Route::AutomationEditor(page) => {
                self.automation_ui.creating = true;
                self.automation_ui.editing = page.editing();
                if let Some(id) = page.editing() {
                    self.automation_ui.selected = Some(id);
                }
            }
            _ => {}
        }
        let query = frame.page.query.clone();
        let _ = self.nav.set_research_query(query.clone());
        self.search.update(cx, |state, cx| state.set_value(query, window, cx));
        if let Some((x, y)) = frame.scroll.get(ScrollKey::TaskList) {
            self.navigation
                .task_list_scroll
                .0
                .borrow()
                .base_handle
                .set_offset(point(px(0.), px(y)));
            let _ = x;
        }
        if let Some((x, _)) = frame.scroll.get(ScrollKey::BoardX) {
            self.navigation.board_scroll.set_offset(point(px(x), px(0.)));
        }
        for stage in crate::domain::Stage::BOARD_ORDER {
            if let Some((_, y)) = frame
                .scroll
                .get(ScrollKey::BoardColumn(Self::stage_index(stage)))
            {
                self.navigation
                    .board_column(stage)
                    .set_offset(point(px(0.), px(y)));
            }
        }
        if let Some((x, y)) = frame.scroll.get(ScrollKey::Topic) {
            self.navigation.topic_scroll.set_offset(point(px(x), px(y)));
        }
        if let Some((x, y)) = frame.scroll.get(ScrollKey::TaskDetail) {
            self.detail_scroll.set_offset(point(px(x), px(y)));
        }
        if let Some((x, y)) = frame.scroll.get(ScrollKey::Result) {
            self.result_scroll.set_offset(point(px(x), px(y)));
        }
        if let Some((x, y)) = frame.scroll.get(ScrollKey::Settings) {
            self.settings_scroll.set_offset(point(px(x), px(y)));
        }
        if let Some((x, y)) = frame.scroll.get(ScrollKey::Dialog) {
            self.automation_ui
                .dialog_scroll
                .set_offset(point(px(x), px(y)));
        }
        if let Some((x, y)) = frame.scroll.get(ScrollKey::PeriodBody) {
            self.automation_ui.scroll.set_offset(point(px(x), px(y)));
        }
    }

    /// Record a run selection that is not a result page (planning/report
    /// payload pending the S4 overlay migration). A result page still owns its
    /// selection in the typed route.
    pub(crate) fn note_selected_run(&mut self, run: Option<u64>) {
        match self.nav.route() {
            crate::navigation_route::Route::ResearchResult(_) => self.set_selected_run(run),
            _ => self.navigation.selected_attempt = run,
        }
    }

    /// Open one task detail over the current page family. Selection is the
    /// typed route identity, so no writable `selected` field survives.
    pub(crate) fn set_task_identity(
        &mut self,
        task: u64,
        panel: crate::navigation_route::TaskPanel,
        steps_layout: Layout,
    ) -> bool {
        let base = self.scope();
        let route =
            crate::navigation_route::Route::research_task_from(task, base, panel, steps_layout);
        self.install_route(route)
    }

    /// Select an ordinary/task/goal result page. `context` is the explicit task
    /// or Goal identity; `None` means the global Research results page.
    pub(crate) fn set_result_selection(&mut self, context: Option<u64>, run: Option<u64>) {
        use crate::navigation_route::{ResultOwner, Route};
        let selected_run = run.and_then(|id| self.store.runs().iter().find(|run| run.id == id));
        let owner = match context {
            Some(context) => match self.store.task(context) {
                None => ResultOwner::OrdinaryPage,
                Some(task) if task.automation_id.is_some() => {
                    match self.store.automation_period_for_task(context) {
                        Some((automation, occurrence)) => ResultOwner::AutomationTask {
                            automation,
                            occurrence,
                            task: context,
                        },
                        None => ResultOwner::OrdinaryPage,
                    }
                }
                Some(_) => match selected_run {
                    Some(run)
                        if run.task_id == context
                            && !self.store.run_is_automation_owned(run) =>
                    {
                        ResultOwner::Task(context)
                    }
                    Some(run)
                        if !self.store.run_is_automation_owned(run)
                            && self.store.owning_research_goal(run.task_id) == Some(context) =>
                    {
                        ResultOwner::Goal { goal: context }
                    }
                    // A stale/unrelated run must not make the explicit context
                    // page unconstructible; keep the context and let its own
                    // resolver choose a valid result.
                    _ => ResultOwner::Task(context),
                },
            },
            None => ResultOwner::OrdinaryPage,
        };
        let run = selected_run
            .filter(|run| owner.allows_run(&self.store, run))
            .map(|run| run.id);
        self.install_route(Route::research_result(owner, run));
        self.navigation.selected_attempt = run;
    }

    /// Update only the selected run while keeping the current result owner.
    pub(crate) fn set_selected_run(&mut self, run: Option<u64>) {
        use crate::navigation_route::Route;
        let Route::ResearchResult(page) = self.nav.route() else {
            self.navigation.selected_attempt = run;
            return;
        };
        let owner = page.owner();
        self.install_route(Route::research_result(owner, run));
        self.navigation.selected_attempt = run;
    }

    /// Automation detail identity carried by the typed route.
    pub(crate) fn automation_detail_id(&self) -> Option<u64> {
        match self.nav.route() {
            crate::navigation_route::Route::AutomationDetail(page) => Some(page.automation()),
            _ => None,
        }
    }
    /// Occurrence currently open in the period overlay, if any.
    pub(crate) fn period_overlay(&self) -> Option<u64> {
        match self
            .nav
            .overlay(crate::navigation_route::OverlayKind::AutomationPeriod)
        {
            Some(crate::navigation_route::Overlay::AutomationPeriod { occurrence, .. }) => {
                Some(occurrence)
            }
            _ => None,
        }
    }

    /// Push the period overlay over its explicit detail/results base. A period
    /// opened from anywhere else installs the owning detail page first.
    pub(crate) fn push_period_overlay(&mut self, occurrence: u64) -> bool {
        use crate::navigation_route::{Overlay, OverlayKind, Route};
        let Some(value) = self.store.occurrence(occurrence) else {
            return false;
        };
        let automation = value.automation_id;
        let base_ok = match self.nav.route() {
            Route::AutomationDetail(page) => page.automation() == automation,
            Route::AutomationResults(_) => true,
            _ => false,
        };
        if !base_ok {
            let route = Route::automation_detail(automation);
            if !self.install_route(route) {
                return false;
            }
            self.automation_ui.selected = Some(automation);
        }
        self.nav
            .push_overlay(
                &self.store,
                Overlay::AutomationPeriod {
                    automation,
                    occurrence,
                },
            )
            .is_ok()
    }

    pub(crate) fn close_period_overlay(&mut self) -> bool {
        self.nav
            .remove_overlay(
                &self.store,
                crate::navigation_route::OverlayKind::AutomationPeriod,
            )
            .is_ok()
    }

    pub(crate) fn clear_typed_overlays(&mut self) {
        self.nav.clear_overlays();
    }

    pub(crate) fn task_editor_overlay(&self) -> Option<u64> {
        match self
            .nav
            .overlay(crate::navigation_route::OverlayKind::TaskEditor)
        {
            Some(crate::navigation_route::Overlay::TaskEditor { task }) => Some(task),
            _ => None,
        }
    }

    pub(crate) fn editing(&self) -> bool {
        self.task_editor_overlay().is_some()
    }

    pub(crate) fn push_task_editor_overlay(&mut self, task: u64) -> bool {
        self.nav
            .push_overlay(
                &self.store,
                crate::navigation_route::Overlay::TaskEditor { task },
            )
            .is_ok()
    }

    pub(crate) fn close_task_editor_overlay(&mut self) -> bool {
        self.nav
            .remove_overlay(
                &self.store,
                crate::navigation_route::OverlayKind::TaskEditor,
            )
            .is_ok()
    }

    /// Settings over the currently selected automation detail.
    pub(crate) fn settings_overlay(&self) -> bool {
        matches!(
            self.nav
                .overlay(crate::navigation_route::OverlayKind::AutomationSettings),
            Some(crate::navigation_route::Overlay::AutomationSettings { .. })
        )
    }

    pub(crate) fn open_settings_overlay(&mut self) -> bool {
        use crate::navigation_route::{Overlay, Route};
        let Some(automation) = self.automation_ui.selected else {
            return false;
        };
        if !matches!(self.nav.route(), Route::AutomationDetail(page) if page.automation() == automation)
        {
            if !self.install_route(Route::automation_detail(automation)) {
                return false;
            }
        }
        let _ = self.close_period_overlay();
        self.nav
            .push_overlay(
                &self.store,
                Overlay::AutomationSettings { automation },
            )
            .is_ok()
    }

    pub(crate) fn close_settings_overlay(&mut self) -> bool {
        self.nav
            .remove_overlay(
                &self.store,
                crate::navigation_route::OverlayKind::AutomationSettings,
            )
            .is_ok()
    }

    pub(crate) fn planning_attempt_overlay(&self) -> Option<u64> {
        match self
            .nav
            .overlay(crate::navigation_route::OverlayKind::PlanningAttempt)
        {
            Some(crate::navigation_route::Overlay::PlanningAttempt { run, .. }) => Some(run),
            _ => None,
        }
    }

    pub(crate) fn push_planning_attempt_overlay(&mut self, run: u64) -> bool {
        let from_settings = self.settings_overlay();
        match self.nav.with_planning_attempt(&self.store, run, from_settings) {
            Ok(nav) => {
                self.nav = nav;
                self.automation_ui.planning_attempt_origin_settings = from_settings;
                true
            }
            Err(_) => false,
        }
    }

    pub(crate) fn close_planning_attempt_overlay(&mut self) -> bool {
        self.nav
            .remove_overlay(
                &self.store,
                crate::navigation_route::OverlayKind::PlanningAttempt,
            )
            .is_ok()
    }
    /// Production code uses `open_task`; tests that used to assign `selected`
    /// directly now open or close the typed task page through the same mutator.
    pub(crate) fn fixture_select_task(&mut self, task: Option<u64>) {
        match task {
            Some(task) => {
                let layout = self.steps_layout();
                self.set_task_identity(
                    task,
                    crate::navigation_route::TaskPanel::Overview,
                    layout,
                );
            }
            None => {
                if let crate::navigation_route::Route::ResearchTask(page) = self.nav.route() {
                    let base = page.base().clone();
                    self.set_scope_identity(base);
                }
            }
        }
    }

    fn initial_nav(store: &Store) -> crate::navigation_route::NavState {
        use crate::navigation_route::{NavState, Route};
        let preferred = Route::for_scope(
            &Scope::Project(1),
            AllStatus::All,
            Layout::List,
            None,
            20,
        );
        NavState::new(store, preferred).unwrap_or_else(|_| {
            let fallback =
                Route::for_scope(&Scope::All, AllStatus::All, Layout::List, None, 20);
            NavState::new(store, fallback).expect("All research is always a valid route")
        })
    }

    fn route_for_scope(&self, scope: &Scope) -> crate::navigation_route::Route {
        if matches!(scope, Scope::Runs) {
            let selection = match self.nav.route() {
                crate::navigation_route::Route::ResearchResult(page) => page.run(),
                _ => self.navigation.selected_attempt,
            }
            .filter(|id| {
                self.store
                    .runs()
                    .iter()
                    .find(|run| run.id == *id)
                    .is_some_and(|run| self.store.run_is_ordinary_result(run))
            });
            return crate::navigation_route::Route::research_result(
                crate::navigation_route::ResultOwner::OrdinaryPage,
                selection,
            );
        }
        if matches!(scope, Scope::Settings) {
            return crate::navigation_route::Route::Settings(
                crate::navigation_route::SettingsPage::new(self.settings_section()),
            );
        }
        crate::navigation_route::Route::for_scope(
            scope,
            self.all_status,
            self.layout,
            self.automation_ui.result_filter,
            self.automation_ui.history_limit,
        )
        .with_research_query(self.query())
    }

    /// Apply a route derived from the current page preferences. A rejected
    /// destination falls back predictably inside the same family and keeps the
    /// return history; the caller may surface the returned `false` as a notice.
    pub(crate) fn install_route(&mut self, route: crate::navigation_route::Route) -> bool {
        if self.nav.replace_route(&self.store, route.clone()).is_ok() {
            return true;
        }
        let fallback = self.fallback_route(&route);
        self.nav.reset_route_for_fallback(fallback);
        false
    }

    fn fallback_route(&self, route: &crate::navigation_route::Route) -> crate::navigation_route::Route {
        use crate::navigation_route::{ResearchScope, ResultOwner, Route};
        match route {
            Route::ResearchList(_) | Route::ResearchTask(_) => {
                Route::research_list(ResearchScope::All, self.all_status, Layout::List)
            }
            Route::ResearchResult(_) => {
                Route::research_result(ResultOwner::OrdinaryPage, None)
            }
            Route::AutomationDetail(_) | Route::AutomationEditor(_) => Route::AutomationList,
            Route::AutomationResults(_) => {
                Route::automation_results(None, self.automation_ui.history_limit.max(1))
            }
            Route::PausedProjects | Route::MarketResearch | Route::Settings(_) | Route::AutomationList => {
                Route::research_list(ResearchScope::All, self.all_status, Layout::List)
            }
        }
    }

    /// Set only the page-family identity. Production transitions that also
    /// change selection/overlays continue to use `navigate`; this helper exists
    /// for the remaining narrow writers and QA fixture entry points.
    pub(crate) fn set_scope_identity(&mut self, scope: Scope) {
        let route = self.route_for_scope(&scope);
        self.install_route(route);
    }

    /// Repair the typed route after Undo or a reload replaced the Store.
    /// A disappeared owner falls back inside the same family; return frames are
    /// retained and validated again when Back is used.
    fn reconcile_nav_after_store_change(&mut self) {
        use crate::navigation_route::Route;
        let route = self.nav.route().clone();
        if self.nav.replace_route(&self.store, route.clone()).is_ok() {
            return;
        }
        let fallback = self.fallback_route(&route);
        self.nav.reset_route_for_fallback(fallback);
    }

    pub fn navigate(&mut self, scope: Scope, window: &mut Window, cx: &mut Context<Self>) {
        self.park_current_comment(cx);
        self.park_research_composer(cx);
        let removed_modal_focus = self.period_overlay().is_some()
            || self.settings_overlay()
            || self.task_editor_overlay().is_some()
            || self.previous_versions_task.is_some()
            || self.research_save_impact.is_some();
        self.clear_navigation_overlays();
        if removed_modal_focus {
            window.focus(&self.focus, cx);
        }
        if scope == Scope::Runs {
            // `route_for_scope` keeps only an ordinary selected run; an
            // automation step cannot leak into the ordinary results page.
            self.reveal_selected_run();
        }
        self.save_feedback = None;
        self.show_run_details = false;
        self.show_run_result = true;
        self.research_model_picker = None;
        let project = match &scope {
            Scope::Project(id) => Some(*id),
            Scope::Topic(id) => self
                .store
                .topics()
                .iter()
                .find(|t| t.id == *id)
                .map(|t| t.project_id),
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
        let route = self.route_for_scope(&scope);
        if !self.install_route(route) {
            self.toast = "That page is no longer available. Returned to a safe parent.".into();
        }
        let _ = self.close_task_editor_overlay();
        self.draft = None;
        self.property = None;
        self.error.clear();
        self.clear_query();
        self.search.update(cx, |s, cx| s.set_value("", window, cx));
        if matches!(scope, Scope::Topic(_)) {
            window.focus(&self.focus, cx);
        }
        if self.scope() == Scope::Research {
            self.restore_research_composer(window, cx);
        }
        cx.notify();
    }
    pub fn open_collection_project(
        &mut self,
        project: u64,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_scope(Scope::Project(project), w, cx);
    }
    pub(crate) fn sidebar_scope(&self) -> Scope {
        self.nav
            .task()
            .or(self.selected().or(self.task_result_context()))
            .and_then(|id| self.store.task(id))
            .map(|task| Scope::Project(task.project_id))
            .unwrap_or_else(|| self.scope())
    }
    pub(crate) fn remember_active_project(&mut self, project: u64) {
        if self.store.projects().iter().any(|p| p.id == project) {
            self.store.set_last_active_project(Some(project));
        }
    }
    pub(crate) fn remember_dialog_opener(&mut self, window: &Window, cx: &App) {
        self.dialog_opener_focus = window.focused(cx).map(|opener| opener.downgrade());
    }

    pub(crate) fn restore_dialog_opener(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let opener = self
            .dialog_opener_focus
            .take()
            .and_then(|opener| opener.upgrade());
        let target = opener.unwrap_or_else(|| self.focus.clone());
        window.defer(cx, move |window, cx| {
            window.focus(&target, cx);
        });
    }

    pub(crate) fn restore_sidebar(&mut self) {
        self.expanded_sidebar_projects = self
            .store
            .last_active_project()
            .filter(|id| self.store.projects().iter().any(|p| p.id == *id))
            .into_iter()
            .collect();
        self.collapsed_sidebar_topics = self.store.topics().iter().map(|t| t.id).collect();
        self.collapsed_sidebar_goals = self.store.tasks().iter().map(|t| t.id).collect();
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
        let removed_modal_focus = self.period_overlay().is_some()
            || self.settings_overlay()
            || self.task_editor_overlay().is_some()
            || self.previous_versions_task.is_some()
            || self.research_save_impact.is_some();
        if self.selected() != Some(id) {
            self.park_current_comment(cx);
        }
        if self.selected() != Some(id) && self.task_result_context() != Some(id) {
            self.detail_scroll.set_offset(point(px(0.), px(0.)));
        }
        self.clear_navigation_overlays();
        if removed_modal_focus {
            // The focused handle belonged to the removed overlay subtree; an
            // orphan FocusId would make app shortcuts fall back to the window
            // root and miss TaskApp's key handlers.
            window.focus(&self.focus, cx);
        }
        self.tabs.retain(|task| *task != id);
        self.tabs.push(id);
        self.show_task_details = false;
        self.expanded_request_brief = None;
        self.show_task_activity = false;
        let (panel, steps_layout) = match self.nav.route() {
            crate::navigation_route::Route::ResearchTask(page) if page.task() == id => {
                (page.panel(), page.steps_layout())
            }
            _ => (crate::navigation_route::TaskPanel::Overview, self.steps_layout()),
        };
        self.set_task_identity(id, panel, steps_layout);
        if self.selected() != Some(id) {
            self.toast = "That task is no longer available. Returned to a safe parent.".into();
        }
        self.navigation
            .recent_surfaces
            .insert(id, crate::navigation_context::RecentSurface::Task);
        self.reveal_task_project(id);
        self.sidebar_reveal = (!self.store.task_is_archived(id)).then_some(id);
        let _ = self.close_task_editor_overlay();
        self.draft = None;
        self.property = None;
        self.error.clear();
        self.restore_task_comment(id, window, cx);
        cx.notify();
    }
    pub fn title(&self) -> String {
        self.scope_title(&self.scope())
    }
    pub(crate) fn scope_title(&self, scope: &Scope) -> String {
        match scope {
            Scope::All => "All research".into(),
            Scope::Project(id) => self
                .store
                .projects()
                .iter()
                .find(|p| p.id == *id)
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            Scope::Topic(id) => self
                .store
                .topics()
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
        if self.store.task_is_archived(t.id) != (self.scope() == Scope::Archive) {
            return false;
        }
        match self.scope() {
            Scope::All => self
                .all_status
                .matches(&self.store.task_status(t.id, self.todo_batch.as_ref())),
            Scope::Project(id) => t.project_id == id,
            Scope::Topic(id) => self
                .store
                .task_topic(t.id)
                .is_some_and(|topic| topic.id == id),
            Scope::Cycle(c) => t.cycle.as_ref() == Some(&c),
            _ => true,
        }
    }
    pub fn visible_tasks(&self) -> Vec<Task> {
        let mut list: Vec<_> = self
            .store
            .tasks()
            .iter()
            .filter(|t| {
                t.parent_id.is_none()
                    && self.matches_scope(t)
                    && self.store.matches_query(t, &self.query())
                    && (matches!(self.scope(), Scope::All | Scope::Archive)
                        || self.show_done
                        || !self
                            .store
                            .task_status(t.id, self.todo_batch.as_ref())
                            .phase
                            .is_closed())
            })
            .cloned()
            .collect();
        list.sort_by(|a, b| self.store.compare_display_tasks(a, b));
        list
    }
    pub fn change_stage(&mut self, id: u64, stage: Stage, cx: &mut Context<Self>) {
        if let Err(error) =
            self.store
                .validate_manual_transition(id, stage, self.todo_batch.as_ref())
        {
            self.error = error;
            cx.notify();
            return;
        }
        if stage == Stage::InProgress {
            // Submit through the existing scheduler; never manufacture Running.
            let route = self.nav.route().clone();
            self.start_goal_batch(id, cx);
            self.install_route(route);
            if self.error.is_empty()
                && self.store.task_status(id, self.todo_batch.as_ref()).phase == Stage::InProgress
            {
                self.toast = format!(
                    "Submitted · {}",
                    self.store.task_status(id, self.todo_batch.as_ref()).label()
                );
            }
            cx.notify();
            return;
        }
        if self.store.task(id).is_some_and(|t| t.stage == stage) {
            return;
        }
        self.apply_task_stage(id, stage, crate::scoped_edit::StageIntent::Manual, cx);
    }
    pub fn begin_draft(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.store.task(id) else {
            return;
        };
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
        if !self.push_task_editor_overlay(id) {
            self.draft = None;
            self.error = "Open the task before editing it.".into();
            cx.notify();
            return;
        }
        self.error.clear();
        self.begin_task_edit_baseline(cx);
        title.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }
    pub fn save_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        // Bool adapter for the existing caller API. The typed outcome remains
        // internal; UI save requests use `request_save_draft`.
        self.request_save_draft(window, cx)
    }
    /// Scoped undo only needs the edited goal to be idle. Unrelated active or
    /// queued runs must not block a safe task-edit inverse.
    fn scoped_undo_busy_reason_for_task(&self, task: u64) -> Option<String> {
        let goal = self.store.owning_research_goal(task).unwrap_or(task);
        let pending = self.todo_batch.as_ref().is_some_and(|batch| {
            batch.items.iter().any(|id| {
                batch.contains_pending(*id)
                    && (*id == task
                        || *id == goal
                        || self.store.owning_research_goal(*id) == Some(goal))
            })
        });
        let run_is_relevant = |run_id: u64| {
            self.store
                .runs()
                .iter()
                .find(|run| run.id == run_id)
                .is_some_and(|run| {
                    run.task_id == task
                        || run.task_id == goal
                        || self.store.owning_research_goal(run.task_id) == Some(goal)
                })
        };
        let active = self.active_runs.iter().any(|run| run_is_relevant(run.id))
            || self
                .queued_runs
                .iter()
                .any(|(run_id, _)| run_is_relevant(*run_id));
        if pending || active {
            return Some("Stop or finish work for this task before undoing this edit.".into());
        }
        None
    }

    /// Reason the next Undo entry cannot run now. Legacy snapshots keep the
    /// global submitted-work protection; scoped task edits only wait for work
    /// that belongs to the same goal.
    fn undo_references_reserved_material(&self, entry: &UndoRecord) -> bool {
        if self.material_cleanup_reserved.is_empty() {
            return false;
        }
        let references = match entry {
            UndoRecord::Material(undo) => undo.reference_paths(),
            UndoRecord::Settings(_)
            | UndoRecord::TaskEdit(_)
            | UndoRecord::Comment(_)
            | UndoRecord::Domain(_) => Vec::new(),
        };
        references
            .iter()
            .any(|path| self.material_cleanup_reserved.contains(path))
    }

    pub(crate) fn undo_block_reason(&self) -> Option<String> {
        let global_blocked = !self.active_runs.is_empty()
            || !self.queued_runs.is_empty()
            || self
                .todo_batch
                .as_ref()
                .is_some_and(|batch| batch.items.iter().any(|id| batch.contains_pending(*id)));
        if self.pending_material_job.is_some() {
            return Some("Material verification is still in progress. Try again shortly.".into());
        }
        if self
            .undo
            .peek()
            .is_some_and(|entry| self.undo_references_reserved_material(entry))
        {
            return Some("Material cleanup is still in progress. Try again shortly.".into());
        }
        match self.undo.peek() {
            Some(UndoRecord::Domain(_)) if global_blocked => {
                Some("Stop or finish submitted work before undoing changes.".into())
            }
            Some(UndoRecord::TaskEdit(undo)) => self.scoped_undo_busy_reason_for_task(undo.id()),
            Some(UndoRecord::Comment(undo)) => self.scoped_undo_busy_reason_for_task(undo.id()),
            Some(UndoRecord::Material(undo)) => self.scoped_undo_busy_reason_for_task(undo.id()),
            _ => None,
        }
    }

    pub fn undo_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let global_blocked = !self.active_runs.is_empty()
            || !self.queued_runs.is_empty()
            || self
                .todo_batch
                .as_ref()
                .is_some_and(|b| b.items.iter().any(|id| b.contains_pending(*id)));
        if self.pending_material_job.is_some() {
            self.error = "Material verification is still in progress. Try again shortly.".into();
            cx.notify();
            return;
        }
        if self
            .undo
            .peek()
            .is_some_and(|entry| self.undo_references_reserved_material(entry))
        {
            self.error = "Material cleanup is still in progress. Try again shortly.".into();
            cx.notify();
            return;
        }
        match self.undo.peek() {
            None => return,
            Some(UndoRecord::Domain(_)) if global_blocked => {
                self.error = "Stop or finish submitted work before undoing changes.".into();
                cx.notify();
                return;
            }
            Some(UndoRecord::TaskEdit(undo)) => {
                if let Some(reason) = self.scoped_undo_busy_reason_for_task(undo.id()) {
                    self.error = reason;
                    cx.notify();
                    return;
                }
            }
            Some(UndoRecord::Comment(undo)) => {
                if let Some(reason) = self.scoped_undo_busy_reason_for_task(undo.id()) {
                    self.error = reason;
                    cx.notify();
                    return;
                }
            }
            Some(UndoRecord::Material(undo)) => {
                if let Some(reason) = self.scoped_undo_busy_reason_for_task(undo.id()) {
                    self.error = reason;
                    cx.notify();
                    return;
                }
            }
            Some(UndoRecord::Settings(_)) | Some(UndoRecord::Domain(_)) => {}
        }
        let Some(entry) = self.undo.pop() else {
            return;
        };
        match entry {
            UndoRecord::TaskEdit(undo) => {
                let mut staged = self.store.clone();
                if let Err(error) = crate::scoped_edit::undo_task_edit(&mut staged, &undo) {
                    self.undo.push_task_edit(undo);
                    self.error = match error {
                        crate::scoped_edit::ScopedEditError::Rejected(message)
                        | crate::scoped_edit::ScopedEditError::Conflict(message) => message,
                    };
                    cx.notify();
                    return;
                }
                if let Err(error) = self.save_staged_store(&staged, crate::live_save_app::LiveCompletion::None, cx) {
                    self.undo.push_task_edit(undo);
                    self.storage_error = error.clone();
                    self.show_save_error(error, cx);
                    return;
                }
                if !self.live_persistence_active() {
                    self.adopt_saved_store(staged);
                }
                self.pending_task_edit = None;
                self.pending_comment_edit = None;
                self.research_save_impact = None;
                self.undo_invalidation = None;
                crate::appearance::apply(self.store.appearance(), Some(window), cx);
                self.tabs.retain(|id| self.store.task(*id).is_some());
                self.reconcile_nav_after_store_change();
                self.draft = None;
                let _ = self.close_task_editor_overlay();
                self.property = None;
                self.refresh_research_inputs(window, cx);
                self.refresh_global_model_input(window, cx);
                self.toast = "Last change undone".into();
                self.error.clear();
                cx.notify();
            }
            UndoRecord::Comment(undo) => {
                let mut staged = self.store.clone();
                if let Err(error) = crate::scoped_edit::undo_comment_edit(&mut staged, &undo) {
                    self.undo.push_comment(undo);
                    self.error = match error {
                        crate::scoped_edit::ScopedEditError::Rejected(message)
                        | crate::scoped_edit::ScopedEditError::Conflict(message) => message,
                    };
                    cx.notify();
                    return;
                }
                if let Err(error) = self.save_staged_store(&staged, crate::live_save_app::LiveCompletion::None, cx) {
                    self.undo.push_comment(undo);
                    self.storage_error = error.clone();
                    self.show_save_error(error, cx);
                    return;
                }
                if !self.live_persistence_active() {
                    self.adopt_saved_store(staged);
                }
                self.pending_task_edit = None;
                self.pending_comment_edit = None;
                self.research_save_impact = None;
                self.undo_invalidation = None;
                crate::appearance::apply(self.store.appearance(), Some(window), cx);
                self.refresh_research_inputs(window, cx);
                self.refresh_global_model_input(window, cx);
                self.toast = "Last change undone".into();
                self.error.clear();
                cx.notify();
            }
            UndoRecord::Settings(undo) => {
                let mut staged = self.store.clone();
                if let Err(error) = crate::scoped_settings::undo_settings_change(&mut staged, &undo) {
                    self.undo.push_settings(undo);
                    self.error = match error {
                        crate::scoped_settings::SettingsError::Rejected(message) => message.into(),
                        crate::scoped_settings::SettingsError::NoChange => {
                            "Nothing to undo".into()
                        }
                    };
                    cx.notify();
                    return;
                }
                if let Err(error) = self.save_staged_store(&staged, crate::live_save_app::LiveCompletion::None, cx) {
                    self.undo.push_settings(undo);
                    self.storage_error = error.clone();
                    self.show_save_error(error, cx);
                    return;
                }
                if !self.live_persistence_active() {
                    self.adopt_saved_store(staged);
                }
                self.undo_invalidation = None;
                crate::appearance::apply(self.store.appearance(), None, cx);
                self.toast = "Last change undone".into();
                self.error.clear();
                cx.notify();
            }
            UndoRecord::Material(undo) => {
                let remove = matches!(undo, crate::scoped_material::MaterialUndo::Remove(_));
                if remove {
                    if !self.submit_material_undo_verify(undo, cx) {
                        // The pending operation keeps ownership; a submit failure
                        // did not consume the inverse.
                    }
                } else if !self.apply_material_undo(undo.clone(), cx) {
                    self.undo.push_material(undo);
                    self.error = if self.error.is_empty() {
                        "Material undo did not run.".into()
                    } else {
                        self.error.clone()
                    };
                }
            }
            UndoRecord::Domain(undo) => {
                if let Err(error) = crate::scoped_domain::apply_undo(&mut self.store, &undo) {
                    self.undo.push_domain(undo);
                    self.error = match error {
                        crate::scoped_edit::ScopedEditError::Rejected(message)
                        | crate::scoped_edit::ScopedEditError::Conflict(message) => message,
                    };
                    cx.notify();
                    return;
                }
                let acceptance = match self.save_current_store_durably(
                    crate::live_save_app::LiveCompletion::None,
                    cx,
                ) {
                    Ok(acceptance) => acceptance,
                    Err(error) => {
                        // Restore the state the inverse just replaced; the entry
                        // is kept so the user can retry once storage recovers.
                        let redo = undo.invert();
                        let _ = crate::scoped_domain::apply_undo(&mut self.store, &redo);
                        self.undo.push_domain(undo);
                        self.storage_error = error.clone();
                        self.show_save_error(error, cx);
                        return;
                    }
                };
                self.undo_invalidation = None;
                self.tabs.retain(|id| self.store.task(*id).is_some());
                self.reconcile_nav_after_store_change();
                if let Scope::Project(id) = self.scope()
                    && !self.store.projects().iter().any(|p| p.id == id)
                {
                    self.set_scope_identity(Scope::All);
                }
                if let Scope::Topic(id) = self.scope()
                    && !self.store.topics().iter().any(|t| t.id == id)
                {
                    self.set_scope_identity(Scope::All);
                }
                self.draft = None;
                let _ = self.close_task_editor_overlay();
                self.property = None;
                self.refresh_research_inputs(window, cx);
                self.refresh_global_model_input(window, cx);
                self.toast = if acceptance.is_durable() {
                    "Last change undone".into()
                } else {
                    "Undoing…".into()
                };
                self.error.clear();
                cx.notify();
            }
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
        let loaded = storage.acquire_lock().and_then(|lock| {
            storage.upgrade_paths(|_| {})?;
            storage.load().map(|store| (lock, store))
        });
        self.attach_loaded_storage(storage, loaded, window, cx);
        if self.storage_error.is_empty() {
            self.check_interrupted_sessions(cx);
        }
    }
    pub(crate) fn attach_loaded_storage(
        &mut self,
        storage: Storage,
        loaded: Result<(std::fs::File, Option<Store>), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.attach_loaded_storage_live(storage, loaded, None, window, cx);
    }

    /// Attach a workspace using the incremental v4 backend when `live` is
    /// present. Migration/load already happened on the background thread; this
    /// path attaches the single ordered worker and never performs a legacy
    /// synchronous save before the first edit.
    pub(crate) fn attach_loaded_storage_live(
        &mut self,
        storage: Storage,
        loaded: Result<(std::fs::File, Option<Store>), String>,
        live: Option<crate::persistence_live::LiveStartup>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match loaded {
            Ok((lock, store)) => {
                if let Some(live) = live {
                    self.reset_live_state();
                    self._storage_lock = Some(lock);
                    self.hydrate_loaded_store(store, window, cx);
                    self.live_required = true;
                    self.live_attach =
                        Some((live.path.clone(), live.root.clone(), live.incarnation));
                    self.live_recovery_path = Some(live.path.clone());
                    // Durability in live mode is proven by the ordered receipt.
                    // Do not clone the whole history into a second UI copy; an
                    // empty baseline can only understate durability, never claim
                    // a write that did not happen.
                    self.last_saved = Store::empty();
                    let intent_path = live.path.with_extension("dispatch-intents.json");
                    match self.attach_dispatch_admission(
                        &live.path,
                        &live.root,
                        live.incarnation,
                        intent_path,
                    ) {
                        Ok(()) => {
                            self.live_save_enabled = true;
                            self.live_workspace = Some(live.incarnation);
                            self.start_history_reader(
                                &live.path,
                                &live.root,
                                live.incarnation,
                            );
                            self.storage_error.clear();
                            self.start_pump(cx);
                        }
                        Err(error) => {
                            self.live_save_enabled = false;
                            self.storage_error = format!(
                                "Incremental persistence could not start. The workspace is read-only until the worker reattaches; legacy writes are disabled: {error}"
                            );
                            self.toast = "Persistence unavailable · changes are not being saved".into();
                        }
                    }
                    self.storage = Some(storage);
                    if let Some(warning) = live.recovery_warning.clone() {
                        self.storage_error = warning.clone();
                        self.toast = warning;
                    }
                } else {
                    self.live_required = false;
                    self.live_attach = None;
                    self._storage_lock = Some(lock);
                    self.hydrate_loaded_store(store, window, cx);
                    match storage.save(&self.store) {
                        Ok(()) => {
                            self.store =
                                std::mem::replace(&mut self.store, Store::empty()).into_durably_saved();
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
                }
                // Stop intents are loaded and reconciled before recovery can
                // enqueue or start anything. A malformed journal blocks recovery.
                let stop_ready = self.prepare_stop_recovery(cx);
                if stop_ready {
                    self.recover_automations(cx);
                } else {
                    self.cancel_automation_timer();
                }
            }
            Err(e) => {
                self.storage_error = e;
                self.toast = "Storage unavailable · changes are not saved".into();
            }
        }
    }

    fn hydrate_loaded_store(
        &mut self,
        store: Option<Store>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.store = store.unwrap_or_else(Store::empty);
        self.automation_ui.clear_store_caches();
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
        self.set_scope_identity(Scope::All);
        crate::appearance::apply(self.store.appearance(), Some(window), cx);
        if self.store.opencode_path().is_empty() {
            self.store.set_opencode_path(opencode::detect_executable());
        }
        let path = self.store.opencode_path().clone();
        self.executable_input
            .update(cx, |s, cx| s.set_value(path, window, cx));
        let codex_path = self.store.research_config().codex_path.clone();
        self.codex_executable_input
            .update(cx, |s, cx| s.set_value(codex_path, window, cx));
        self.refresh_research_inputs(window, cx);
        self.refresh_global_model_input(window, cx);
    }

    // Every notified domain mutation is saved. Streaming runs are coalesced to two writes/second.
    /// Persist a staged store without mutating the live one. Used by creation
    /// forms so a rejected write cannot leave a half-created request behind.
    ///
    /// In live mode the validated typed delta is merged into the live store and
    /// submitted to the ordered worker. `Pending` means the data is in memory
    /// and queued but not yet durable: the caller keeps its form open and defers
    /// saved status, effects and acknowledgement-only UI work.
    pub(crate) fn save_staged_store(
        &mut self,
        staged: &Store,
        completion: crate::live_save_app::LiveCompletion,
        cx: &mut Context<Self>,
    ) -> Result<crate::live_save_app::SaveAcceptance, String> {
        #[cfg(feature = "qa")]
        self.qa_storage_writes.set(self.qa_storage_writes.get() + 1);
        if self.live_persistence_active() {
            self.live_can_accept_primary()?;
            self.merge_staged_store(staged)?;
            let acceptance = self.submit_live_snapshot(completion, cx)?;
            return Ok(acceptance);
        }
        if self.live_required {
            return Err(
                "Incremental persistence is unavailable. Nothing was written to a legacy file; retry the save when the worker is attached."
                    .into(),
            );
        }
        let storage = self.storage.as_ref().ok_or_else(|| {
            "Workspace storage is unavailable. Nothing was saved; the request stays editable."
                .to_string()
        })?;
        storage.save(staged).map_err(|error| error.to_string())?;
        self.complete_live_now(completion, cx);
        Ok(crate::live_save_app::SaveAcceptance::Durable)
    }

    /// Adopt a store that was just confirmed durable. Runtime state that lives
    /// outside `Store` (active runs, queues) is untouched.
    pub(crate) fn adopt_saved_store(&mut self, staged: Store) {
        self.store = staged.into_durably_saved();
        self.last_saved = self.store.clone();
        self.storage_error.clear();
    }

    pub(crate) fn persist_if_changed(&mut self, force: bool) -> bool {
        let _total = crate::persistence_metrics::Span::start("persist.total");
        crate::persistence_metrics::bump("persist.calls", 1);
        // A pending command never owns the live store. Background completions
        // may still be saved while it waits; its base becomes stale and retry
        // reprepares from the current store instead of replaying staged data.
        {
            let _span = crate::persistence_metrics::Span::start("persist.reconcile");
            self.store.reconcile_goal_approvals();
        }
        let unchanged = !self.store.has_tracked_changes();
        if unchanged {
            crate::persistence_metrics::bump("persist.skip.unchanged", 1);
            return false;
        }
        if !force
            && !self.active_runs.is_empty()
            && self.last_save.elapsed() < Duration::from_millis(500)
        {
            crate::persistence_metrics::bump("persist.skip.throttle", 1);
            return false;
        }
        if self.live_persistence_active() {
            self.persist_live_followup();
            return false;
        }
        if self.live_required {
            self.storage_error =
                "Incremental persistence is unavailable; changes are kept in memory and retry after the worker reattaches."
                    .into();
            return false;
        }
        let Some(storage) = &self.storage else {
            return false;
        };
        let receipt = match self.store.drain_store_delta() {
            Ok(receipt) => receipt,
            Err(error) => {
                self.storage_error = error;
                return false;
            }
        };
        self.last_save = Instant::now();
        let result = {
            let _span = crate::persistence_metrics::Span::start("persist.storage_save");
            storage.save(&self.store)
        };
        match result {
            Ok(()) => {
                crate::persistence_metrics::bump("persist.commit.ok", 1);
                self.store.retire_store_delta(&receipt);
                {
                    let _span = crate::persistence_metrics::Span::start("persist.clone_last_saved");
                    self.last_saved = self.store.clone();
                }
                let had_error = !self.storage_error.is_empty();
                self.storage_error.clear();
                if had_error {
                    self.toast = "Saved · local storage recovered".into();
                }
                had_error
            }
            Err(e) => {
                crate::persistence_metrics::bump("persist.commit.failed", 1);
                let changed = self.storage_error != e;
                self.storage_error = e;
                changed
            }
        }
    }
    /// True while the in-memory workspace differs from the last confirmed write.
    pub fn has_unsaved_changes(&self) -> bool {
        self.store.has_tracked_changes()
    }

    /// Explicit retry for the footer's "Not saved" state. A plain save is not
    /// enough because the in-memory store already equals the last confirmed
    /// snapshot, so `persist_if_changed` would skip the write.
    pub fn retry_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_task_edit.is_some() {
            self.retry_task_editor_save(window, cx);
            cx.notify();
            return;
        }
        if self.pending_comment_edit.is_some() {
            self.retry_comment_save(window, cx);
            cx.notify();
            return;
        }
        if self.pending_material_edit.is_some() {
            self.retry_material_save(window, cx);
            cx.notify();
            return;
        }
        if self.pending_commit.is_some()
            || self.pending_stop.is_some()
            || self.pending_dispatch.is_some()
        {
            self.retry_pending_saves(cx);
            cx.notify();
            return;
        }
        if !self.stop_intents.is_empty() || self.stop_journal_error.is_some() {
            let ready = self.prepare_stop_recovery(cx);
            if ready {
                self.recover_automations(cx);
            }
            cx.notify();
            return;
        }
        if self.live_required && !self.live_persistence_active() {
            match self.retry_live_attachment(cx) {
                Ok(()) => {
                    self.toast = "Persistence reattached".into();
                }
                Err(error) => {
                    self.storage_error = error;
                    self.show_save_error(self.storage_error.clone(), cx);
                }
            }
            cx.notify();
            return;
        }
        let receipt = match self.store.drain_store_delta() {
            Ok(receipt) => receipt,
            Err(error) => {
                self.storage_error = error;
                cx.notify();
                return;
            }
        };
        let result = self
            .storage
            .as_ref()
            .map(|storage| storage.save(&self.store));
        match result {
            Some(Ok(())) => {
                self.store.retire_store_delta(&receipt);
                self.last_saved = self.store.clone();
                self.storage_error.clear();
                self.error.clear();
                self.toast = "Saved to local workspace".into();
            }
            Some(Err(error)) => {
                self.storage_error = error;
            }
            None => {
                self.storage_error =
                    "Workspace storage is unavailable. Reopen the workspace to reconnect.".into();
            }
        }
        cx.notify();
    }
    pub fn task_running(&self, id: u64) -> bool {
        self.store.runs().iter().any(|r| {
            r.project_plan.is_none()
                && r.task_id == id
                && matches!(
                    r.status.as_str(),
                    "Queued" | "Running" | "Cancelling" | "Recovering"
                )
        })
    }
    pub fn goal_busy(&self, goal: u64) -> bool {
        self.store.tasks().iter().any(|t| {
            t.research.as_ref().is_some_and(|r| r.goal_id == goal) && self.task_running(t.id)
        })
    }
    pub fn run_goal(&self, run_id: u64) -> Option<u64> {
        self.store
            .runs()
            .iter()
            .find(|r| r.id == run_id)
            .and_then(|r| self.store.task(r.task_id))
            .and_then(|t| t.research.as_ref())
            .map(|r| r.goal_id)
    }
    pub fn open_directory(&mut self, project: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.remember_dialog_opener(window, cx);
        self.directory_project = Some(project);
        self.error.clear();
        let path = self
            .store
            .projects()
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
        let Some(target) = self.directory_project else {
            self.show_save_error("Choose a project first.", cx);
            return;
        };
        let directory = path.to_string_lossy().into_owned();
        if self
            .apply_domain_command_undoable(
                crate::scoped_domain::DomainCommand::ProjectDirectory {
                    project: target,
                    directory,
                },
                "Project folder saved to local workspace",
                cx,
            )
            .is_some()
        {
            self.directory_project = None;
            self.error.clear();
        }
    }
    pub fn discovering_codex_models(&self) -> bool {
        self.codex_discovery.is_some()
    }
    pub fn discover_codex_models(&mut self, cx: &mut Context<Self>) {
        if self.codex_discovery.is_some() {
            return;
        }
        let executable = PathBuf::from(&self.store.research_config().codex_path);
        let (tx, rx) = mpsc::channel();
        self.codex_discovery = Some(rx);
        self.codex_status = "Checking Codex…".into();
        std::thread::spawn(move || {
            let _ = tx.send(crate::codex::discover(&executable));
        });
        self.start_pump(cx);
        cx.notify();
    }
    #[cfg(feature = "qa")]
    pub(crate) fn complete_discovery_for_qa(
        &mut self,
        rx: mpsc::Receiver<(PathBuf, Discovery)>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.discovery = Some(rx);
        self.poll_runtime(cx)
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
        self.store.set_opencode_path(executable.to_string_lossy().into_owned());
        let directory = self
            .store
            .projects()
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
        let _ = self.start_run_with_navigation(id, true, cx);
    }

    /// Timer ticks, queue continuations and recovery imports submit work without
    /// changing the route, selection, drafts or open dialogs.
    pub(crate) fn start_run_background(&mut self, id: u64, cx: &mut Context<Self>) {
        let _ = self.start_run_with_navigation(id, false, cx);
    }

    /// Batch callers need to distinguish a durably queued run from an accepted
    /// admission whose run record is intentionally not inserted until ACK.
    pub(crate) fn start_run_background_submission(
        &mut self,
        id: u64,
        cx: &mut Context<Self>,
    ) -> RunSubmission {
        self.start_run_with_navigation(id, false, cx)
    }

    fn start_run_with_navigation(
        &mut self,
        id: u64,
        navigate: bool,
        cx: &mut Context<Self>,
    ) -> RunSubmission {
        let awaiting_before = self.dispatch_awaiting.as_ref().map(|id| id.run_id);
        if self.route_automation_task(id, cx) {
            return RunSubmission::Rejected;
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
        } else if let Some(dispatch) = self.start_run_inner(id, cx) {
            if navigate {
                self.open_run_progress(dispatch);
            }
            return RunSubmission::Dispatched(dispatch);
        }
        if !self.error.is_empty() {
            self.task_start_feedback = Some((id, self.error.clone()));
            cx.notify();
            return RunSubmission::Rejected;
        }
        // A live admission was accepted but has not written the run record yet.
        if let Some(awaiting) = self.dispatch_awaiting.as_ref()
            && Some(awaiting.run_id) != awaiting_before
        {
            let submission = RunSubmission::PendingAdmission {
                run_id: awaiting.run_id,
                automation_owned: self
                    .store
                    .task(id)
                    .and_then(|task| task.automation_id)
                    .is_some(),
            };
            cx.notify();
            return submission;
        }
        cx.notify();
        RunSubmission::Rejected
    }
    fn start_run_inner(&mut self, id: u64, cx: &mut Context<Self>) -> Option<RunDispatch> {
        if !self.ready_to_run(cx) || self.task_running(id) {
            return None;
        }
        let mut run = match self.store.prepare_run(id) {
            Ok(run) => run,
            Err(e) => {
                self.error = e;
                cx.notify();
                return None;
            }
        };
        let files = match self.prepare_run_materials(id, &mut run) {
            Ok(files) => files,
            Err(e) => {
                self.error = e;
                cx.notify();
                return None;
            }
        };
        self.launch_run(run, files, cx)
    }

    /// Open the progress surface requested by an explicit user command. Only
    /// callers that handled a user action use this; dispatch itself never routes.
    pub(crate) fn open_run_progress(&mut self, dispatch: RunDispatch) {
        self.follow_logs = true;
        self.show_run_result = true;
        self.show_run_details = false;
        if dispatch.automation_owned {
            self.set_scope_identity(Scope::AutomationRuns);
        } else {
            self.set_result_selection(None, Some(dispatch.run_id));
            self.reveal_selected_run();
        }
    }

    /// Validate and prepare a run without changing the store, queue, undo or
    /// feedback. The existing launch checks live here; the durable dispatch
    /// commit and queue effect happen separately.
    pub(crate) fn prepare_run_dispatch(
        &mut self,
        mut run: AgentRun,
        files: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Option<PreparedRunDispatch> {
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
            return None;
        }
        if self.store.run_is_archived(&run) {
            self.error = "Restore the archived topic or goal before executing.".into();
            cx.notify();
            return None;
        }
        if self.recovery_blocks_run(&run) {
            self.error =
                "Check the previous session in Paused projects before starting again.".into();
            cx.notify();
            return None;
        }
        if let Err(error) = self.check_workspace_directory(std::path::Path::new(&run.directory)) {
            self.error = error.clone();
            self.task_start_feedback = Some((run.task_id, error));
            cx.notify();
            return None;
        }
        if self.store.runs().iter().any(|r| {
            r.task_id == run.task_id
                && r.project_plan == run.project_plan
                && (run.project_plan.is_none()
                    || self.store.request_for_run(r.id).map(|q| q.id)
                        == self.store.request_for_run(run.id).map(|q| q.id))
                && matches!(r.status.as_str(), "Queued" | "Running" | "Cancelling")
        }) {
            return None;
        }
        if run.project_plan.is_none() {
            run.workspace_source = match self.store.workspace_source(run.task_id) {
                Ok(source) => source,
                Err(error) => {
                    self.error = error;
                    cx.notify();
                    return None;
                }
            };
            match self.store.execution_directory(run.task_id) {
                Ok(path) => {
                    if let Err(error) = self.check_workspace_directory(&path) {
                        self.error = error;
                        cx.notify();
                        return None;
                    }
                    if let Err(e) = if run.workspace_source.is_none() {
                        std::fs::create_dir_all(&path)
                    } else {
                        Ok(())
                    } {
                        self.error = format!("Cannot create task working folder: {e}");
                        cx.notify();
                        return None;
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
                    return None;
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
        Some(PreparedRunDispatch { run, files })
    }

    pub(crate) fn launch_intent_for(&self, run: &AgentRun) -> LaunchIntent {
        if let Some(owner) = &run.planning_owner {
            return LaunchIntent::RequestPlan {
                request_id: owner.request_id,
            };
        }
        if let Some(project) = run.project_plan {
            return LaunchIntent::ProjectPlan {
                project_id: project,
            };
        }
        if let Some(occurrence) = run.automation_occurrence_id {
            return LaunchIntent::Occurrence(occurrence);
        }
        LaunchIntent::Task(run.task_id)
    }

    pub(crate) fn launch_run(
        &mut self,
        run: AgentRun,
        files: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Option<RunDispatch> {
        let intent = self.launch_intent_for(&run);
        self.launch_run_with_intent(run, files, intent, cx)
    }

    pub(crate) fn launch_run_with_intent(
        &mut self,
        run: AgentRun,
        files: Vec<PathBuf>,
        intent: LaunchIntent,
        cx: &mut Context<Self>,
    ) -> Option<RunDispatch> {
        let prepared = self.prepare_run_dispatch(run, files, cx)?;
        self.commit_prepared_dispatch(prepared, intent, cx)
    }

    /// Commit the run and its occurrence link in one staged store, then let the
    /// existing scheduler queue effect start. No run record is written by the
    /// effect itself.
    pub(crate) fn commit_prepared_dispatch(
        &mut self,
        prepared: PreparedRunDispatch,
        intent: LaunchIntent,
        cx: &mut Context<Self>,
    ) -> Option<RunDispatch> {
        // Resolve an earlier user command before preparing this dispatch. If it
        // still cannot save, preserve that pending intent and do not replace it
        // with a dispatch failure.
        if self.pending_commit.is_some()
            || self.pending_stop.is_some()
            || self.pending_dispatch.is_some()
        {
            self.retry_pending_saves(cx);
            if self.pending_commit.is_some()
                || self.pending_stop.is_some()
                || self.pending_dispatch.is_some()
            {
                self.error =
                    "Resolve the previous pending change before dispatching this run.".into();
                cx.notify();
                return None;
            }
        }
        let run_id = prepared.run.id;
        let task_id = prepared.run.task_id;
        let run_occurrence = prepared.run.automation_occurrence_id;
        let intended_occurrence = match &intent {
            LaunchIntent::Occurrence(id) => Some(*id),
            LaunchIntent::ReportRegeneration { occurrence, .. } => *occurrence,
            LaunchIntent::Task(_)
            | LaunchIntent::RequestPlan { .. }
            | LaunchIntent::ProjectPlan { .. } => None,
        };
        if run_occurrence != intended_occurrence {
            self.error = "The prepared run ownership does not match its dispatch intent.".into();
            cx.notify();
            return None;
        }
        // Report regeneration for an automation period is still automation-owned;
        // never route it to ordinary Research results or the Research slot.
        let automation_owned = run_occurrence.is_some();
        let command = match &intent {
            LaunchIntent::Occurrence(occurrence) => crate::domain_command::Command::DispatchRun {
                occurrence: Some(*occurrence),
                run: prepared.run,
            },
            LaunchIntent::RequestPlan { request_id } => {
                crate::domain_command::Command::DispatchRequestPlan {
                    request_id: *request_id,
                    run: prepared.run,
                }
            }
            LaunchIntent::ReportRegeneration {
                source_id,
                style,
                occurrence,
            } => crate::domain_command::Command::DispatchRegeneration {
                occurrence: *occurrence,
                source_id: *source_id,
                style: style.clone(),
                run: prepared.run,
            },
            LaunchIntent::Task(_) | LaunchIntent::ProjectPlan { .. } => {
                crate::domain_command::Command::DispatchRun {
                    occurrence: None,
                    run: prepared.run,
                }
            }
        };
        if self.dispatch_admission.is_some() {
            return self.admit_prepared_dispatch(command, intent, prepared.files, cx);
        }
        match self.execute_command(command, cx) {
            Ok(_) => {
                if let Some(entry) = self.queued_runs.iter_mut().find(|(id, _)| *id == run_id) {
                    entry.1 = prepared.files;
                }
                self.error.clear();
                self.toast = if automation_owned {
                    "Automation period queued · waiting for an automation slot".into()
                } else {
                    "Task queued · waiting for a research execution slot".into()
                };
                self.start_pump(cx);
                cx.notify();
                Some(RunDispatch {
                    run_id,
                    automation_owned,
                })
            }
            Err(error) => {
                // Retain only the typed intent. The next attempt rebuilds the
                // run/ids from the current authoritative request/source.
                self.pending_commit = None;
                self.pending_dispatch = Some(PendingDispatch {
                    intent,
                    files: prepared.files,
                });
                self.error = error.clone();
                self.task_start_feedback = Some((task_id, error));
                cx.notify();
                None
            }
        }
    }
    fn run_is_automation_owned(&self, run_id: u64) -> bool {
        self.store
            .runs()
            .iter()
            .find(|run| run.id == run_id)
            .is_some_and(|run| self.store.run_is_automation_owned(run))
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
                .runs()
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
        for occurrence in self.store.all_occurrences() {
            if occurrence.status == crate::automation::OccurrenceStatus::Running {
                occupied.insert(occurrence.id);
            }
        }
        for active in &self.active_runs {
            if let Some(occurrence) = self
                .store
                .runs()
                .iter()
                .find(|run| run.id == active.id)
                .and_then(|run| run.automation_occurrence_id)
            {
                occupied.insert(occurrence);
            }
        }
        for run in self.store.runs() {
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
            let research_full = self.research_slots_used() >= self.store.concurrency_limit();
            let automation_full =
                self.automation_slots_used() >= self.store.automation_concurrency_limit();
            if research_full && automation_full {
                break;
            }
            let next = self.queued_runs.iter().position(|(id, _)| {
                let candidate = self.store.runs().iter().find(|r| r.id == *id).unwrap();
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
                        self.store.all_occurrences().iter().any(|occurrence| {
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
                    if self
                        .active_runs
                        .iter()
                        .filter_map(|active| {
                            self.store.runs().iter().find(|run| run.id == active.id)
                        })
                        .chain(self.store.runs().iter().filter(|run| {
                            run.status == "Recovering" && self.store.is_recovery_candidate(run)
                        }))
                        .filter(|run| run.automation_occurrence_id.is_none())
                        .any(|run| {
                            run.project_plan.is_none()
                                && self.store.execution_topic(run.task_id) != topic
                        })
                    {
                        return false;
                    }
                    let front = self
                        .todo_batch
                        .as_ref()
                        .and_then(|batch| batch.pending_topics(&self.store).first().copied())
                        .or_else(|| {
                            self.queued_runs
                                .iter()
                                .filter_map(|(id, _)| {
                                    self.store.runs().iter().find(|run| {
                                        run.id == *id
                                            && run.project_plan.is_none()
                                            && run.automation_occurrence_id.is_none()
                                    })
                                })
                                .next()
                                .map(|run| self.store.execution_topic(run.task_id))
                        });
                    if front.is_some_and(|front| front != topic) {
                        return false;
                    }
                }
                if candidate.auto_retry_attempt > 0 {
                    if candidate.retry_delay_remaining(chrono::Utc::now().timestamp()) > 0 {
                        return false;
                    }
                    if self.todo_batch.as_ref().is_some_and(|batch| {
                        batch.run_ids.contains_key(id)
                            && batch.state != crate::batch::BatchState::Running
                    }) {
                        return false;
                    }
                }
                !self
                    .store
                    .runs()
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
                            .runs()
                            .iter()
                            .find(|r| r.id == a.id)
                            .is_some_and(|running| self.store.run_conflicts(candidate, running))
                    })
            });
            let Some(index) = next else {
                break;
            };
            let (id, files) = self.queued_runs.remove(index);
            let index = self.store.runs().iter().position(|r| r.id == id).unwrap();
            let run = self.store.runs()[index].clone();
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
                    self.store.research_signature(run.task_id).as_ref()
                        == Some(&run.input_signature)
                        && !self.store.research_pending_approval(run.task_id)
                        && self.store.task(run.task_id).is_some_and(|t| {
                            !t.archived && !matches!(t.stage, Stage::Backlog | Stage::Cancelled)
                        })
                } else {
                    false
                };
            if !valid || self.store.run_is_archived(&run) {
                {
                    let mut run = self.store.run_at_mut(index).unwrap();
                    run.status = "Stale".into();
                    run.ended_at = Some(opencode::timestamp());
                    run.logs.push(
                        "[queue] Inputs or approval changed before execution; review and start again."
                            .into(),
                    );
                }
                self.complete_research_run(id, cx);
                continue;
            }
            if let Err(error) = self.check_workspace_directory(std::path::Path::new(&run.directory))
            {
                {
                    let mut run = self.store.run_at_mut(index).unwrap();
                    run.status = "Blocked".into();
                    run.ended_at = Some(opencode::timestamp());
                    run.logs.push(format!("[workspace] {error}"));
                    self.task_start_feedback = Some((run.task_id, error.clone()));
                }
                self.complete_research_run(id, cx);
                continue;
            }
            if let Some(source) = run.workspace_source.clone() {
                if let Err(error) = crate::scheduler::migrate_workspace(&source, &run.directory) {
                    {
                        let mut run = self.store.run_at_mut(index).unwrap();
                        run.status = "Blocked".into();
                        run.ended_at = Some(opencode::timestamp());
                        run.logs.push(format!("[workspace] {error}"));
                        self.task_start_feedback = Some((run.task_id, error.clone()));
                    }
                    self.error = error.clone();
                    self.persist_if_changed(true);
                    self.complete_research_run(id, cx);
                    continue;
                }
                self.store.run_at_mut(index).unwrap().workspace_source = None;
                self.store.run_at_mut(index).unwrap().logs.push("[workspace] Legacy research files copied to the isolated goal folder; original files preserved.".into());
            }
            if let Err(error) =
                crate::research::write_dependency_files(&self.store, &self.store.runs()[index])
            {
                {
                    let mut run = self.store.run_at_mut(index).unwrap();
                    run.status = "Blocked".into();
                    run.ended_at = Some(opencode::timestamp());
                    run.logs.push(format!("[workspace] {error}"));
                    self.task_start_feedback = Some((run.task_id, error.clone()));
                }
                self.persist_if_changed(true);
                self.complete_research_run(id, cx);
                continue;
            }
            self.store.refresh_queued_browser_control(index);
            let automation_occurrence = self.store.runs()[index].automation_occurrence_id;
            let automation_research_kind = self.store.runs()[index].research_kind;
            if let Some(occurrence_id) = automation_occurrence {
                let Some(occurrence) = self.store.occurrence(occurrence_id).cloned() else {
                    {
                        let mut run = self.store.run_at_mut(index).unwrap();
                        run.status = "Blocked".into();
                        run.ended_at = Some(opencode::timestamp());
                        run.logs.push(
                            "[automation] The occurrence is missing; nothing was started.".into(),
                        );
                    }
                    self.persist_if_changed(true);
                    self.complete_research_run(id, cx);
                    continue;
                };
                let now = Self::automation_now();
                let regeneration = occurrence.status
                    == crate::automation::OccurrenceStatus::Succeeded
                    && automation_research_kind == Some(crate::research::ResearchKind::Advise);
                if self.store.runs()[index]
                    .deadline_at
                    .is_some_and(|deadline| now >= deadline)
                {
                    {
                        let mut run = self.store.run_at_mut(index).unwrap();
                        run.status = "Stale".into();
                        run.ended_at = Some(opencode::timestamp());
                        run.logs.push(
                            "[automation] Report regeneration exceeded its own time limit before starting; the previous report is preserved."
                                .into(),
                        );
                    }
                    self.persist_if_changed(true);
                    self.complete_research_run(id, cx);
                    continue;
                }
                if !regeneration && occurrence.first_start_expired(now) {
                    {
                        let mut run = self.store.run_at_mut(index).unwrap();
                        run.status = "Stale".into();
                        run.ended_at = Some(opencode::timestamp());
                        run.logs.push("[automation] The queued request exceeded its allowed delay and was skipped.".into());
                    }
                    let _ = self.store.occurrence_mut(occurrence_id).map(|mut value| value.transition(
                        crate::automation::OccurrenceStatus::Skipped,
                        Some(crate::automation::OccurrenceReason::new(crate::automation::SkipCode::Expired, "The queued request exceeded its allowed delay before a slot was available.")),
                    ));
                    self.persist_if_changed(true);
                    self.complete_research_run(id, cx);
                    continue;
                }
                if !regeneration
                    && occurrence
                        .deadline_at
                        .is_some_and(|deadline| now >= deadline)
                {
                    {
                        let mut run = self.store.run_at_mut(index).unwrap();
                        run.status = "Stale".into();
                        run.ended_at = Some(opencode::timestamp());
                        run.logs.push("[automation] The occurrence's total time limit was reached before this step started.".into());
                    }
                    let _ = self.store.occurrence_mut(occurrence_id).map(|mut value| {
                        value.transition(
                            crate::automation::OccurrenceStatus::Failed,
                            Some(crate::automation::OccurrenceReason::new(
                                crate::automation::SkipCode::Expired,
                                "The automation's total time limit was reached.",
                            )),
                        )
                    });
                    self.store
                        .note_automation_failure(occurrence.automation_id, now);
                    self.persist_if_changed(true);
                    self.complete_research_run(id, cx);
                    continue;
                }
                if !regeneration
                    && occurrence.status != crate::automation::OccurrenceStatus::Running
                {
                    let first_start = occurrence.actual_started_at.is_none();
                    let mark_error = match self.store.occurrence_mut(occurrence_id) {
                        Some(mut value) => value.mark_running(now).err(),
                        None => None,
                    };
                    if let Some(error) = mark_error {
                        {
                            let mut run = self.store.run_at_mut(index).unwrap();
                            run.status = "Blocked".into();
                            run.ended_at = Some(opencode::timestamp());
                            run.logs.push(format!("[automation] {error}"));
                        }
                        self.store
                            .mark_automation_attention(occurrence.automation_id, error.clone());
                        self.persist_if_changed(true);
                        self.complete_research_run(id, cx);
                        continue;
                    }
                    if first_start {
                        self.automation_started
                            .insert(occurrence_id, std::time::Instant::now());
                    }
                }
            }
            let run = self.store.runs()[index].clone();
            let request = opencode::Request {
                agent: None,
                browser_executable: (run.agent == "Codex"
                    && run.research_kind == Some(crate::research::ResearchKind::Collect)
                    && !self
                        .store
                        .owning_research_goal(run.task_id)
                        .is_some_and(|goal| self.store.research_browser_handed_off(goal)))
                .then(|| PathBuf::from(&self.store.research_config().ego_path)),
                files,
                session_id: run.session_id.clone(),
                executable: PathBuf::from(self.store.agent_path(&run.agent)),
                directory: PathBuf::from(&run.directory),
                model: run.model.clone(),
                prompt: run.prompt.clone(),
                title: run.task_title.clone(),
            };
            self.store.run_at_mut(index).unwrap().status = "Running".into();
            self.store.run_at_mut(index).unwrap().started_at = opencode::timestamp();
            self.persist_if_changed(true);
            if !self.storage_error.is_empty() {
                self.store.run_at_mut(index).unwrap().status = "Queued".into();
                self.store.run_at_mut(index).unwrap().started_at.clear();
                self.queued_runs.insert(0, (id, request.files));
                break;
            }
            let started = match self.store.runs()[index].agent.as_str() {
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
                    {
                        let mut run = self.store.run_at_mut(index).unwrap();
                        run.status = "Failed".into();
                        run.ended_at = Some(opencode::timestamp());
                        run.logs.push(format!("[error] {e}"));
                    }
                    self.error = e;
                    self.complete_research_run(id, cx);
                }
            }
            self.persist_if_changed(true);
        }
    }
    pub fn cancel_run_id(&mut self, id: u64, cx: &mut Context<Self>) {
        // A run can be durable but still between its receipt and the launch
        // claim/queue effect. It is in neither `active_runs` nor `queued_runs`,
        // so cancel it through the admission path; otherwise the later claim
        // would launch after Stop.
        let already_scheduled = self.active_runs.iter().any(|active| active.id == id)
            || self.queued_runs.iter().any(|(queued, _)| *queued == id);
        if !already_scheduled && self.cancel_pending_dispatch(id, cx) {
            return;
        }
        if let Some(goal) = self.run_goal(id) {
            self.research_sequences.remove(&goal);
        }
        if let Some(active) = self.active_runs.iter().find(|a| a.id == id) {
            active.handle.cancel();
            if let Some(mut run) = self.store.run_mut(id) {
                run.status = "Cancelling".into();
                run.logs.push("[run] Cancellation requested".into());
            }
        } else if let Some(index) = self.queued_runs.iter().position(|(run, _)| *run == id) {
            self.queued_runs.remove(index);
            if let Some(mut run) = self.store.run_mut(id) {
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
    pub(crate) fn runtime_pump_active(&self) -> bool {
        self.pumping
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
        if self.startup.workspace_pending.is_some() {
            cx.notify();
        }
        self.poll_startup(cx);
        self.poll_pending_workspace_switch(cx);
        self.poll_dispatch_admissions(cx);
        self.poll_live_retry();
        self.poll_history_io(cx);
        self.drain_live_deferred_runtime(cx);
        self.finish_live_quit_drain(cx);
        self.enforce_active_run_deadlines(Self::automation_now(), cx);
        self.poll_research_io(cx);
        self.poll_material_jobs(cx);
        self.poll_recovery(cx);
        if let Some(result) = self
            .codex_discovery
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.codex_discovery = None;
            match result {
                Ok(result) => {
                    self.codex_version = Some(result.version);
                    self.codex_models = result.models;
                    self.codex_status = result.status;
                }
                Err(error) => {
                    self.codex_status = error;
                    self.codex_models.clear();
                }
            }
            cx.notify();
        }
        let discovery = self.discovery.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some((executable, result)) = discovery {
            self.discovery = None;
            match result {
                Ok((version, models)) => {
                    self.opencode_version =
                        Some((executable.to_string_lossy().into_owned(), version));
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
                self.toast =
                    format!("Browser paused; research will continue with evidence gaps. {message}");
            }
            if let Some(mut run) = self.store.run_mut(active.id) {
                if record_events(&mut *run, &mut active.had_error, events) {
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
        if self.queued_runs.iter().any(|(id, _)| {
            self.store
                .runs()
                .iter()
                .any(|run| run.id == *id && run.auto_retry_attempt > 0)
        }) {
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
            // Material verification/cleanup is active external work. Human
            // approval (`pending_material_edit`) is intentionally excluded.
            || self.pending_material_job.is_some()
            || self.recovery_pending()
            || self.quit_requested
            || self.window_close_requested
            || self.dispatch_pump_pending()
            || self.live_needs_pump();
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
            if let Some(mut run) = self.store.run_mut(active.id) {
                let finished = record_events(&mut *run, &mut active.had_error, events);
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
            self.finalize_research_run_without_ui(active.id);
        }
        for (id, _) in self.queued_runs.drain(..) {
            if let Some(mut run) = self.store.run_mut(id) {
                run.status = "Interrupted".into();
                run.ended_at = Some(opencode::timestamp());
            }
        }
        self.persist_if_changed(true);
    }
}
pub(crate) fn record_events(run: &mut AgentRun, had_error: &mut bool, events: Vec<opencode::Event>) -> bool {
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
                run.logs
                    .push(format!("[browser] Access paused · {message}"));
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
                    if run.status == "Succeeded" {
                        "Exited normally (process only)"
                    } else if run.status == "Blocked"
                        && run
                            .logs
                            .iter()
                            .any(|line| line == crate::research::BROWSER_HANDOFF_LOG)
                    {
                        "Stopped for browser handoff (research result pending)"
                    } else {
                        &run.status
                    },
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
    fn scoped_undo_preserves_task_comment_material_and_domain_entries_in_order() {
        let directory = std::env::temp_dir().join(format!(
            "t13-undo-mixed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let mut store = crate::domain::Store::empty();
        store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into_owned();
        let task = store
            .create_task("Mixed task", 1, crate::domain::Stage::Planned, None)
            .unwrap();
        let input = crate::scoped_edit::TaskEditInput {
            id: task,
            title: "Edited task".into(),
            description: String::new(),
            labels: vec![],
            due_date: String::new(),
            cycle: None,
            priority: crate::domain::Priority::None,
            bucket: String::new(),
            project: Some(1),
            stage: Some(crate::domain::Stage::Planned),
            stage_intent: crate::scoped_edit::StageIntent::Editor,
        };
        let plan = crate::scoped_edit::prepare_task_edit(&store, &input).unwrap();
        let task_edit = crate::scoped_edit::apply_task_edit(&mut store, &plan).unwrap();
        let comment_plan =
            crate::scoped_edit::prepare_comment_edit(&store, task, "Comment").unwrap();
        let comment = crate::scoped_edit::apply_comment_edit(&mut store, &comment_plan).unwrap();
        let attachment = crate::attachments::import_bytes(
            &directory,
            "material.pdf",
            b"%PDF-1.4\nmixed undo fixture\n",
        )
        .unwrap();
        let imported =
            crate::scoped_material::imported_copy(&directory, task, &attachment).unwrap();
        let material_plan = crate::scoped_material::prepare_material_add(
            &store,
            &directory,
            task,
            vec![attachment.clone()],
            vec![imported],
        )
        .unwrap();
        let material =
            crate::scoped_material::apply_material_add(&mut store, &directory, &material_plan)
                .unwrap();

        let domain = crate::scoped_domain::DomainCommand::TopicCreate {
            project: 1,
            name: "Scoped context".into(),
        }
        .execute(&mut store)
        .expect("topic fixture")
        .undo;
        let topic = store
            .topics()
            .iter()
            .find(|candidate| candidate.name == "Scoped context")
            .expect("topic fixture")
            .id;

        let mut stack = super::UndoStack::default();
        stack.push_task_edit(task_edit.undo);
        stack.push_comment(comment.undo);
        stack.push_material(crate::scoped_material::MaterialUndo::Add(material.undo));
        stack.push_domain(domain.clone());
        assert!(super::undo_entry_references_reserved(
            stack
                .entries
                .iter()
                .find(|entry| matches!(entry, super::UndoRecord::Material(_)))
                .unwrap(),
            &[attachment.relative_path.clone()],
        ));
        assert_eq!(stack.len(), 4);
        assert!(matches!(stack.peek(), Some(super::UndoRecord::Domain(_))));
        assert!(matches!(stack.pop(), Some(super::UndoRecord::Domain(_))));
        assert!(matches!(stack.peek(), Some(super::UndoRecord::Material(_))));
        assert!(
            stack
                .material_reference_paths()
                .iter()
                .any(|path| path == &attachment.relative_path)
        );
        assert!(matches!(stack.pop(), Some(super::UndoRecord::Material(_))));
        assert!(matches!(stack.pop(), Some(super::UndoRecord::Comment(_))));
        assert!(matches!(stack.pop(), Some(super::UndoRecord::TaskEdit(_))));
        assert!(stack.pop().is_none());
        assert!(stack.is_empty());
        // The captured domain change removes only the topic the fixture created.
        let mut restored = store;
        crate::scoped_domain::apply_undo(&mut restored, &domain).unwrap();
        assert!(!restored.topics().iter().any(|candidate| candidate.id == topic));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn requested_cancellation_is_not_a_retryable_failure() {
        let mut store = crate::domain::Store::mock();
        store.simulate_run(1).unwrap();
        let mut run = store.run_at_mut(0).unwrap();
        run.status = "Cancelling".into();
        super::record_events(
            &mut *run,
            &mut false,
            vec![crate::opencode::Event::Finished {
                code: Some(1),
                cancelled: false,
                error: None,
            }],
        );
        assert_eq!(run.status, "Cancelled");
    }
    #[test]
    fn browser_handoff_stays_blocked_after_process_cleanup() {
        let mut store = crate::domain::Store::mock();
        store.simulate_run(1).unwrap();
        let mut run = store.run_at_mut(0).unwrap();
        assert!(super::record_events(
            &mut *run,
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
        let mut run = store.run_at_mut(0).unwrap();
        run.status = "Running".into();
        assert!(super::record_events(
            &mut *run,
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
        assert!(
            run.logs
                .iter()
                .any(|s| s == "[process] Exited normally (process only) · exit 0")
        );
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
