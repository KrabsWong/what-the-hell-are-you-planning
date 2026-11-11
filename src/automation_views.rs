//! Automation management surface: list, three-step creation, settings, history and results.
use crate::appearance::palette;
use crate::views::{chip, col, label, muted, observed, row, selectable};
use crate::{
    app::{Scope, TaskApp},
    automation::{
        AutomationDefinition, AutomationOccurrence, AutomationPolicy, AutomationShape,
        AutomationTemplate, DefinitionStatus, OccurrenceStatus, TimeWindowState,
    },
    automation_activity::{compare_newest_first, occurrence_activity},
    automation_execution::build_quick_template,
    automation_schedule::{CatchUp, Schedule, ScheduleSpec, ScheduledTime, parse_timezone},
    controls::*,
    domain::Store,
};
use gpui_kit::{
    InteractiveElement,
    base::FocusTrapElement,
    component::Disableable,
    component::button::ButtonVariants,
    component::input::{Input, InputState, Textarea, TextareaState},
    component::menu::{DropdownMenu, PopupMenuItem},
    prelude::*,
    *,
};
use std::{cell::RefCell, collections::{HashMap, HashSet}, io::Read};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomationKind {
    Interval,
    Daily,
    Weekly,
}
impl AutomationKind {
    pub const ALL: [Self; 3] = [Self::Interval, Self::Daily, Self::Weekly];
    pub fn label(self) -> &'static str {
        match self {
            Self::Interval => "Fixed interval",
            Self::Daily => "Every day",
            Self::Weekly => "Selected weekdays",
        }
    }
}

/// Weekdays selected by a fresh automation: Monday through Friday.
pub const DEFAULT_WEEKDAYS: [bool; 7] = [true, true, true, true, true, false, false];

/// Explicit defaults for a new automation form. Only values that legitimately
/// depend on the machine are passed in; every other field is fixed here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomationDraftDefaults {
    pub timezone: String,
}
impl AutomationDraftDefaults {
    pub fn system() -> Self {
        Self {
            timezone: system_timezone(),
        }
    }
}

/// Everything the three-step creation/edit wizard owns. Building one value for
/// a fresh form or an existing definition keeps policy fields from leaking
/// between sessions and lets tests assert the whole form at once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomationDraft {
    pub shape: AutomationShape,
    pub kind: AutomationKind,
    /// Interval input unit: `false` is minutes, `true` is hours.
    pub interval_in_hours: bool,
    pub catch_up: CatchUp,
    pub weekdays: [bool; 7],
    pub name: String,
    pub objective: String,
    pub expected: String,
    pub interval: String,
    pub hour: String,
    pub minute: String,
    pub timezone: String,
    pub delay: String,
    pub timeout: String,
    pub failures: String,
    pub retention: String,
    pub plan: Option<AutomationTemplate>,
}

impl AutomationDraft {
    /// A fresh creation session: Quick shape, Daily 10:00 in the system time
    /// zone, LatestOnly, Monday–Friday, minute interval unit and default limits.
    pub fn new(defaults: &AutomationDraftDefaults) -> Self {
        Self {
            shape: AutomationShape::Quick,
            kind: AutomationKind::Daily,
            interval_in_hours: false,
            catch_up: CatchUp::LatestOnly,
            weekdays: DEFAULT_WEEKDAYS,
            name: String::new(),
            objective: String::new(),
            expected: String::new(),
            interval: "10".into(),
            hour: "10".into(),
            minute: "00".into(),
            timezone: defaults.timezone.clone(),
            delay: String::new(),
            timeout: "30".into(),
            failures: "3".into(),
            retention: "30".into(),
            plan: None,
        }
    }

    /// Load an existing definition without inheriting values from whatever form
    /// happened to be open before it.
    pub fn from_definition(
        definition: &AutomationDefinition,
        defaults: &AutomationDraftDefaults,
    ) -> Self {
        let mut draft = Self::new(defaults);
        draft.shape = definition.shape;
        draft.catch_up = definition.policy.catch_up;
        draft.name = definition.name.clone();
        draft.objective = definition.objective.clone();
        draft.expected = definition.expected_result.clone();
        draft.timezone = definition.schedule.timezone.clone();
        draft.delay = (definition.policy.allowed_delay_seconds / 60).to_string();
        draft.timeout = (definition.policy.timeout_seconds / 60).to_string();
        draft.failures = definition.policy.max_consecutive_failures.to_string();
        draft.retention = definition.policy.log_retention_days.to_string();
        draft.plan = definition.template.clone();
        match &definition.schedule.spec {
            ScheduleSpec::Interval { minutes } => {
                draft.kind = AutomationKind::Interval;
                draft.interval_in_hours = *minutes >= 60 && minutes % 60 == 0;
                let value = if draft.interval_in_hours {
                    minutes / 60
                } else {
                    *minutes
                };
                draft.interval = value.to_string();
            }
            ScheduleSpec::Daily { hour, minute } => {
                draft.kind = AutomationKind::Daily;
                draft.hour = format!("{hour:02}");
                draft.minute = format!("{minute:02}");
            }
            ScheduleSpec::Weekly { days, hour, minute } => {
                draft.kind = AutomationKind::Weekly;
                draft.hour = format!("{hour:02}");
                draft.minute = format!("{minute:02}");
                draft.weekdays = std::array::from_fn(|index| days.contains(&(index as u8)));
            }
        }
        draft
    }
}

/// Session-only name colours for automation results. Hues follow the golden
/// angle so consecutive automations land far apart on the colour wheel, and the
/// walk is never persisted: colours change on the next start while every period
/// of one automation stays consistent this session. Rows share one neutral
/// surface; only the automation name carries its colour.
const GOLDEN_ANGLE: f32 = 137.50776;

fn session_seed() -> u64 {
    let mut bytes = [0u8; 8];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .is_ok()
    {
        return u64::from_ne_bytes(bytes);
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0)
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> (u32, u32, u32) {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue / 60.0;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (red, green, blue) = match sector as u32 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let offset = lightness - chroma / 2.0;
    let channel = |value: f32| ((value + offset) * 255.0).round() as u32;
    (channel(red), channel(green), channel(blue))
}

/// Supported result-row surfaces used by the session name tints.
/// Stored as 24-bit RGB; `contrast_against` converts them to opaque RGBA.
pub(crate) const LIGHT_RESULT_PANEL_RGB: u32 = 0xffffff;
pub(crate) const DARK_RESULT_PANEL_RGB: u32 = 0x202124;
/// WCAG AA contrast for ordinary text.
const RESULT_NAME_MIN_CONTRAST: f32 = 4.5;

fn srgb_channel_luminance(channel: u32) -> f32 {
    let value = channel as f32 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Relative luminance of an **opaque RGBA** color (`0xRRGGBBAA`).
pub(crate) fn relative_luminance(rgba: u32) -> f32 {
    0.2126 * srgb_channel_luminance((rgba >> 24) & 0xff)
        + 0.7152 * srgb_channel_luminance((rgba >> 16) & 0xff)
        + 0.0722 * srgb_channel_luminance((rgba >> 8) & 0xff)
}

/// Contrast between an opaque **RGBA** text color and a 24-bit **RGB**
/// background. The background is widened to opaque RGBA internally.
pub(crate) fn contrast_against(text_rgba: u32, background_rgb: u32) -> f32 {
    let text = relative_luminance(text_rgba);
    let background = relative_luminance((background_rgb << 8) | 0xff);
    let (lighter, darker) = if text >= background {
        (text, background)
    } else {
        (background, text)
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// Build one session name colour. Hue and saturation are preserved; when the
/// preferred lightness cannot reach AA contrast against the supported row
/// surface, lightness moves only as far as needed (darker on light panels,
/// lighter on dark panels).
fn result_tint_color(hue: f32, saturation: f32, preferred_lightness: f32, dark: bool) -> ResultTint {
    let background = if dark {
        DARK_RESULT_PANEL_RGB
    } else {
        LIGHT_RESULT_PANEL_RGB
    };
    let mut lightness = preferred_lightness.clamp(0.08, 0.94);
    let mut color = {
        let (red, green, blue) = hsl_to_rgb(hue, saturation, lightness);
        (red << 24) | (green << 16) | (blue << 8) | 0xff
    };
    if dark {
        while contrast_against(color, background) < RESULT_NAME_MIN_CONTRAST && lightness < 0.94 {
            lightness = (lightness + 0.01).min(0.94);
            let (red, green, blue) = hsl_to_rgb(hue, saturation, lightness);
            color = (red << 24) | (green << 16) | (blue << 8) | 0xff;
        }
    } else {
        while contrast_against(color, background) < RESULT_NAME_MIN_CONTRAST && lightness > 0.08 {
            lightness = (lightness - 0.01).max(0.08);
            let (red, green, blue) = hsl_to_rgb(hue, saturation, lightness);
            color = (red << 24) | (green << 16) | (blue << 8) | 0xff;
        }
    }
    ResultTint { text: color }
}

#[derive(Clone, Copy)]
pub(crate) struct ResultTint {
    /// Solid text colour for the automation name.
    pub text: u32,
}

pub(crate) struct ResultTints {
    base_hue: f32,
    next: RefCell<usize>,
    /// The golden-angle walk position assigned to each automation. Colours are
    /// computed per call so a theme switch adjusts lightness.
    assigned: RefCell<HashMap<u64, usize>>,
}

impl ResultTints {
    fn new() -> Self {
        Self::seeded(session_seed())
    }

    fn seeded(seed: u64) -> Self {
        Self {
            base_hue: (seed % 36_000) as f32 / 100.0,
            next: RefCell::new(0),
            assigned: RefCell::new(HashMap::new()),
        }
    }

    /// The golden-angle walk maximises separation between the first few
    /// automations; rotating saturation and lightness keeps later picks
    /// distinguishable when hues eventually come close again. Light themes use
    /// darker text and dark themes use lighter text so names stay readable.
    fn tint(&self, automation: u64, dark: bool) -> ResultTint {
        let mut assigned = self.assigned.borrow_mut();
        let index = match assigned.get(&automation) {
            Some(index) => *index,
            None => {
                let mut next = self.next.borrow_mut();
                let index = *next;
                *next += 1;
                assigned.insert(automation, index);
                index
            }
        };
        let hue = (self.base_hue + index as f32 * GOLDEN_ANGLE) % 360.0;
        let saturation = [0.62, 0.74, 0.86][index % 3];
        let lightness = if dark {
            [0.72, 0.64, 0.78][(index / 3) % 3]
        } else {
            [0.38, 0.30, 0.44][(index / 3) % 3]
        };
        result_tint_color(hue, saturation, lightness, dark)
    }
}

pub(crate) enum AutomationSaveOutcome {
    Saved,
    Saving {
        input: crate::domain_command::AutomationDraftInput,
        subject: Option<u64>,
        form_epoch: u64,
    },
    Rejected(String),
}

/// A reviewed SaveDraft that must finish before exactly one planner request is
/// created. It is consumed only by the matching durable receipt.
#[derive(Clone, Debug, PartialEq)]
struct PendingAutomationPlanning {
    input: crate::domain_command::AutomationDraftInput,
    form_epoch: u64,
    automation: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlanningSaveIdentity {
    pub(crate) input: crate::domain_command::AutomationDraftInput,
    pub(crate) form_epoch: u64,
}

/// Exact Enable command waiting for its durable receipt. The completion closes
/// this wizard only if the same form/form epoch is still open.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AutomationEnableReceipt {
    /// The editor id before submission; None for a new unsaved creation.
    pub(crate) id: Option<u64>,
    pub(crate) form_epoch: u64,
    /// Exact form input persisted by this Enable. Same-form edits after the
    /// click must keep the wizard open.
    pub(crate) input: crate::domain_command::AutomationDraftInput,
}

/// Generic automation lifecycle receipt. Enable owns the wizard; pause/resume
/// only settle the matching detail/form and never overwrite a newer surface.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AutomationCommandReceipt {
    Enable(AutomationEnableReceipt),
    Pause {
        id: u64,
        form_epoch: u64,
        creating: bool,
    },
    Resume {
        id: u64,
        form_epoch: u64,
        creating: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutomationPlanningStart {
    Started(u64),
    WaitingForDraft,
}

pub struct AutomationUi {
    pub selected: Option<u64>,
    pub creating: bool,
    /// Generation of the currently open wizard form. Reopening the same values
    /// creates a distinct identity for late-ACK safety.
    pub form_epoch: u64,
    pub editing: Option<u64>,
    pub step: u8,
    /// Generated automation project; never shown as a project choice.
    pub project: u64,
    pub shape: AutomationShape,
    /// Automation whose research plan is being generated right now.
    pub planning_automation: Option<u64>,
    /// Run id of the planning attempt allowed to update this wizard. A shape or
    /// identity change clears it so a stale completion cannot repopulate a
    /// discarded plan or move the wizard forward.
    pub planning_run: Option<u64>,
    /// SaveDraft receipt that must become durable before the planner starts.
    pending_planning: Option<PendingAutomationPlanning>,
    /// Planning-owned SaveDraft still in flight. Survives Back so its late
    /// receipt cannot be mistaken for an ordinary draft completion.
    pub(crate) planning_save_inflight: Option<PlanningSaveIdentity>,
    /// Enable/pause/resume command whose durable receipt must settle the UI.
    pub(crate) pending_command: Option<AutomationCommandReceipt>,
    /// The same obligation while its receipt is still in flight. Kept after the
    /// completion handoff so a repeated activation cannot be submitted.
    pub(crate) automation_command_inflight: Option<AutomationCommandReceipt>,
    pub show_advanced: bool,
    pub kind: AutomationKind,
    pub interval_in_hours: bool,
    pub catch_up: CatchUp,
    pub weekdays: [bool; 7],
    pub plan: Option<AutomationTemplate>,
    pub preview: Vec<i64>,
    pub preview_error: String,
    /// Whether closing that attempt should restore the Settings dialog that
    /// opened it. The automation list is the other valid origin.
    pub planning_attempt_origin_settings: bool,
    /// Scroll and focus restored when a planning attempt dialog closes.
    pub settings_scroll: ScrollHandle,
    /// Stable opener focus per planning-run Review control.
    pub planning_opener_focus: std::cell::RefCell<std::collections::HashMap<u64, FocusHandle>>,
    /// Attempt whose Review control must regain focus when its dialog closes.
    pub planning_attempt_return_run: Option<u64>,
    /// Automation whose periods the all-results list is limited to. `None`
    /// keeps every automation.
    pub result_filter: Option<u64>,
    pub history_limit: usize,
    pub(crate) result_order: std::cell::RefCell<ResultOrderCache>,
    pub(crate) run_index: std::cell::RefCell<RunIndexCache>,
    pub(crate) result_list: ListState,
    pub(crate) result_list_key: std::cell::RefCell<ResultListKey>,
    pub(crate) result_scroll_synced: std::cell::Cell<f32>,
    pub(crate) result_scroll_target: std::cell::Cell<Option<f32>>,
    pub(crate) result_scroll_attempts: std::cell::Cell<u8>,
    pub(crate) result_list_width: std::rc::Rc<std::cell::Cell<Option<f32>>>,
    pub pending_period: Option<u64>,
    pub show_technical: bool,
    pub show_completed_steps: bool,
    pub expanded_sources: HashSet<String>,
    pub technical_run: Option<u64>,
    pub technical_scroll: ScrollHandle,
    pub show_cleanup: bool,
    pub cleanup_preview: String,
    pub feedback: Option<Result<String, String>>,
    pub scroll: ScrollHandle,
    /// Scroll offset of the open period dialog body, restored by Back.
    pub dialog_scroll: ScrollHandle,
    pub(crate) report_readers: RefCell<HashMap<String, Entity<gpui_kit::base::text::TextViewState>>>,
    /// Session-only background tint per automation in the all-results list.
    pub result_tints: ResultTints,
    pub name: Entity<InputState>,
    pub objective: Entity<TextareaState>,
    pub expected: Entity<TextareaState>,
    pub interval: Entity<InputState>,
    pub hour: Entity<InputState>,
    pub minute: Entity<InputState>,
    pub timezone: Entity<InputState>,
    pub delay: Entity<InputState>,
    pub timeout: Entity<InputState>,
    pub failures: Entity<InputState>,
    pub retention: Entity<InputState>,
}

/// Bounded view cache for the global results order. `dirty_epoch` covers every
/// collection the order or the queued/waiting counts depend on.
#[derive(Default)]
pub(crate) struct ResultOrderCache {
    epoch: Option<u64>,
    filter: Option<u64>,
    pub(crate) indices: Vec<usize>,
    all_total: usize,
    filtered_total: usize,
    queued: usize,
    waiting: usize,
}

impl ResultOrderCache {
    pub(crate) fn clear(&mut self) {
        self.epoch = None;
        self.filter = None;
        self.indices.clear();
        self.all_total = 0;
        self.filtered_total = 0;
        self.queued = 0;
        self.waiting = 0;
    }

    pub(crate) fn refresh(&mut self, epoch: u64, filter: Option<u64>, store: &crate::domain::Store) {
        if self.epoch == Some(epoch) && self.filter == filter {
            return;
        }
        let occurrences = store.all_occurrences();
        self.all_total = occurrences.len();
        self.indices.clear();
        self.queued = 0;
        self.waiting = 0;
        for (index, occurrence) in occurrences.iter().enumerate() {
            if filter.is_some_and(|id| occurrence.automation_id != id) {
                continue;
            }
            self.queued += usize::from(occurrence.status == OccurrenceStatus::Queued);
            self.waiting += usize::from(occurrence.status == OccurrenceStatus::WaitingRetry);
            self.indices.push(index);
        }
        self.filtered_total = self.indices.len();
        self.indices.sort_by(|a, b| {
            compare_newest_first(&occurrences[*a], &occurrences[*b])
        });
        self.epoch = Some(epoch);
        self.filter = filter;
    }
}

/// Epoch-keyed positional run index. Hydration updates a run in place and does
/// not move positions, so the index stays correct without an epoch bump; any
/// real mutation that changes the run set moves the epoch and rebuilds it.
#[derive(Default)]
pub(crate) struct RunIndexCache {
    epoch: Option<u64>,
    index: Option<crate::result_presentation::RunIndex>,
}

impl RunIndexCache {
    pub(crate) fn clear(&mut self) {
        self.epoch = None;
        self.index = None;
    }

    pub(crate) fn ensure(&mut self, epoch: u64, store: &crate::domain::Store) {
        if self.epoch != Some(epoch) {
            self.epoch = Some(epoch);
            self.index = Some(crate::result_presentation::RunIndex::new(store));
        }
    }

    pub(crate) fn index(&self) -> &crate::result_presentation::RunIndex {
        self.index
            .as_ref()
            .expect("run index is built before use")
    }
}

#[derive(Debug, PartialEq)]
enum ResultScrollStep {
    Apply(f32),
    Settle { handle: f32, synced: f32 },
    WriteBack { handle: f32, synced: f32 },
    Idle,
}

/// Keep the list authoritative during an external restore. The handle value is
/// only written back after the list has caught up, so a stale ScrollHandle can
/// never overwrite the newest list offset. Restore is a relative pixel
/// `scroll_by`; `scroll_to(item_ix: 0, large offset_in_item)` is clamped by
/// ListState to the first item height and cannot express an arbitrary offset.
fn result_scroll_step(
    reset: bool,
    handle: f32,
    synced: f32,
    current: f32,
    target: Option<f32>,
    attempts: u8,
) -> ResultScrollStep {
    if reset {
        // A fresh list at the top needs no restore target. Keeping a pending
        // zero target would cancel the first wheel gestures on a newly mounted
        // route.
        if handle.abs() <= 0.5 {
            return ResultScrollStep::Settle {
                handle: 0.0,
                synced: 0.0,
            };
        }
        return ResultScrollStep::Apply(handle);
    }
    if let Some(target) = target {
        if target.abs() <= 0.5 {
            return ResultScrollStep::Settle {
                handle: current,
                synced: current,
            };
        }
        if (current - target).abs() <= 1.0 || attempts >= 6 {
            return ResultScrollStep::Settle {
                handle: current,
                synced: current,
            };
        }
        return ResultScrollStep::Apply(target);
    }
    if (handle - synced).abs() > 0.5 {
        return ResultScrollStep::Apply(handle);
    }
    if (current - synced).abs() > 0.5 {
        return ResultScrollStep::WriteBack {
            handle: current,
            synced: current,
        };
    }
    ResultScrollStep::Idle
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
struct ResultListKey {
    filter: Option<u64>,
    limit: usize,
    ids: Vec<u64>,
    items: usize,
    structure: u8,
}

impl ResultListKey {
    fn structure_of(items: &[AutomationResultsItem]) -> u8 {
        match items.last() {
            Some(AutomationResultsItem::EmptyAll) => 1,
            Some(AutomationResultsItem::EmptyFiltered) => 2,
            Some(AutomationResultsItem::LoadMore) => 4,
            _ => 0,
        }
    }
}

#[derive(Clone, Copy)]
enum AutomationResultsItem {
    Header,
    Row(usize),
    EmptyAll,
    EmptyFiltered,
    LoadMore,
}

/// Initial height hint for unmeasured rows so ListState knows the scrollable
/// extent before offscreen items are rendered. Real heights replace it.
const RESULT_ROW_HEIGHT_HINT: f32 = 56.;

struct AutomationResultsPlan {
    items: Vec<AutomationResultsItem>,
    rows: Vec<(u64, usize)>,
    total: usize,
    all_total: usize,
    queued: usize,
    waiting: usize,
    filter: Option<u64>,
    limit: usize,
}

impl AutomationResultsPlan {
    fn new(
        rows: Vec<(u64, usize)>,
        total: usize,
        all_total: usize,
        queued: usize,
        waiting: usize,
        filter: Option<u64>,
        limit: usize,
    ) -> Self {
        let mut items = vec![AutomationResultsItem::Header];
        if total == 0 {
            items.push(if all_total == 0 {
                AutomationResultsItem::EmptyAll
            } else {
                AutomationResultsItem::EmptyFiltered
            });
        } else {
            items.extend((0..rows.len()).map(AutomationResultsItem::Row));
            if total > limit {
                items.push(AutomationResultsItem::LoadMore);
            }
        }
        Self {
            items,
            rows,
            total,
            all_total,
            queued,
            waiting,
            filter,
            limit,
        }
    }
}

impl AutomationUi {
    pub fn new(window: &mut Window, cx: &mut Context<TaskApp>) -> Self {
        let input =
            |window: &mut Window, cx: &mut Context<TaskApp>, placeholder: &str, value: &str| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(placeholder.to_string())
                        .default_value(value.to_string())
                })
            };
        Self {
            selected: None,
            creating: false,
            form_epoch: 1,
            editing: None,
            step: 1,
            project: 1,
            shape: AutomationShape::Quick,
            planning_automation: None,
            planning_run: None,
            pending_planning: None,
            planning_save_inflight: None,
            pending_command: None,
            automation_command_inflight: None,
            show_advanced: false,
            kind: AutomationKind::Daily,
            interval_in_hours: false,
            catch_up: CatchUp::LatestOnly,
            weekdays: DEFAULT_WEEKDAYS,
            plan: None,
            preview: vec![],
            preview_error: String::new(),
            planning_attempt_origin_settings: false,
            settings_scroll: ScrollHandle::new(),
            planning_opener_focus: std::cell::RefCell::new(std::collections::HashMap::new()),
            planning_attempt_return_run: None,
            result_filter: None,
            history_limit: 20,
            result_order: std::cell::RefCell::new(ResultOrderCache::default()),
            run_index: std::cell::RefCell::new(RunIndexCache::default()),
            result_list: ListState::new(0, ListAlignment::Top, px(64.)),
            result_list_key: std::cell::RefCell::new(ResultListKey::default()),
            result_scroll_synced: std::cell::Cell::new(0.0),
            result_scroll_target: std::cell::Cell::new(None),
            result_scroll_attempts: std::cell::Cell::new(0),
            result_list_width: std::rc::Rc::new(std::cell::Cell::new(None)),
            pending_period: None,
            show_technical: false,
            show_completed_steps: false,
            expanded_sources: HashSet::new(),
            technical_run: None,
            technical_scroll: ScrollHandle::new(),
            show_cleanup: false,
            cleanup_preview: String::new(),
            feedback: None,
            scroll: ScrollHandle::new(),
            dialog_scroll: ScrollHandle::new(),
            report_readers: RefCell::new(HashMap::new()),
            result_tints: ResultTints::new(),
            name: input(window, cx, "e.g. Shenzhen fuel price", ""),
            objective: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Describe the recurring check: the question and its scope.")
                    .rows(5)
            }),
            expected: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Optional: what the answer should contain and how you will judge it useful.")
                    .rows(3)
            }),
            interval: input(window, cx, "10", "10"),
            hour: input(window, cx, "10", "10"),
            minute: input(window, cx, "00", "00"),
            timezone: input(window, cx, "Asia/Shanghai", &system_timezone()),
            delay: input(window, cx, "Default: one period (max 24 h)", ""),
            timeout: input(window, cx, "minutes", "30"),
            failures: input(window, cx, "3", "3"),
            retention: input(window, cx, "30", "30"),
        }
    }

    /// A workspace attach replaces the whole Store. Per-store epoch sums are not
    /// a workspace generation, so derived caches must not survive the swap.
    pub(crate) fn clear_store_caches(&mut self) {
        self.result_order.borrow_mut().clear();
        self.run_index.borrow_mut().clear();
        self.report_readers.borrow_mut().clear();
    }

    /// Stable focus handle for one attempt's Review control. Creating it lazily
    /// here keeps the same handle across renders and close/reopen cycles.
    fn planning_opener_focus(&self, run_id: u64, cx: &mut Context<TaskApp>) -> FocusHandle {
        self.planning_opener_focus
            .borrow_mut()
            .entry(run_id)
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    fn planning_opener_focus_for(&self, run_id: u64) -> Option<FocusHandle> {
        self.planning_opener_focus.borrow().get(&run_id).cloned()
    }

    /// Current wizard contents as the same value used to compare against a
    /// session baseline. Excludes the step number; content alone is dirt.
    pub(crate) fn form_draft(&self, cx: &App) -> AutomationDraft {
        AutomationDraft {
            shape: self.shape,
            kind: self.kind,
            interval_in_hours: self.interval_in_hours,
            catch_up: self.catch_up,
            weekdays: self.weekdays,
            name: self.name.read(cx).value().to_string(),
            objective: self.objective.read(cx).value().to_string(),
            expected: self.expected.read(cx).value().to_string(),
            interval: self.interval.read(cx).value().to_string(),
            hour: self.hour.read(cx).value().to_string(),
            minute: self.minute.read(cx).value().to_string(),
            timezone: self.timezone.read(cx).value().to_string(),
            delay: self.delay.read(cx).value().to_string(),
            timeout: self.timeout.read(cx).value().to_string(),
            failures: self.failures.read(cx).value().to_string(),
            retention: self.retention.read(cx).value().to_string(),
            plan: self.plan.clone(),
        }
    }

    /// Replace the whole form in one step. Callers construct either a fresh
    /// default draft or one loaded from a definition; nothing is left over from
    /// the previous session.
    pub fn apply_draft(
        &mut self,
        draft: &AutomationDraft,
        cx: &mut Context<TaskApp>,
        window: &mut Window,
    ) {
        self.shape = draft.shape;
        self.kind = draft.kind;
        self.interval_in_hours = draft.interval_in_hours;
        self.catch_up = draft.catch_up;
        self.weekdays = draft.weekdays;
        self.plan = draft.plan.clone();
        set_input(cx, &self.name, &draft.name, window);
        set_textarea(cx, &self.objective, &draft.objective, window);
        set_textarea(cx, &self.expected, &draft.expected, window);
        set_input(cx, &self.interval, &draft.interval, window);
        set_input(cx, &self.hour, &draft.hour, window);
        set_input(cx, &self.minute, &draft.minute, window);
        set_input(cx, &self.timezone, &draft.timezone, window);
        set_input(cx, &self.delay, &draft.delay, window);
        set_input(cx, &self.timeout, &draft.timeout, window);
        set_input(cx, &self.failures, &draft.failures, window);
        set_input(cx, &self.retention, &draft.retention, window);
    }

    pub fn schedule(&self, cx: &App) -> Result<Schedule, String> {
        let timezone = self.timezone.read(cx).value().trim().to_string();
        parse_timezone(&timezone)?;
        let hour = |value: &str| -> Result<u8, String> {
            value
                .trim()
                .parse::<u8>()
                .map_err(|_| "Enter a whole hour from 00 to 23.".to_string())
        };
        let minute = |value: &str| -> Result<u8, String> {
            value
                .trim()
                .parse::<u8>()
                .map_err(|_| "Enter a whole minute from 00 to 59.".to_string())
        };
        let spec = match self.kind {
            AutomationKind::Interval => {
                let value = self
                    .interval
                    .read(cx)
                    .value()
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| "Enter a whole number for the interval.".to_string())?;
                let minutes = if self.interval_in_hours {
                    value
                        .checked_mul(60)
                        .ok_or("The interval is too large to schedule.")?
                } else {
                    value
                };
                ScheduleSpec::Interval { minutes }
            }
            AutomationKind::Daily => ScheduleSpec::Daily {
                hour: hour(&self.hour.read(cx).value())?,
                minute: minute(&self.minute.read(cx).value())?,
            },
            AutomationKind::Weekly => {
                let days: Vec<u8> = self
                    .weekdays
                    .iter()
                    .enumerate()
                    .filter_map(|(index, on)| on.then_some(index as u8))
                    .collect();
                ScheduleSpec::Weekly {
                    days,
                    hour: hour(&self.hour.read(cx).value())?,
                    minute: minute(&self.minute.read(cx).value())?,
                }
            }
        };
        let schedule = Schedule { spec, timezone };
        schedule.validate()?;
        Ok(schedule)
    }

    pub fn policy(&self, cx: &App) -> Result<AutomationPolicy, String> {
        let delay_text = self.delay.read(cx).value().trim().to_string();
        let delay = if delay_text.is_empty() {
            crate::automation_schedule::default_allowed_delay(&self.schedule(cx)?)? / 60
        } else {
            delay_text
                .parse::<i64>()
                .map_err(|_| "Allowed delay must be a whole number of minutes.".to_string())?
        };
        let timeout = self
            .timeout
            .read(cx)
            .value()
            .trim()
            .parse::<i64>()
            .map_err(|_| "Time limit must be a whole number of minutes.".to_string())?;
        let failures = self
            .failures
            .read(cx)
            .value()
            .trim()
            .parse::<u32>()
            .map_err(|_| "Failure threshold must be a whole number.".to_string())?;
        let retention = self
            .retention
            .read(cx)
            .value()
            .trim()
            .parse::<u32>()
            .map_err(|_| "Log retention must be a whole number of days.".to_string())?;
        let policy = AutomationPolicy {
            allowed_delay_seconds: delay.saturating_mul(60),
            timeout_seconds: timeout.saturating_mul(60),
            max_consecutive_failures: failures,
            catch_up: self.catch_up,
            log_retention_days: retention,
        };
        policy.validate()?;
        Ok(policy)
    }
}

pub fn system_timezone() -> String {
    if let Ok(value) = std::env::var("TZ")
        && parse_timezone(value.trim_start_matches(':')).is_ok()
    {
        return value.trim().trim_start_matches(':').to_string();
    }
    if let Ok(path) = std::fs::read_link("/etc/localtime")
        && let Some(zone) = path
            .to_string_lossy()
            .split("zoneinfo/")
            .nth(1)
            .map(str::to_string)
        && parse_timezone(&zone).is_ok()
    {
        return zone;
    }
    String::new()
}

pub fn schedule_summary(schedule: &Schedule) -> String {
    match &schedule.spec {
        ScheduleSpec::Interval { minutes } => {
            if minutes % 60 == 0 {
                format!("Every {} hours · {}", minutes / 60, schedule.timezone)
            } else {
                format!("Every {minutes} minutes · {}", schedule.timezone)
            }
        }
        ScheduleSpec::Daily { hour, minute } => {
            format!("Daily at {hour:02}:{minute:02} · {}", schedule.timezone)
        }
        ScheduleSpec::Weekly { days, hour, minute } => {
            let names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
            let days = days
                .iter()
                .filter_map(|day| names.get(*day as usize))
                .copied()
                .collect::<Vec<_>>()
                .join(", ");
            format!("{days} at {hour:02}:{minute:02} · {}", schedule.timezone)
        }
    }
}

/// Next trigger labels shared by the detail page and the settings dialog.
pub fn next_trigger_labels(definition: &AutomationDefinition, now: i64) -> Vec<String> {
    let mut values = vec![];
    if let Some(cursor) = definition.cursor {
        values.push(cursor);
        values.extend(
            definition
                .schedule
                .preview(definition.anchor.unwrap_or(now), cursor, 2)
                .unwrap_or_default(),
        );
    } else {
        values.extend(
            definition
                .schedule
                .preview(definition.anchor.unwrap_or(now), now, 3)
                .unwrap_or_default(),
        );
    }
    values
        .into_iter()
        .map(|value| format_in_zone(value, &definition.schedule.timezone))
        .collect()
}

pub fn format_in_zone(utc: i64, timezone: &str) -> String {
    ScheduledTime::new(utc, timezone)
        .map(|value| value.local)
        .unwrap_or_else(|_| utc.to_string())
}

/// Occurrence timestamps carry seconds because several periods can start inside
/// the same minute; minute precision would make the rows indistinguishable.
pub fn format_in_zone_precise(utc: i64, timezone: &str) -> String {
    use chrono::TimeZone;
    parse_timezone(timezone)
        .ok()
        .and_then(|zone| {
            chrono::Utc
                .timestamp_opt(utc, 0)
                .single()
                .map(|value| value.with_timezone(&zone))
        })
        .map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| format_in_zone(utc, timezone))
}

/// Reading text for the automation overview. What the user came for keeps full
/// contrast at body size, so supporting notes cannot be mistaken for content.
pub fn overview_body(text: impl Into<SharedString>, cx: &App) -> Div {
    let colors = palette(cx);
    div()
        .text_sm()
        .text_color(rgb(colors.text))
        .line_height(relative(1.55))
        .child(selectable(text))
}

/// Secondary explanation inside the overview: how the shape works, how runs are
/// governed. Small and muted so it supports the overview without competing.
pub fn overview_note(text: impl Into<SharedString>, cx: &App) -> Div {
    let colors = palette(cx);
    div()
        .text_xs()
        .text_color(rgb(colors.muted))
        .line_height(relative(1.6))
        .child(selectable(text))
}

pub fn overview_heading(text: impl Into<SharedString>) -> Div {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .child(selectable(text))
}

/// Status, shape, version and project as one meta line under the schedule.
/// These are read-only facts, so they must not look like tabs or filters.
pub fn overview_meta_line(definition: &AutomationDefinition, project: &str, cx: &App) -> Div {
    let colors = palette(cx);
    row()
        .flex_wrap()
        .gap_1()
        .text_xs()
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(crate::views::stage_color_for_definition(
                    definition.status,
                    cx,
                )))
                .child(selectable(definition.status.label())),
        )
        .child(
            div()
                .text_color(rgb(colors.muted))
                .child(selectable(format!(
                    "· {} · Version {} · {project}",
                    definition.shape.label(),
                    definition.version
                ))),
        )
}

/// Plain-language run limits, one sentence per line.
pub fn run_policy_summary(policy: &AutomationPolicy) -> String {
    let delay = human_duration(policy.allowed_delay_seconds);
    let limit = human_duration(policy.timeout_seconds);
    let failures = policy.max_consecutive_failures;
    let missed = match policy.catch_up {
        CatchUp::LatestOnly => format!(
            "Missed periods: catch up on the newest one only, and skip it if it is more than {delay} late."
        ),
        CatchUp::SkipAll => "Missed periods: skip all of them.".to_string(),
    };
    format!(
        "{missed}\nEach run stops after {limit}, and {failures} failures in a row pause the automation."
    )
}

fn human_duration(seconds: i64) -> String {
    let minutes = seconds.max(0) / 60;
    if minutes >= 60 {
        match (minutes / 60, minutes % 60) {
            (hours, 0) => format!("{hours} h"),
            (hours, minutes) => format!("{hours} h {minutes} min"),
        }
    } else if minutes > 0 {
        format!("{minutes} min")
    } else {
        format!("{} s", seconds.max(0))
    }
}

pub fn occurrence_window_label(occurrence: &AutomationOccurrence) -> String {
    match &occurrence.data_window.state {
        TimeWindowState::Resolved {
            start_utc, end_utc, ..
        } => format!(
            "{} → {}",
            format_in_zone(*start_utc, &occurrence.timezone),
            format_in_zone(*end_utc, &occurrence.timezone)
        ),
        TimeWindowState::Unresolved { reason } => format!("Unresolved · {reason}"),
    }
}

/// Quick periods show only the time facts a reader needs; the full scheduler
/// provenance stays on research periods, where it affects the evidence window.
fn occurrence_context(
    definition: Option<&AutomationDefinition>,
    occurrence: &AutomationOccurrence,
    store: &Store,
) -> String {
    let scheduled = occurrence
        .scheduled_for
        .map(|value| format_in_zone(value, &occurrence.timezone))
        .unwrap_or_else(|| "Manual request".into());
    let started = occurrence
        .actual_started_at
        .map(|value| format_in_zone_precise(value, &occurrence.timezone))
        .unwrap_or_else(|| "Not started".into());
    let version = match definition {
        Some(definition) if definition.version != occurrence.definition_version => format!(
            "Approved version: v{} · current automation: v{}",
            occurrence.definition_version, definition.version
        ),
        _ => format!("Approved version: v{}", occurrence.definition_version),
    };
    if definition.is_some_and(|definition| definition.shape.is_quick()) {
        let mut lines = vec![
            format!(
                "Scheduled: {scheduled} · Started: {started} · {}",
                occurrence.timezone
            ),
            format!(
                "Reference time: {}",
                format_in_zone(occurrence.reference_time, &occurrence.timezone)
            ),
            version,
        ];
        if matches!(
            occurrence.data_window.state,
            TimeWindowState::Resolved { .. }
        ) {
            lines.push(format!(
                "Data window: {}",
                occurrence_window_label(occurrence)
            ));
        }
        return lines.join("\n");
    }
    format!(
        "Scheduled for: {scheduled}\nActual start: {started}\nTime zone: {}\nReference time: {}\nData window: {}\n{}\n{}",
        occurrence.timezone,
        format_in_zone(occurrence.reference_time, &occurrence.timezone),
        occurrence_window_label(occurrence),
        version,
        occurrence
            .research_topic_id
            .and_then(|topic| store.topics().iter().find(|candidate| candidate.id == topic))
            .map(|topic| format!("Period topic: {} (under Automations)", topic.display_name()))
            .unwrap_or_else(|| "Stored directly under Automations (no topic).".into()),
    )
}

fn stop_outcome_feedback(outcome: &crate::domain_command::StopOutcome) -> Result<String, String> {
    use crate::domain_command::{StopDurability, StopOutcome};
    match outcome {
        StopOutcome::Stopped {
            durability: StopDurability::Saved,
        } => Ok("Stopped. The cancellation is saved and this period will not restart.".into()),
        StopOutcome::Stopped {
            durability: StopDurability::NotSaved { message },
        } => Err(format!(
            "Stopped externally · cancellation not saved · Retry save: {message}"
        )),
        StopOutcome::Stopped {
            durability: StopDurability::NotRecorded { message },
        } => Err(format!(
            "Stopped externally · stop intent not recorded · Retry save: {message}"
        )),
        StopOutcome::StopRequestedNotDurable { durability, .. } => Err(match durability {
            StopDurability::Saved => {
                "Stop requested · cancellation is not confirmed durable.".into()
            }
            StopDurability::NotSaved { message } => {
                format!("Stop requested · cancellation not saved · Retry save: {message}")
            }
            StopDurability::NotRecorded { message } => {
                format!("Stop requested · stop intent not recorded · Retry save: {message}")
            }
        }),
        StopOutcome::StopRequestFailed { error, .. } => Err(format!("Stop requested · {error}")),
        StopOutcome::AlreadyEnded { durable: true } => {
            Ok("This period already ended; no restart will be attempted.".into())
        }
        StopOutcome::AlreadyEnded { durable: false } => {
            Err("Stop requested · period ended, cancellation not saved.".into())
        }
    }
}

impl TaskApp {
    pub fn open_automations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Scope::Automations, window, cx);
        self.automation_ui.scroll.set_offset(point(px(0.), px(0.)));
    }

    pub fn open_automation(&mut self, id: u64, _window: &mut Window, cx: &mut Context<Self>) {
        if self.store.automation(id).is_none() {
            return;
        }
        self.install_route(crate::navigation_route::Route::automation_detail(id));
        self.automation_ui.selected = Some(id);
        self.automation_ui.creating = false;
        let _ = self.close_settings_overlay();
        // Results stay closed by default; the newest period is still first in
        // the history, and a row opens its result in a dialog.
        let _ = self.close_period_overlay();
        self.automation_ui.show_technical = false;
        self.automation_ui.show_completed_steps = false;
        self.automation_ui.expanded_sources.clear();
        self.automation_ui.technical_run = None;
        self.automation_ui.technical_scroll = ScrollHandle::new();
        self.automation_ui.feedback = None;
        self.automation_ui.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    pub fn open_automation_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.automation_ui.selected.is_none() {
            return;
        }
        self.remember_dialog_opener(window, cx);
        let _ = self.open_settings_overlay();
        let _ = self.close_period_overlay();
        self.automation_ui.feedback = None;
        window.focus(&self.modal_focus, cx);
        cx.notify();
    }

    pub fn close_automation_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.close_settings_overlay();
        self.restore_dialog_opener(window, cx);
        cx.notify();
    }

    pub fn open_occurrence_result(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.occurrence(id).is_none() {
            return;
        }
        self.automation_ui.pending_period = None;
        self.pending_result_navigation = None;
        if self.history_reader.is_some() && !self.pending_period_bodies(id).is_empty() {
            self.automation_ui.pending_period = Some(id);
            self.poll_history_io(cx);
            self.start_pump(cx);
            cx.notify();
            return;
        }
        self.remember_dialog_opener(window, cx);
        let _ = self.push_period_overlay(id);
        let _ = self.close_settings_overlay();
        self.automation_ui.show_technical = false;
        self.automation_ui.show_completed_steps = false;
        self.automation_ui.expanded_sources.clear();
        self.automation_ui.technical_run = None;
        self.automation_ui.technical_scroll = ScrollHandle::new();
        self.automation_ui.feedback = None;
        if self
            .store
            .occurrence(id)
            .is_some_and(|occurrence| occurrence.unread)
        {
            self.store.mark_occurrence_read(id);
            self.persist_if_changed(true);
        }
        window.focus(&self.dialog_first_focus, cx);
        cx.notify();
    }

    /// Prepare only this period's retained results before revealing its dialog.
    pub(crate) fn pending_period_bodies(&self, id: u64) -> Vec<u64> {
        let Some(occurrence) = self.store.occurrence(id) else { return vec![]; };
        let mut runs: Vec<_> = occurrence.final_run_id
            .or_else(|| occurrence.run_ids.last().copied()).into_iter().collect();
        let goals = if occurrence.research_goal_ids.is_empty() {
            occurrence.research_goal_id.into_iter().collect()
        } else { occurrence.research_goal_ids.clone() };
        for goal in goals {
            for task in self.store.research_children(goal) {
                if let Some(run) = self.store.research_result_body_pending(task) { runs.push(run); }
            }
        }
        // Include earlier attempts: a historical report may be the chosen fallback.
        runs.extend(self.store.runs().iter()
            .filter(|run| run.automation_occurrence_id == Some(id)).map(|run| run.id));
        runs.sort_unstable();
        runs.dedup();
        runs.retain(|id| self.store.run_body_is_lazy(*id)
            && self.store.runs().iter().any(|run| run.id == *id && run.output.is_empty())
            && !matches!(self.run_body_state.get(id),
                Some(crate::history_reader::BodyLoad::Failed { .. }
                    | crate::history_reader::BodyLoad::Absent
                    | crate::history_reader::BodyLoad::Loaded)));
        runs
    }

    pub(crate) fn apply_pending_period(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.automation_ui.pending_period else { return; };
        if self.store.occurrence(id).is_none() {
            self.automation_ui.pending_period = None;
        } else if self.pending_period_bodies(id).is_empty() {
            self.automation_ui.pending_period = None;
            self.open_occurrence_result(id, window, cx);
        }
    }

    /// Explicit progress action for an automation-owned step: open the period
    /// that owns it, not generic Research results.
    pub(crate) fn open_automation_progress_for_goal(
        &mut self,
        goal: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let owned = |occurrence: &&AutomationOccurrence| {
            occurrence.research_goal_id == Some(goal)
                || occurrence.research_goal_ids.contains(&goal)
        };
        let occurrence = self
            .store
            .all_occurrences()
            .iter()
            .rev()
            .find(|occurrence| occurrence.status.is_active() && owned(occurrence))
            .or_else(|| self.store.all_occurrences().iter().rev().find(owned))
            .cloned();
        let Some(occurrence) = occurrence else {
            return false;
        };
        self.open_automation(occurrence.automation_id, window, cx);
        self.open_occurrence_result(occurrence.id, window, cx);
        true
    }

    pub fn close_occurrence_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.automation_ui.pending_period = None;
        let _ = self.close_period_overlay();
        self.automation_ui.show_technical = false;
        self.automation_ui.show_completed_steps = false;
        self.automation_ui.expanded_sources.clear();
        self.automation_ui.technical_run = None;
        self.automation_ui.technical_scroll = ScrollHandle::new();
        self.restore_dialog_opener(window, cx);
        cx.notify();
    }

    pub fn open_planning_attempt(
        &mut self,
        run_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.store.runs().iter().any(|run| run.id == run_id) {
            return;
        }
        self.automation_ui.planning_attempt_return_run = Some(run_id);
        if !self.push_planning_attempt_overlay(run_id) {
            self.automation_ui.planning_attempt_return_run = None;
            return;
        }
        window.focus(&self.modal_focus, cx);
        cx.notify();
    }

    pub fn close_planning_attempt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.close_planning_attempt_overlay();
        if self.automation_ui.planning_attempt_origin_settings {
            let _ = self.open_settings_overlay();
        }
        self.automation_ui.planning_attempt_origin_settings = false;
        let opener = self
            .automation_ui
            .planning_attempt_return_run
            .take()
            .and_then(|run_id| self.automation_ui.planning_opener_focus_for(run_id));
        if let Some(opener) = opener {
            window.focus(&opener, cx);
        } else if self.settings_overlay() {
            window.focus(&self.modal_focus, cx);
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    pub fn start_automation_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.install_route(crate::navigation_route::Route::automation_editor(None));
        self.automation_ui.form_epoch = self.automation_ui.form_epoch.wrapping_add(1);
        self.automation_ui.selected = None;
        self.automation_ui.creating = true;
        self.automation_ui.editing = None;
        self.automation_ui.step = 1;
        self.automation_ui.scroll.set_offset(point(px(0.), px(0.)));
        self.automation_ui.planning_automation = None;
        self.automation_ui.planning_run = None;
        self.automation_ui.pending_planning = None;
        self.automation_ui.planning_save_inflight = None;
        self.automation_ui.show_advanced = false;
        self.automation_ui.preview.clear();
        self.automation_ui.preview_error.clear();
        self.automation_ui.feedback = None;
        match self.ensure_automation_project() {
            Ok(project) => self.automation_ui.project = project,
            Err(error) => {
                self.automation_ui.creating = false;
                self.automation_ui.feedback = Some(Err(error));
                cx.notify();
                return;
            }
        }
        let draft = AutomationDraft::new(&AutomationDraftDefaults::system());
        self.automation_ui.apply_draft(&draft, cx, window);
        self.set_automation_baseline(cx);
        cx.notify();
    }

    /// Open the wizard with the frozen plan cleared so continuing generates a new
    /// plan version. Nothing changes until the new plan is reviewed and enabled.
    pub fn replan_automation(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if !self
            .store
            .automation(id)
            .is_some_and(|definition| definition.shape == AutomationShape::Research)
        {
            return;
        }
        self.edit_automation(id, window, cx);
        self.automation_ui.plan = None;
        self.automation_ui.planning_automation = None;
        self.set_automation_baseline(cx);
        self.automation_ui.feedback = Some(Ok(
            "Replanning starts a new version. Adjust the request, continue, then approve the new plan."
                .into(),
        ));
        cx.notify();
    }

    /// Reopen the wizard on the planning step after the user leaves it. The
    /// in-flight planner keeps its identity so its result lands in this review.
    pub fn review_automation_planning(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.store.automation(id).is_none() {
            return;
        }
        let state = self.effective_planning_state(id);
        self.edit_automation(id, window, cx);
        self.automation_ui.step = 3;
        let planning = state
            == crate::automation_planning_state::AutomationPlanningState::Planning;
        self.automation_ui.planning_automation = planning.then_some(id);
        // Restore the in-flight attempt token only while planning is actually
        // active; a terminal or needs-review request must show the retry path,
        // not a fake planning spinner.
        self.automation_ui.planning_run = if planning {
            self.store
                .latest_planning_request(id)
                .and_then(|request| self.store.latest_request_attempt(request.id))
                .map(|run| run.id)
        } else {
            None
        };
        self.automation_ui.preview_error.clear();
        cx.notify();
    }

    pub fn edit_automation(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(definition) = self.store.automation(id).cloned() else {
            return;
        };
        self.install_route(crate::navigation_route::Route::automation_editor(Some(id)));
        self.automation_ui.form_epoch = self.automation_ui.form_epoch.wrapping_add(1);
        self.automation_ui.selected = Some(id);
        self.automation_ui.creating = true;
        self.automation_ui.editing = Some(id);
        self.automation_ui.step = 1;
        self.automation_ui.planning_automation = None;
        self.automation_ui.planning_run = None;
        self.automation_ui.pending_planning = None;
        self.automation_ui.planning_save_inflight = None;
        self.automation_ui.show_advanced = false;
        self.automation_ui.preview.clear();
        self.automation_ui.preview_error.clear();
        self.automation_ui.feedback = None;
        self.automation_ui.project = self
            .ensure_automation_project()
            .unwrap_or(definition.project_id);
        let draft =
            AutomationDraft::from_definition(&definition, &AutomationDraftDefaults::system());
        self.automation_ui.apply_draft(&draft, cx, window);
        self.set_automation_baseline(cx);
        cx.notify();
    }

    /// Switching shape discards any plan generated for the previous shape and
    /// ignores completion of an in-flight planning attempt, so an incompatible
    /// plan can never be approved silently. The saved planning run is left
    /// intact. Existing definitions are re-versioned on save, which also clears
    /// their stored approval.
    pub fn select_automation_shape(&mut self, shape: AutomationShape, cx: &mut Context<Self>) {
        if self.automation_ui.shape == shape {
            return;
        }
        self.automation_ui.shape = shape;
        self.automation_ui.plan = None;
        self.automation_ui.planning_automation = None;
        self.automation_ui.planning_run = None;
        self.automation_ui.pending_planning = None;
        self.automation_ui.planning_save_inflight = None;
        self.automation_ui.preview.clear();
        self.automation_ui.preview_error.clear();
        self.automation_ui.feedback = None;
        cx.notify();
    }

    fn automation_validate_step_one(&mut self, cx: &App) -> Result<(), String> {
        let objective = self
            .automation_ui
            .objective
            .read(cx)
            .value()
            .trim()
            .to_string();
        let expected = self
            .automation_ui
            .expected
            .read(cx)
            .value()
            .trim()
            .to_string();
        crate::automation_policy::validate_automation_fields(&objective, &expected, false)?;
        let name = self.automation_ui.name.read(cx).value().trim().to_string();
        if name.chars().count() > 160 {
            return Err("Automation names must be at most 160 characters.".into());
        }
        if !self
            .store
            .automation_project()
            .is_some_and(|project| project.id == self.automation_ui.project)
        {
            return Err("The automation workspace is not ready yet. Try again.".into());
        }
        Ok(())
    }

    /// Quick tasks are saved without any plan step; this fills the fields the
    /// domain requires while keeping the single visible input authoritative.
    fn quick_task_text(&self, cx: &App) -> (String, String) {
        let objective = self
            .automation_ui
            .objective
            .read(cx)
            .value()
            .trim()
            .to_string();
        let expected = self
            .automation_ui
            .expected
            .read(cx)
            .value()
            .trim()
            .to_string();
        let expected = if expected.is_empty() {
            "A concise answer with dates, sources and any evidence gaps.".to_string()
        } else {
            expected
        };
        (objective, expected)
    }

    fn quick_task_name(&self, objective: &str, cx: &App) -> String {
        let name = self.automation_ui.name.read(cx).value().trim().to_string();
        if !name.is_empty() {
            return name;
        }
        objective.chars().take(60).collect::<String>()
    }

    pub fn automation_wizard_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.automation_ui.feedback = None;
        match self.automation_ui.step {
            1 => {
                if let Err(error) = self.automation_validate_step_one(cx) {
                    self.automation_ui.feedback = Some(Err(error));
                    cx.notify();
                    return;
                }
                self.automation_ui.step = 2;
            }
            2 => {
                let schedule = match self.automation_ui.schedule(cx) {
                    Ok(value) => value,
                    Err(error) => {
                        self.automation_ui.preview_error = error;
                        cx.notify();
                        return;
                    }
                };
                if let Err(error) = self.automation_ui.policy(cx) {
                    self.automation_ui.preview_error = error;
                    cx.notify();
                    return;
                }
                let now = Self::automation_now();
                match schedule.preview(now, now, 3) {
                    Ok(values) => self.automation_ui.preview = values,
                    Err(error) => {
                        self.automation_ui.preview_error = error;
                        cx.notify();
                        return;
                    }
                }
                if self.automation_ui.shape.is_quick() {
                    let (objective, expected) = self.quick_task_text(cx);
                    match build_quick_template(&objective, &expected) {
                        Ok(template) => {
                            self.automation_ui.plan = Some(template);
                            self.automation_ui.preview_error.clear();
                            self.automation_ui.step = 3;
                        }
                        Err(error) => self.automation_ui.preview_error = error,
                    }
                } else if self.automation_ui.plan.is_some() {
                    self.automation_ui.preview_error.clear();
                    self.automation_ui.step = 3;
                } else if self.automation_ui.planning_automation.is_some() {
                    // A changed objective/expected result is a new immutable
                    // revision; never let the in-flight plan stand in for it.
                    let changed = self.automation_ui.planning_run.is_some_and(|run_id| {
                        let objective = self
                            .automation_ui
                            .objective
                            .read(cx)
                            .value()
                            .trim()
                            .to_string();
                        let expected = self
                            .automation_ui
                            .expected
                            .read(cx)
                            .value()
                            .trim()
                            .to_string();
                        self.store
                            .runs()
                            .iter()
                            .find(|run| run.id == run_id)
                            .and_then(|run| run.planning_owner.as_ref())
                            .and_then(|owner| {
                                self.store
                                    .research_requests()
                                    .iter()
                                    .find(|request| request.id == owner.request_id)
                            })
                            .is_some_and(|request| {
                                request.input.trim() != objective
                                    || request
                                        .expected_result
                                        .as_deref()
                                        .unwrap_or_default()
                                        .trim()
                                        != expected
                            })
                    });
                    if changed {
                        match self.start_automation_planning(cx) {
                            Ok(AutomationPlanningStart::Started(automation)) => {
                                self.automation_ui.planning_automation = Some(automation);
                                self.automation_ui.preview_error.clear();
                                self.automation_ui.step = 3;
                            }
                            Ok(AutomationPlanningStart::WaitingForDraft) => {
                                self.automation_ui.preview_error.clear();
                                self.automation_ui.step = 3;
                                self.automation_ui.feedback = Some(Ok(
                                    "Saving… the planner starts after the draft is durable."
                                        .into(),
                                ));
                            }
                            Err(error) => self.automation_ui.preview_error = error,
                        }
                    } else {
                        // The planner is already running; keep the same request instead
                        // of starting a duplicate planning run.
                        self.automation_ui.step = 3;
                    }
                } else {
                    // Research automations are planned once, here. Periods never plan.
                    match self.start_automation_planning(cx) {
                        Ok(AutomationPlanningStart::Started(automation)) => {
                            self.automation_ui.planning_automation = Some(automation);
                            self.automation_ui.preview_error.clear();
                            self.automation_ui.step = 3;
                        }
                        Ok(AutomationPlanningStart::WaitingForDraft) => {
                            self.automation_ui.preview_error.clear();
                            self.automation_ui.step = 3;
                            self.automation_ui.feedback = Some(Ok(
                                "Saving… the planner starts after the draft is durable.".into(),
                            ));
                        }
                        Err(error) => self.automation_ui.preview_error = error,
                    }
                }
            }
            _ => {}
        }
        let _ = window;
        cx.notify();
    }

    pub fn automation_wizard_back(&mut self, cx: &mut Context<Self>) {
        if self.automation_ui.pending_planning.is_some() {
            self.automation_ui.pending_planning = None;
        }
        if self.automation_ui.step > 1 {
            self.automation_ui.step -= 1;
        }
        self.automation_ui.feedback = None;
        cx.notify();
    }

    /// Run the research planning chain once for this automation and freeze the
    /// reviewed result into a template. Later periods instantiate it directly.
    fn start_automation_planning(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<AutomationPlanningStart, String> {
        let current_input = self.automation_draft_input(cx).ok();
        if let Some(pending) = self.automation_ui.pending_planning.as_ref() {
            let same_review = pending.form_epoch == self.automation_ui.form_epoch
                && current_input.as_ref() == Some(&pending.input);
            if same_review && self.live_has_pending_receipts() {
                // The reviewed SaveDraft is still in flight or retained for
                // Retry. Its exact durable receipt owns the one planner start.
                return Ok(AutomationPlanningStart::WaitingForDraft);
            }
            if same_review {
                // A failed save left no in-flight obligation. Drop the stale
                // intent and submit the reviewed form again.
                self.automation_ui.pending_planning = None;
            }
        }
        self.automation_ui.pending_planning = None;
        match self.save_automation_draft_outcome(cx) {
            AutomationSaveOutcome::Saved => {
                let id = self
                    .automation_ui
                    .selected
                    .or(self.automation_ui.editing)
                    .ok_or("The saved draft did not return an automation id.")?;
                // A synchronous/no-op SaveDraft normally closes the wizard.
                // Generate plan still owns the wizard while it starts planning.
                self.automation_ui.creating = true;
                self.automation_ui.editing = Some(id);
                self.automation_ui.selected = Some(id);
                self.begin_automation_planning(id, cx)?;
                self.automation_ui.feedback =
                    Some(Ok("Plan request submitted · one planner run is queued.".into()));
                self.toast = "Automation plan requested".into();
                Ok(AutomationPlanningStart::Started(id))
            }
            AutomationSaveOutcome::Saving {
                input,
                subject,
                form_epoch,
            } => {
                self.automation_ui.planning_save_inflight = Some(PlanningSaveIdentity {
                    input: input.clone(),
                    form_epoch,
                });
                self.automation_ui.pending_planning = Some(PendingAutomationPlanning {
                    input,
                    form_epoch,
                    automation: subject.or(self.automation_ui.editing),
                });
                Ok(AutomationPlanningStart::WaitingForDraft)
            }
            AutomationSaveOutcome::Rejected(error) => {
                self.automation_ui.planning_save_inflight = None;
                Err(error)
            }
        }
    }

    /// Create the immutable planning request and its single planner run after
    /// the reviewed draft is durable.
    fn begin_automation_planning(
        &mut self,
        id: u64,
        cx: &mut Context<Self>,
    ) -> Result<u64, String> {
        let objective = self
            .automation_ui
            .objective
            .read(cx)
            .value()
            .trim()
            .to_string();
        let expected = self
            .automation_ui
            .expected
            .read(cx)
            .value()
            .trim()
            .to_string();
        let project = self
            .store
            .automation(id)
            .map(|definition| definition.project_id)
            .ok_or("Automation not found.")?;
        let request = self
            .store
            .create_automation_request(id, project, &objective, &expected)?;
        let run = self.store.prepare_request_plan(request)?;
        let run_id = run.id;
        // The typed request-plan dispatch commits the request mapping and the
        // queued run in one staged store. Planning is part of creating this
        // automation, so the wizard stays visible instead of following the run.
        self.launch_run_with_intent(
            run,
            vec![],
            crate::app::LaunchIntent::RequestPlan { request_id: request },
            cx,
        );
        // Live admission returns before its run is merged into Store. The
        // exact reserved run identity keeps this wizard attached to that ACK.
        let accepted = self.store.runs().iter().any(|candidate| candidate.id == run_id)
            || self.dispatch_awaiting.as_ref().is_some_and(|pending| pending.run_id == run_id);
        if !accepted {
            return Err(if self.error.is_empty() {
                "The plan could not start. Retry after resolving the pending change.".into()
            } else {
                self.error.clone()
            });
        }
        self.automation_ui.planning_automation = Some(id);
        self.automation_ui.planning_run = Some(run_id);
        Ok(id)
    }

    /// Resume the reviewed planning intent after the exact durable SaveDraft
    /// receipt. A newer form or a different receipt cannot consume it.
    pub(crate) fn resume_automation_planning_after_save(
        &mut self,
        subject: Option<u64>,
        receipt: &crate::live_save_app::AutomationSaveReceipt,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.automation_ui.pending_planning.clone() else {
            return;
        };
        if pending.form_epoch != receipt.form_epoch || pending.input != receipt.input {
            self.automation_ui.pending_planning = None;
            return;
        }
        if self.automation_ui.form_epoch != receipt.form_epoch
            || !self.automation_ui.creating
            || self.automation_ui.step != 3
        {
            // Back, Cancel, a newer form or a route change owns the surface;
            // never launch planning from a stale waiting intent.
            self.automation_ui.pending_planning = None;
            return;
        }
        self.automation_ui.pending_planning = None;
        let Some(id) = subject.or(pending.automation).or(self.automation_ui.editing) else {
            self.automation_ui.feedback = Some(Err(
                "The durable draft did not identify an automation for planning.".into(),
            ));
            return;
        };
        self.automation_ui.editing = Some(id);
        self.automation_ui.selected = Some(id);
        match self.begin_automation_planning(id, cx) {
            Ok(_) => {
                self.automation_ui.preview_error.clear();
                self.automation_ui.step = 3;
                self.automation_ui.feedback =
                    Some(Ok("Plan request submitted · one planner run is queued.".into()));
                self.toast = "Automation plan requested".into();
                cx.notify();
            }
            Err(error) => {
                self.automation_ui.preview_error = error.clone();
                self.automation_ui.feedback = Some(Err(error));
            }
        }
    }

    pub(crate) fn clear_pending_automation_planning(&mut self) {
        self.automation_ui.pending_planning = None;
    }

    pub(crate) fn clear_planning_save_inflight(&mut self) {
        self.automation_ui.planning_save_inflight = None;
    }

    /// A lifecycle command must not overlap a planning/draft receipt. The
    /// receipts share the ordered primary slot; allowing both would let one
    /// completion consume the other's identity.
    pub(crate) fn automation_command_busy(&self) -> bool {
        self.navigation.pending_automation_save.is_some()
            || self.automation_ui.planning_save_inflight.is_some()
            || self.automation_ui.pending_planning.is_some()
            || self.automation_ui.automation_command_inflight.is_some()
    }

    /// Settle the exact enable/pause/resume UI after its durable receipt.
    pub(crate) fn finish_automation_command(
        &mut self,
        subject: Option<u64>,
        receipt: AutomationCommandReceipt,
        cx: &mut Context<Self>,
    ) {
        if self.automation_ui.automation_command_inflight.as_ref() == Some(&receipt) {
            self.automation_ui.automation_command_inflight = None;
        }
        match receipt {
            AutomationCommandReceipt::Enable(enable) => {
                if self.automation_ui.form_epoch != enable.form_epoch {
                    return;
                }
                match enable.id {
                    Some(id) if self.automation_ui.editing != Some(id) => return,
                    None if !self.automation_ui.creating => return,
                    _ => {}
                }
                // Same-form edits after the click are not part of the saved
                // input; keep the wizard and its dirty state.
                let same_input = self
                    .automation_draft_input(cx)
                    .ok()
                    .as_ref()
                    == Some(&enable.input);
                if !same_input {
                    // A new Quick creation has just become durable. Keep the
                    // same open form and adopt the created identity so the next
                    // Save/Enable updates that definition instead of creating a
                    // second one. Never clear newer fields or the dirty baseline.
                    if enable.id.is_none()
                        && self.automation_ui.creating
                        && self.automation_ui.editing.is_none()
                        && let Some(id) = subject
                    {
                        self.automation_ui.editing = Some(id);
                        self.automation_ui.selected = Some(id);
                    }
                    self.automation_ui.feedback = Some(Ok(
                        "Enabled the saved version; newer edits remain unsaved.".into(),
                    ));
                    self.toast = "Automation enabled · newer edits remain unsaved".into();
                    return;
                }
                let Some(id) = subject.or(enable.id) else {
                    return;
                };
                self.automation_ui.creating = false;
                self.automation_ui.editing = None;
                self.automation_ui.selected = Some(id);
                self.automation_ui.step = 1;
                self.navigation.automation_baseline = None;
                self.automation_ui.pending_planning = None;
                self.automation_ui.planning_save_inflight = None;
                self.automation_ui.feedback = Some(Ok(
                    "Enabled. The first run starts after this moment.".into(),
                ));
                self.toast = "Automation enabled".into();
                self.start_pump(cx);
            }
            AutomationCommandReceipt::Pause {
                id,
                form_epoch,
                creating,
            } => {
                let matches = self.automation_ui.form_epoch == form_epoch
                    && self.automation_ui.creating == creating
                    && if creating {
                        self.automation_ui.editing == Some(id)
                    } else {
                        self.automation_ui.selected == Some(id)
                    };
                if matches {
                    self.automation_ui.feedback = Some(Ok(
                        "Paused. Future triggers stopped and unstarted requests were cancelled. A running period can finish."
                            .into(),
                    ));
                }
                self.toast = "Automation paused".into();
                self.start_pump(cx);
            }
            AutomationCommandReceipt::Resume {
                id,
                form_epoch,
                creating,
            } => {
                let matches = self.automation_ui.form_epoch == form_epoch
                    && self.automation_ui.creating == creating
                    && if creating {
                        self.automation_ui.editing == Some(id)
                    } else {
                        self.automation_ui.selected == Some(id)
                    };
                if matches {
                    self.automation_ui.feedback = Some(Ok(
                        "Enabled again. The next trigger is computed from now; time spent paused is not backfilled."
                            .into(),
                    ));
                }
                self.toast = "Automation enabled".into();
                self.start_pump(cx);
                self.arm_automation_timer(cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn automation_planning_pending(&self) -> bool {
        self.automation_ui.pending_planning.is_some()
    }

    /// Called when the planner run finishes so the review step shows the frozen
    /// plan. Only the attempt that started this wizard may update it; a stale
    /// completion is left in durable state but cannot repopulate a discarded
    /// plan, move the wizard, or revive a previous shape.
    pub(crate) fn automation_plan_ready(&mut self, automation: u64, run_id: u64, cx: &App) {
        let accepted = self.automation_ui.planning_run == Some(run_id)
            && self.automation_ui.planning_automation == Some(automation)
            && self.automation_ui.shape == AutomationShape::Research;
        if accepted {
            let request_matches = self
                .store
                .runs()
                .iter()
                .find(|run| run.id == run_id)
                .and_then(|run| run.planning_owner.as_ref())
                .and_then(|owner| {
                    self.store
                        .research_requests()
                        .iter()
                        .find(|request| request.id == owner.request_id)
                        .map(|request| (request, owner))
                })
                .is_some_and(|(request, owner)| {
                    let objective = self
                        .automation_ui
                        .objective
                        .read(cx)
                        .value()
                        .trim()
                        .to_string();
                    let expected = self
                        .automation_ui
                        .expected
                        .read(cx)
                        .value()
                        .trim()
                        .to_string();
                    owner.automation_id == Some(automation)
                        && request.automation_id == Some(automation)
                        && request.input.trim() == objective
                        && request
                            .expected_result
                            .as_deref()
                            .unwrap_or_default()
                            .trim()
                            == expected
                });
            self.automation_ui.planning_automation = None;
            self.automation_ui.planning_run = None;
            if !request_matches {
                self.automation_ui.plan = None;
                self.automation_ui.feedback = Some(Err(
                    "The plan that finished belongs to earlier planning text. The saved draft is unchanged; continue to replan against the current objective and expected result."
                        .into(),
                ));
                self.toast = "Plan kept as history · replan the edited request".into();
                self.persist_if_changed(true);
                return;
            }
            let template = self
                .store
                .automation(automation)
                .and_then(|definition| definition.template.clone());
            self.automation_ui.plan = template;
            if let Some(baseline) = self.navigation.automation_baseline.as_mut() {
                baseline.plan = self.automation_ui.plan.clone();
            }
            self.automation_ui.feedback = Some(Ok(
                "Plan generated · review the goals and steps before enabling.".into(),
            ));
            self.toast = "Automation plan generated · review and approve".into();
        }
        self.persist_if_changed(true);
    }

    /// Read the validated form into the domain command input. All schedule,
    /// policy, template and approval rules remain in their existing owners.
    pub(crate) fn automation_draft_input(
        &mut self,
        cx: &App,
    ) -> Result<crate::domain_command::AutomationDraftInput, String> {
        self.automation_validate_step_one(cx)?;
        let schedule = self.automation_ui.schedule(cx)?;
        let policy = self.automation_ui.policy(cx)?;
        let shape = self.automation_ui.shape;
        let (objective, expected) = self.quick_task_text(cx);
        let name = self.quick_task_name(&objective, cx);
        let template = if shape.is_quick() {
            Some(build_quick_template(&objective, &expected)?)
        } else {
            self.automation_ui.plan.clone()
        };
        Ok(crate::domain_command::AutomationDraftInput {
            project: self.automation_ui.project,
            shape,
            name,
            objective,
            expected_result: expected,
            schedule,
            policy,
            template,
        })
    }

    /// Create or revise the draft version through the S1 command executor.
    /// Returns the definition id only after the staged store is durable.
    fn save_automation_form(&mut self, cx: &mut Context<Self>) -> Result<u64, String> {
        let input = self.automation_draft_input(cx)?;
        let command = crate::domain_command::Command::SaveDraft {
            id: self.automation_ui.editing,
            input,
            now: Self::automation_now(),
        };
        let run = self.execute_command(command, cx)?;
        let receipt = run
            .receipt()
            .ok_or_else(|| "Saving… retry when the current change is durable.".to_string())?;
        let id = receipt
            .subject
            .ok_or_else(|| "The saved draft did not return an automation id.".to_string())?;
        self.automation_ui.editing = Some(id);
        self.automation_ui.selected = Some(id);
        Ok(id)
    }

    pub fn save_automation_draft(&mut self, cx: &mut Context<Self>) -> bool {
        matches!(
            self.save_automation_draft_outcome(cx),
            AutomationSaveOutcome::Saved
        )
    }

    /// Distinguishes a genuine in-flight durable save from a rejection. The
    /// dirty-exit decision keeps its modal open and waits for the exact ACK.
    pub(crate) fn save_automation_draft_outcome(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AutomationSaveOutcome {
        self.automation_ui.feedback = None;
        let input = match self.automation_draft_input(cx) {
            Ok(input) => input,
            Err(error) => {
                self.automation_ui.feedback = Some(Err(error.clone()));
                cx.notify();
                return AutomationSaveOutcome::Rejected(error);
            }
        };
        let command = crate::domain_command::Command::SaveDraft {
            id: self.automation_ui.editing,
            input: input.clone(),
            now: Self::automation_now(),
        };
        self.navigation.pending_automation_save =
            Some(crate::live_save_app::AutomationSaveReceipt {
                input: input.clone(),
                form_epoch: self.automation_ui.form_epoch,
            });
        let outcome = match self.execute_command(command, cx) {
            Ok(crate::command_app::CommandRun::Saving { subject }) => {
                if let Some(id) = subject {
                    self.automation_ui.editing = Some(id);
                    self.automation_ui.selected = Some(id);
                }
                self.automation_ui.feedback =
                    Some(Ok("Saving… the draft becomes durable after acknowledgement.".into()));
                AutomationSaveOutcome::Saving {
                    input,
                    subject,
                    form_epoch: self.automation_ui.form_epoch,
                }
            }
            Ok(crate::command_app::CommandRun::Durable(receipt)) => {
                let id = receipt.subject.expect("save draft returns a definition id");
                self.automation_ui.creating = false;
                self.automation_ui.selected = Some(id);
                let status = self
                    .store
                    .automation(id)
                    .map(|definition| definition.status)
                    .unwrap_or(DefinitionStatus::Draft);
                let message = match status {
                    DefinitionStatus::Enabled => "Saved. The enabled version remains approved.",
                    DefinitionStatus::Paused => "Saved. The automation remains paused.",
                    DefinitionStatus::NeedsAttention => {
                        "No changes. Resolve the listed problem before it can run."
                    }
                    _ => "Draft saved. Nothing will run until you enable it.",
                };
                self.automation_ui.feedback = Some(Ok(message.into()));
                self.toast = "Automation saved locally".into();
                AutomationSaveOutcome::Saved
            }
            Err(error) => {
                self.automation_ui.feedback = Some(Err(error.clone()));
                AutomationSaveOutcome::Rejected(error)
            }
        };
        // A rejected/preparation failure or legacy synchronous save never takes
        // the handoff. Never let a later unrelated command consume it.
        self.navigation.pending_automation_save = None;
        cx.notify();
        outcome
    }

    pub fn enable_automation(&mut self, cx: &mut Context<Self>) {
        self.automation_ui.feedback = None;
        if self.automation_command_busy() {
            self.automation_ui.feedback = Some(Err(
                "Wait for the pending save or planning receipt before enabling this automation."
                    .into(),
            ));
            cx.notify();
            return;
        }
        // A research plan is bound to the request it was generated from. Editing the
        // wording without replanning would silently approve a different task.
        if self.automation_ui.shape == AutomationShape::Research
            && let Some(id) = self.automation_ui.editing
            && let Some(definition) = self.store.automation(id)
            && definition.template.is_some()
            && let Some(request) = self.store.latest_planning_request(id)
        {
            let objective = self
                .automation_ui
                .objective
                .read(cx)
                .value()
                .trim()
                .to_string();
            let expected = self
                .automation_ui
                .expected
                .read(cx)
                .value()
                .trim()
                .to_string();
            if request.input.trim() != objective
                || request
                    .expected_result
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    != expected
            {
                self.automation_ui.feedback = Some(Err(
                    "The research request or expected result changed after planning. Use Replan to generate and review a new plan before enabling."
                        .into(),
                ));
                cx.notify();
                return;
            }
            if let Some(problem) = self.store.plan_binding_problem(definition) {
                self.automation_ui.feedback = Some(Err(problem));
                cx.notify();
                return;
            }
        }
        let input = match self.automation_draft_input(cx) {
            Ok(input) => input,
            Err(error) => {
                self.automation_ui.feedback = Some(Err(error));
                cx.notify();
                return;
            }
        };
        let enable_input = input.clone();
        let command = crate::domain_command::Command::Enable {
            id: self.automation_ui.editing,
            input,
            materials: vec![],
            now: Self::automation_now(),
        };
        let enable_id = self.automation_ui.editing;
        let enable_epoch = self.automation_ui.form_epoch;
        let receipt = AutomationCommandReceipt::Enable(AutomationEnableReceipt {
            id: enable_id,
            form_epoch: enable_epoch,
            input: enable_input,
        });
        self.automation_ui.pending_command = Some(receipt.clone());
        self.automation_ui.automation_command_inflight = Some(receipt.clone());
        match self.execute_command(command, cx) {
            Ok(crate::command_app::CommandRun::Saving { .. }) => {
                self.automation_ui.feedback =
                    Some(Ok("Saving… enabling finishes after the durable receipt.".into()));
            }
            Ok(crate::command_app::CommandRun::Durable(receipt_subject)) => {
                let id = receipt_subject.subject.expect("enable returns a definition id");
                self.automation_ui.pending_command = None;
                if self.automation_ui.automation_command_inflight.is_some() {
                    self.automation_ui.automation_command_inflight = None;
                    self.finish_automation_command(Some(id), receipt, cx);
                }
            }
            Err(error) => {
                self.automation_ui.pending_command = None;
                self.automation_ui.automation_command_inflight = None;
                self.automation_ui.feedback = Some(Err(error));
            }
        }
        cx.notify();
    }

    pub(crate) fn cancel_unstarted_automation_work(&mut self, occurrences: &[u64]) {
        let runs: Vec<u64> = self
            .store
            .runs()
            .iter()
            .filter(|run| {
                run.status == "Queued"
                    && run
                        .automation_occurrence_id
                        .is_some_and(|id| occurrences.contains(&id))
            })
            .map(|run| run.id)
            .collect();
        self.queued_runs.retain(|(run, _)| !runs.contains(run));
        let ids: Vec<u64> = self.store.runs().iter().filter(|r| runs.contains(&r.id)).map(|r| r.id).collect();
        for run_id in ids {
            let Some(mut run) = self.store.run_mut(run_id) else { continue };
            run.status = "Cancelled".into();
            run.ended_at = Some(crate::opencode::timestamp());
            run.logs.push(
                "[automation] Cancelled before starting because its request was replaced, paused or archived."
                    .into(),
            );
        }
    }

    pub fn pause_automation_ui(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.automation_command_busy() {
            self.automation_ui.feedback = Some(Err(
                "Wait for the pending save or planning receipt before pausing this automation."
                    .into(),
            ));
            cx.notify();
            return;
        }
        let command = crate::domain_command::Command::AutomationPause {
            id,
            now: Self::automation_now(),
        };
        let receipt = AutomationCommandReceipt::Pause {
            id,
            form_epoch: self.automation_ui.form_epoch,
            creating: self.automation_ui.creating,
        };
        self.automation_ui.pending_command = Some(receipt.clone());
        self.automation_ui.automation_command_inflight = Some(receipt.clone());
        match self.execute_command(command, cx) {
            Ok(crate::command_app::CommandRun::Saving { .. }) => {
                self.automation_ui.feedback =
                    Some(Ok("Saving… the pause applies after the durable receipt.".into()));
            }
            Ok(crate::command_app::CommandRun::Durable(_)) => {
                self.automation_ui.pending_command = None;
                if self.automation_ui.automation_command_inflight.is_some() {
                    self.automation_ui.automation_command_inflight = None;
                    self.finish_automation_command(Some(id), receipt, cx);
                }
            }
            Err(error) => {
                self.automation_ui.pending_command = None;
                self.automation_ui.automation_command_inflight = None;
                self.automation_ui.feedback = Some(Err(error));
            }
        }
        cx.notify();
    }

    pub fn resume_automation_schedule(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.automation_command_busy() {
            self.automation_ui.feedback = Some(Err(
                "Wait for the pending save or planning receipt before resuming this automation."
                    .into(),
            ));
            cx.notify();
            return;
        }
        let command = crate::domain_command::Command::ResumeSchedule {
            id,
            now: Self::automation_now(),
        };
        let receipt = AutomationCommandReceipt::Resume {
            id,
            form_epoch: self.automation_ui.form_epoch,
            creating: self.automation_ui.creating,
        };
        self.automation_ui.pending_command = Some(receipt.clone());
        self.automation_ui.automation_command_inflight = Some(receipt.clone());
        match self.execute_command(command, cx) {
            Ok(crate::command_app::CommandRun::Saving { .. }) => {
                self.automation_ui.feedback =
                    Some(Ok("Saving… the schedule resumes after the durable receipt.".into()));
            }
            Ok(crate::command_app::CommandRun::Durable(_)) => {
                self.automation_ui.pending_command = None;
                if self.automation_ui.automation_command_inflight.is_some() {
                    self.automation_ui.automation_command_inflight = None;
                    self.finish_automation_command(Some(id), receipt, cx);
                }
            }
            Err(error) => {
                self.automation_ui.pending_command = None;
                self.automation_ui.automation_command_inflight = None;
                self.automation_ui.feedback = Some(Err(error));
            }
        }
        cx.notify();
    }

    /// Retry one period in place. This is a period command: it never enables a
    /// paused schedule, never moves the cursor and never changes the definition.
    /// A terminal period reopens against its original approved snapshot; finished
    /// steps keep their results and the original reference time.
    pub fn retry_automation_occurrence(&mut self, occurrence: u64, cx: &mut Context<Self>) {
        let Some(occurrence) = self.store.occurrence(occurrence).cloned() else {
            return;
        };
        if let Some(blocker) = self.store.occurrence_retry_blocker(&occurrence) {
            self.automation_ui.feedback = Some(Err(blocker));
            cx.notify();
            return;
        }
        if occurrence.status.is_active() {
            self.enqueue_automation_occurrence(occurrence.id, cx);
            self.toast = "Resuming this period's unfinished steps.".into();
            cx.notify();
            return;
        }
        let command = crate::domain_command::Command::RetryOccurrence {
            occurrence: occurrence.id,
            now: Self::automation_now(),
        };
        match self.execute_command(command, cx) {
            Ok(crate::command_app::CommandRun::Saving { .. }) => {
                self.toast = "Saving… the period retry applies after the durable receipt.".into();
            }
            Ok(crate::command_app::CommandRun::Durable(_)) => {
                self.toast = format!(
                    "Retrying period {} in place · approved version v{} · finished steps keep their results.",
                    occurrence.id, occurrence.definition_version
                );
            }
            Err(error) => self.automation_ui.feedback = Some(Err(error)),
        }
        cx.notify();
    }

    pub fn run_automation_now(&mut self, id: u64, cx: &mut Context<Self>) {
        let command = crate::domain_command::Command::RunNow {
            automation: id,
            request_id: crate::opencode::new_session_id(),
            now: Self::automation_now(),
        };
        match self.execute_command(command, cx) {
            Ok(crate::command_app::CommandRun::Saving { .. }) => {
                self.automation_ui.feedback =
                    Some(Ok("Saving… the manual run queues after the durable receipt.".into()));
            }
            Ok(crate::command_app::CommandRun::Durable(_)) => {
                self.automation_ui.feedback = Some(Ok(
                    "Manual run queued. It uses the same approval and automation queue; the schedule did not move."
                        .into(),
                ));
            }
            Err(error) => self.automation_ui.feedback = Some(Err(error)),
        }
        cx.notify();
    }

    pub fn stop_automation_now(&mut self, id: u64, cx: &mut Context<Self>) {
        let occurrence = self.store.automation_active(id).map(|o| o.id);
        let Some(occurrence) = occurrence else {
            self.automation_ui.feedback = Some(Err("There is no active run to stop.".into()));
            cx.notify();
            return;
        };
        let outcome = self.stop_automation_safely(occurrence, cx);
        self.automation_ui.feedback = Some(stop_outcome_feedback(&outcome));
        cx.notify();
    }

    /// Cancel one active period (including one waiting for its retry) from the
    /// results list or its result dialog. The automation keeps its schedule.
    pub fn cancel_automation_occurrence_ui(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(occurrence) = self.store.occurrence(id) else {
            return;
        };
        if !occurrence.status.is_active() {
            self.toast = "This period is no longer active.".into();
            cx.notify();
            return;
        }
        let outcome = self.stop_automation_safely(id, cx);
        self.toast = match &outcome {
            crate::domain_command::StopOutcome::Stopped { .. } => {
                "Period cancelled · its scheduled retry was stopped".into()
            }
            crate::domain_command::StopOutcome::StopRequestedNotDurable { .. }
            | crate::domain_command::StopOutcome::StopRequestFailed { .. }
            | crate::domain_command::StopOutcome::AlreadyEnded { durable: false } => {
                "Stop requested · cancellation not saved · see the footer".into()
            }
            crate::domain_command::StopOutcome::AlreadyEnded { durable: true } => {
                "This period already ended · no restart will be attempted".into()
            }
        };
        cx.notify();
    }

    /// Kept for the list-level workflow that will own Copy and Archive; both
    /// actions are intentionally absent from the detail page for now.
    #[allow(dead_code)]
    pub fn copy_automation_ui(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let command = crate::domain_command::Command::Copy {
            id,
            now: Self::automation_now(),
        };
        match self.execute_command(command, cx) {
            Ok(crate::command_app::CommandRun::Saving { .. }) => {
                self.automation_ui.feedback = Some(Ok(
                    "Saving… the copy is created after the durable receipt.".into(),
                ));
            }
            Ok(crate::command_app::CommandRun::Durable(receipt)) => {
                if let Some(copy) = receipt.subject {
                    self.open_automation(copy, window, cx);
                }
                self.automation_ui.feedback = Some(Ok(
                    "Copied as an unapproved draft. History was not copied.".into(),
                ));
            }
            Err(error) => self.automation_ui.feedback = Some(Err(error)),
        }
        cx.notify();
    }

    #[allow(dead_code)]
    pub fn archive_automation_ui(&mut self, id: u64, cx: &mut Context<Self>) {
        let command = crate::domain_command::Command::Archive {
            id,
            now: Self::automation_now(),
        };
        match self.execute_command(command, cx) {
            Ok(crate::command_app::CommandRun::Saving { .. }) => {
                self.automation_ui.feedback =
                    Some(Ok("Saving… the archive applies after the durable receipt.".into()));
            }
            Ok(crate::command_app::CommandRun::Durable(_)) => {
                self.automation_ui.selected = None;
                self.automation_ui.creating = false;
                self.toast = "Automation archived · history kept".into();
            }
            Err(error) => self.automation_ui.feedback = Some(Err(error)),
        }
        cx.notify();
    }

    pub fn toggle_automation_read(&mut self, occurrence: u64, cx: &mut Context<Self>) {
        let unread = self.store.occurrence(occurrence).is_some_and(|o| o.unread);
        let command = crate::domain_command::Command::SetRead {
            occurrence,
            unread: !unread,
        };
        if let Err(error) = self.execute_command(command, cx) {
            self.automation_ui.feedback = Some(Err(error));
        }
        cx.notify();
    }

    pub fn automation_cleanup_preview(&mut self, id: u64, cx: &mut Context<Self>) {
        match self
            .store
            .automation_log_cleanup(id, Self::automation_now(), false)
        {
            Ok((occurrences, bytes, _)) => {
                self.automation_ui.cleanup_preview = format!(
                    "{occurrences} completed period(s), {:.1} KB of detailed logs. Reports, sources, materials and recovery records are kept.",
                    bytes as f64 / 1024.
                );
            }
            Err(error) => self.automation_ui.cleanup_preview = error,
        }
        self.automation_ui.show_cleanup = true;
        cx.notify();
    }

    pub fn automation_cleanup_apply(&mut self, id: u64, cx: &mut Context<Self>) {
        let command = crate::domain_command::Command::CleanupLogs {
            id,
            now: Self::automation_now(),
            apply: true,
        };
        match self.execute_command(command, cx) {
            Ok(crate::command_app::CommandRun::Saving { .. }) => {
                self.automation_ui.cleanup_preview =
                    "Saving… cleanup completes after the durable receipt.".into();
            }
            Ok(crate::command_app::CommandRun::Durable(receipt)) => {
                if let Some(feedback) = receipt.feedback {
                    self.automation_ui.cleanup_preview = format!(
                        "{} · {}. Results and recovery records remain.",
                        feedback.title, feedback.detail
                    );
                }
                self.automation_ui.feedback = Some(Ok("Log cleanup completed.".into()));
            }
            Err(error) => self.automation_ui.cleanup_preview = error,
        }
        cx.notify();
    }
}

fn set_input(
    cx: &mut Context<TaskApp>,
    input: &Entity<InputState>,
    value: &str,
    window: &mut Window,
) {
    let value = value.to_string();
    input.update(cx, |state, cx| state.set_value(value, window, cx));
}
fn set_textarea(
    cx: &mut Context<TaskApp>,
    input: &Entity<TextareaState>,
    value: &str,
    window: &mut Window,
) {
    let value = value.to_string();
    input.update(cx, |state, cx| state.set_value(value, window, cx));
}

/// Small read-only presentation helpers used by the views below.
pub fn latest_occurrence<'a>(
    store: &'a Store,
    automation: u64,
) -> Option<&'a AutomationOccurrence> {
    crate::automation_activity::newest(
        store
            .all_occurrences()
            .iter()
            .filter(|o| o.automation_id == automation),
    )
}

pub fn occurrence_report<'a>(
    store: &'a Store,
    occurrence: &AutomationOccurrence,
) -> Option<&'a crate::domain::AgentRun> {
    crate::result_presentation::occurrence_report(store, occurrence)
}

pub fn occurrence_status_color(
    status: OccurrenceStatus,
    palette: &crate::appearance::Palette,
) -> u32 {
    match status {
        OccurrenceStatus::Succeeded => palette.success,
        OccurrenceStatus::Failed => palette.danger,
        OccurrenceStatus::Running | OccurrenceStatus::WaitingRetry => palette.accent,
        OccurrenceStatus::Verifying => palette.warning,
        OccurrenceStatus::Skipped | OccurrenceStatus::Cancelled | OccurrenceStatus::Interrupted => {
            palette.muted
        }
        OccurrenceStatus::Queued => palette.secondary,
    }
}

/// Builder used by `views.rs` so automation fields do not crowd the main render.
impl TaskApp {
    pub(crate) fn automations(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.automation_ui.creating {
            return self.automation_wizard(cx);
        }
        match self.automation_ui.selected {
            Some(id) => self.automation_detail(id, cx),
            None => self.automation_list(cx),
        }
    }

    fn automation_feedback(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(feedback) = &self.automation_ui.feedback else {
            return div().into_any_element();
        };
        let (tone, text) = match feedback {
            Ok(text) => (crate::views::NoticeTone::Success, text.clone()),
            Err(text) => (crate::views::NoticeTone::Danger, text.clone()),
        };
        crate::views::notice(tone, text, cx).into_any_element()
    }

    /// Combine the durable planning state with the live in-memory token. The
    /// token may only add a "Planning…" hint while no durable state exists yet;
    /// it can never hide a terminal/needs-review state.
    fn effective_planning_state(
        &self,
        id: u64,
    ) -> crate::automation_planning_state::AutomationPlanningState {
        let derived = crate::automation_planning_state::automation_planning_state(&self.store, id);
        if derived == crate::automation_planning_state::AutomationPlanningState::None
            && self.automation_ui.planning_automation == Some(id)
        {
            crate::automation_planning_state::AutomationPlanningState::Planning
        } else {
            derived
        }
    }

    fn automation_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let mut definitions: Vec<_> = self.store.automations().iter().collect();
        definitions.sort_by_key(|definition| definition.id);
        let mut content = col()
            .id("automation-list-scroll")
            .track_scroll(&self.automation_ui.scroll)
            .flex_1()
            .h_full()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .p_6()
            .gap_4();
        content = content.child(
            row()
                .justify_between()
                .gap_4()
                .flex_shrink_0()
                .child(
                    col().flex_1().min_w_0().gap_1()
                        .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(selectable("Automations")))
                        .child(muted(
                            "Recurring information work. Runs happen only while the app is open and the computer is awake; missed periods follow the saved catch-up rule.",
                            cx,
                        )),
                )
                .child(
                    primary_button("new-automation", "New automation")
                        .flex_shrink_0()
                        .icon(assets::IconName::Plus)
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.start_automation_create(window, cx)
                        })),
                ),
        );
        if definitions.is_empty() {
            content = content.child(
                col()
                    .mt_6()
                    .p_6()
                    .gap_3()
                    .rounded(px(crate::appearance::GROUP_RADIUS))
                    .border_1()
                    .border_color(rgb(colors.line))
                    .bg(rgb(colors.panel))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("No automations yet")))
                    .child(muted(
                        "Quick tasks need one instruction and a schedule. Research automations plan once, then repeat the approved plan. Everything here lives under Automations, separate from your projects and topics.",
                        cx,
                    )),
            );
        }
        for definition in definitions {
            let id = definition.id;
            let latest = latest_occurrence(&self.store, id);
            let unread = self.store.automation_unread_count(id);
            let next = match definition.status {
                DefinitionStatus::Paused => "Paused".to_string(),
                DefinitionStatus::Archived => "Archived".to_string(),
                _ => definition
                    .cursor
                    .map(|value| format_in_zone(value, &definition.schedule.timezone))
                    .unwrap_or_else(|| "No future trigger".into()),
            };
            let status = match definition.status {
                DefinitionStatus::Enabled => "Enabled",
                DefinitionStatus::Paused => "Paused",
                DefinitionStatus::Draft => "Draft",
                DefinitionStatus::NeedsAttention => "Needs attention",
                DefinitionStatus::Archived => "Archived",
            };
            let latest_label = latest
                .map(|occurrence| {
                    let activity = occurrence_activity(occurrence);
                    let presentation =
                        crate::result_presentation::present_occurrence(&self.store, occurrence);
                    let suffix = presentation
                        .status_suffix()
                        .map(|suffix| format!(" · {suffix}"))
                        .unwrap_or_default();
                    format!(
                        "{} · {} {}{suffix}",
                        occurrence.status.label(),
                        activity.kind.label(),
                        format_in_zone_precise(activity.utc, &occurrence.timezone)
                    )
                })
                .unwrap_or_else(|| "Not run yet".into());
            let planning_state = self.effective_planning_state(id);
            let planning = planning_state
                != crate::automation_planning_state::AutomationPlanningState::None;
            let latest_label = if planning {
                planning_state.label().into()
            } else {
                latest_label
            };
            let summary = format!(
                "{} · Next: {next} · Latest: {latest_label}",
                schedule_summary(&definition.schedule)
            );
            content = content.child(
                row()
                    .id(format!("automation-row-{id}"))
                    .p_3()
                    .gap_3()
                    .items_center()
                    .flex_shrink_0()
                    .rounded(px(crate::appearance::GROUP_RADIUS))
                    .border_1()
                    .border_color(rgb(colors.line))
                    .bg(rgb(colors.panel))
                    .hover(|d| d.bg(rgb(colors.hover)))
                    .on_click(
                        cx.listener(move |app, _, window, cx| {
                            app.record_origin();
                            app.open_automation(id, window, cx)
                        }),
                    )
                    .child(
                        col()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                row()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_base()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(definition.name.clone()),
                                    )
                                    .child(
                                        row()
                                            .flex_shrink_0()
                                            .gap_1()
                                            .items_center()
                                            .child(chip(
                                                definition.shape.label(),
                                                colors.secondary,
                                                cx,
                                            ))
                                            .child(chip(
                                                status,
                                                crate::views::stage_color_for_definition(
                                                    definition.status,
                                                    cx,
                                                ),
                                                cx,
                                            ))
                                            .when(planning, |d| {
                                                d.child(
                                                    chip(planning_state.label(), colors.accent, cx)
                                                        .id(format!("automation-planning-{id}")),
                                                )
                                            })
                                            .when(unread > 0, |d| {
                                                d.child(chip(
                                                    format!("{unread} unread"),
                                                    colors.accent,
                                                    cx,
                                                ))
                                            }),
                                    ),
                            )
                            .child(
                                muted(summary.clone(), cx)
                                    .id(format!("automation-latest-{id}"))
                                    .w_full()
                                    .min_w_0()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .map(observed),
                            ),
                    )
                    .child(
                        gpui_kit::component::Icon::new(assets::IconName::ChevronRight)
                            .flex_shrink_0()
                            .size(px(16.))
                            .text_color(rgb(colors.muted)),
                    ),
            );
        }
        let unassigned = self.store.unassigned_legacy_planning_runs();
        if !unassigned.is_empty() {
            let mut section = col().mt_4().gap_2().pt_3().border_t_1().border_color(rgb(colors.line))
                .child(overview_heading("Legacy automation planning history"))
                .child(overview_note(
                    "These saved planning attempts belong to an automation project, but their original automation could not be verified. They stay here instead of Research results and are never assigned by guesswork.",
                    cx,
                ));
            for run in unassigned {
                let run_id = run.id;
                let status = run.status.clone();
                let opener_focus = self.automation_ui.planning_opener_focus(run_id, cx);
                section = section.child(
                    row()
                        .gap_2()
                        .items_center()
                        .child(div().flex_1().child(selectable(format!(
                            "Run {run_id} · {status} · owner unassigned"
                        ))))
                        .child(
                            quiet_button(format!("legacy-planning-attempt-{run_id}"), "Review")
                                .icon(assets::IconName::ArrowRight)
                                .tooltip("Open this unassigned legacy planning attempt")
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    app.open_planning_attempt(run_id, window, cx)
                                }))
                                .track_focus(&opener_focus),
                        ),
                );
            }
            content = content.child(section);
        }
        col()
            .size_full()
            .flex_1()
            .min_h_0()
            .child(div().px_6().pt_3().child(self.automation_feedback(cx)))
            .child(content.into_any_element())
            .into_any_element()
    }

    /// All automation periods in one place, separate from project/topic results.
    pub(crate) fn automation_results(&self, cx: &mut Context<Self>) -> AnyElement {
        let filter = self
            .automation_ui
            .result_filter
            .filter(|id| self.store.automation(*id).is_some());
        let epoch = self.store.dirty_epoch();
        let plan = {
            let mut cache = self.automation_ui.result_order.borrow_mut();
            cache.refresh(epoch, filter, &self.store);
            let occurrences = self.store.all_occurrences();
            let rows = cache
                .indices
                .iter()
                .take(self.automation_ui.history_limit)
                .filter_map(|index| occurrences.get(*index).map(|occurrence| (occurrence.id, *index)))
                .collect::<Vec<_>>();
            std::sync::Arc::new(AutomationResultsPlan::new(
                rows,
                cache.filtered_total,
                cache.all_total,
                cache.queued,
                cache.waiting,
                filter,
                self.automation_ui.history_limit,
            ))
        };

        // Reset only when the visible set/filter/limit changes. Unrelated saves
        // and read markers move dirty_epoch but must not discard list heights.
        // Full ordered visible identity: middle-row replacements or reorders must
        // invalidate measured heights, not only endpoint changes.
        let key = ResultListKey {
            filter,
            limit: plan.limit,
            ids: plan.rows.iter().map(|(id, _)| *id).collect(),
            items: plan.items.len(),
            structure: ResultListKey::structure_of(&plan.items),
        };
        let previous = self.automation_ui.result_list_key.borrow().clone();
        // Load more appends rows to the same filtered prefix. Splice instead of
        // reset so the visible row stays anchored and measured offsets survive.
        let append = previous.filter == filter
            && previous.limit < plan.limit
            && previous.items > 0
            && previous.ids.len() < plan.rows.len()
            && plan
                .rows
                .iter()
                .take(previous.ids.len())
                .map(|(id, _)| *id)
                .eq(previous.ids.iter().copied());
        let reset = key != previous && !append;
        if append {
            let prefix = 1 + previous.ids.len();
            self.automation_ui
                .result_list
                .splice(prefix..previous.items, plan.items.len().saturating_sub(prefix));
        } else if reset {
            self.automation_ui
                .result_list
                .reset_with_uniform_height(plan.items.len(), px(RESULT_ROW_HEIGHT_HINT));
        }
        if append {
            // This API maps every item to Unmeasured while preserving measured
            // size hints; it does not reset the scroll anchor. The all-item
            // conversion is why it is limited to reset/append/width changes.
            let _ = self
                .automation_ui
                .result_list
                .clone()
                .with_uniform_item_height(px(RESULT_ROW_HEIGHT_HINT));
        }
        *self.automation_ui.result_list_key.borrow_mut() = key;
        let handle = self.automation_ui.scroll.offset().y.as_f32();
        let current = self
            .automation_ui
            .result_list
            .scroll_px_offset_for_scrollbar()
            .y
            .as_f32();
        let step = result_scroll_step(
            reset,
            handle,
            self.automation_ui.result_scroll_synced.get(),
            current,
            self.automation_ui.result_scroll_target.get(),
            self.automation_ui.result_scroll_attempts.get(),
        );
        match step {
            ResultScrollStep::Apply(offset) => {
                // `scroll_to(item_ix: 0, offset_in_item: large)` is clamped by
                // ListState to the first item's height. A relative pixel scroll
                // is the supported way to restore an arbitrary offset.
                self.automation_ui
                    .result_list
                    .scroll_by(px(current - offset));
                self.automation_ui.result_scroll_target.set(Some(offset));
                self.automation_ui
                    .result_scroll_attempts
                    .set(self.automation_ui.result_scroll_attempts.get().saturating_add(1));
            }
            ResultScrollStep::Settle { handle, synced } => {
                self.automation_ui
                    .scroll
                    .set_offset(point(px(0.), px(handle)));
                self.automation_ui.result_scroll_synced.set(synced);
                self.automation_ui.result_scroll_target.set(None);
                self.automation_ui.result_scroll_attempts.set(0);
            }
            ResultScrollStep::WriteBack { handle, synced } => {
                self.automation_ui
                    .scroll
                    .set_offset(point(px(0.), px(handle)));
                self.automation_ui.result_scroll_synced.set(synced);
            }
            ResultScrollStep::Idle => {}
        }

        let entity = cx.entity().downgrade();
        // A definite, full-height flex column is required: the inner ListState
        // resolves its viewport from this parent. A plain block div leaves the
        // list at zero height and paints no items.
        let hint_state = self.automation_ui.result_list.clone();
        let hint_width = self.automation_ui.result_list_width.clone();
        col()
            .on_children_prepainted(move |bounds, _, _| {
                // Width changes make ListState drop every cached height hint.
                // Restore them only on that transition (or the first paint),
                // not after every frame, so measured rows are not re-measured.
                let width = bounds.first().map(|bounds| bounds.size.width.as_f32());
                if width.is_some() && hint_width.get() != width {
                    hint_width.set(width);
                    let _ = hint_state
                        .clone()
                        .with_uniform_item_height(px(RESULT_ROW_HEIGHT_HINT));
                }
            })
            .id("automation-results-scroll")
            .w_full()
            .h_full()
            .flex_1()
            .min_h_0()
            .child(
                list(
                    self.automation_ui.result_list.clone(),
                    move |index, _, cx| {
                        let Some(entity) = entity.upgrade() else {
                            return div().into_any_element();
                        };
                        entity.update(cx, |app, cx| {
                            app.automation_results_item(&plan, index, cx)
                        })
                    },
                )
                .w_full()
                .flex_1()
                .min_h_0(),
            )
            .map(observed)
            .into_any_element()
    }

    fn automation_results_header(
        &self,
        plan: &AutomationResultsPlan,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let heading = match plan.filter {
            Some(_) => format!("Periods · {} of {}", plan.total, plan.all_total),
            None => format!("Periods · {}", plan.total),
        };
        let mut panel = col()
            .w_full()
            .min_w_0()
            .p_6()
            .gap_4()
            .child(
                col()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .id("automation-results-title")
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(selectable("Automation results"))
                            .map(observed),
                    )
                    .child(crate::views::observed(
                        div()
                            .id("automation-results-subtitle")
                            .aria_label("Every automation period, newest activity first. Each row names the timestamp it shows; open it for its answer and sources.")
                            .w_full()
                            .min_w_0()
                            .child(muted(
                                "Every automation period, newest activity first. Each row names the timestamp it shows; open it for its answer and sources.",
                                cx,
                            )),
                    )),
            );
        if plan.total > 0 {
            panel = panel.child(
                col()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .child(
                        row()
                            .w_full()
                            .min_w_0()
                            .justify_between()
                            .flex_wrap()
                            .gap_3()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(selectable(heading)),
                            )
                            .child(self.automation_results_filter_control(cx)),
                    )
                    .child(crate::views::observed(
                        div()
                            .id("automation-results-unread-hint")
                            .aria_label("Unread results are marked with a dot.")
                            .w_full()
                            .min_w_0()
                            .child(muted("Unread results are marked with a dot.", cx)),
                    )),
            );
            if plan.queued > 0 || plan.waiting > 0 {
                panel = panel.child(overview_note(
                    format!(
                        "Automation slots: {} of {} running · {} queued · {} waiting to retry. Automation and research queues are independent.",
                        self.automation_slots_used(),
                        self.store.automation_concurrency_limit(),
                        plan.queued,
                        plan.waiting
                    ),
                    cx,
                ));
            }
        }
        panel.into_any_element()
    }

    fn automation_results_item(
        &self,
        plan: &AutomationResultsPlan,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match plan.items[index] {
            AutomationResultsItem::Header => self.automation_results_header(plan, cx),
            AutomationResultsItem::Row(row) => {
                let (id, position) = plan.rows[row];
                let occurrence = self
                    .store
                    .all_occurrences()
                    .get(position)
                    .filter(|occurrence| occurrence.id == id)
                    .or_else(|| self.store.occurrence(id));
                let Some(occurrence) = occurrence else {
                    return div().into_any_element();
                };
                div()
                    .w_full()
                    .min_w_0()
                    .px_6()
                    .pb_2()
                    .child(self.automation_occurrence_row(occurrence, true, cx))
                    .into_any_element()
            }
            AutomationResultsItem::EmptyAll => div()
                .p_6()
                .child(overview_note(
                    "No automation runs yet. Create or enable an automation to collect periods here.",
                    cx,
                ))
                .into_any_element(),
            AutomationResultsItem::EmptyFiltered => div()
                .p_6()
                .child(overview_note(
                    "No periods for this automation yet. Pick another automation or clear the filter.",
                    cx,
                ))
                .into_any_element(),
            AutomationResultsItem::LoadMore => div()
                .px_6()
                .pb_6()
                .child(
                    quiet_button(
                        "load-more-automation-results",
                        format!(
                            "Load more · {} remaining",
                            plan.total.saturating_sub(plan.limit)
                        ),
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.automation_ui.history_limit += 20;
                        cx.notify();
                    })),
                )
                .into_any_element(),
        }
    }

    fn automation_results_filter_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.automation_ui.result_filter;
        let label = selected
            .and_then(|id| self.store.automation(id))
            .map(|definition| definition.name.clone())
            .unwrap_or_else(|| "All automations".into());
        let mut definitions: Vec<_> = self.store.automations().iter().collect();
        definitions.sort_by_key(|definition| definition.id);
        let options: Vec<(Option<u64>, String)> =
            std::iter::once((None, "All automations".to_string()))
                .chain(
                    definitions
                        .into_iter()
                        .map(|definition| (Some(definition.id), definition.name.clone())),
                )
                .collect();
        let entity = cx.entity().downgrade();
        quiet_button("automation-results-filter", label)
            .dropdown_caret(true)
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(palette(cx).accent))
            .bg(rgb(palette(cx).accent_surface))
            .tooltip("Filter results by automation")
            .accessibility_label("Filter results by automation")
            .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                for (id, name) in &options {
                    let value = *id;
                    let entity = entity.clone();
                    menu = menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(selected == value)
                            .on_click(move |_, _, cx| {
                                let _ = entity.update(cx, |app, cx| {
                                    app.automation_ui.result_filter = value;
                                    app.automation_ui.history_limit = 20;
                                    app.automation_ui.scroll.set_offset(point(px(0.), px(0.)));
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    fn automation_detail(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let Some(definition) = self.store.automation(id) else {
            return self.automation_list(cx);
        };
        let colors = palette(cx);
        let project = self
            .store
            .projects()
            .iter()
            .find(|p| p.id == definition.project_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Project unavailable".into());
        let active = self.store.automation_active(id).map(|o| o.id);
        let mut actions = row().flex_wrap().gap_2();
        if definition.status == DefinitionStatus::Draft {
            actions = actions.child(
                primary_button("enable-draft-automation", "Review and enable")
                    .icon(assets::IconName::Check)
                    .on_click(
                        cx.listener(move |app, _, window, cx| {
                            app.record_origin();
                            app.edit_automation(id, window, cx)
                        }),
                    ),
            );
        }
        // One lifecycle control stays in the same place: Pause while scheduled,
        // Resume once paused or needing attention. Drafts show it disabled so
        // the control does not appear to vanish before the first enable.
        match definition.status {
            DefinitionStatus::Enabled => {
                actions = actions.child(
                    button("pause-automation", "Pause")
                        .icon(assets::IconName::Pause)
                        .tooltip("Stop future triggers and cancel unstarted requests. A running period can finish.")
                        .on_click(cx.listener(move |app, _, _, cx| app.pause_automation_ui(id, cx))),
                );
            }
            DefinitionStatus::Paused | DefinitionStatus::NeedsAttention => {
                actions =
                    actions.child(
                        primary_button("resume-automation", "Resume schedule")
                            .icon(assets::IconName::Play)
                            .tooltip("Enable again from now; paused time is not backfilled.")
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.resume_automation_schedule(id, cx)
                            })),
                    );
            }
            DefinitionStatus::Draft => {
                actions = actions.child(
                    button("pause-automation", "Pause")
                        .icon(assets::IconName::Pause)
                        .disabled(true)
                        .tooltip("Enable this automation before pausing it."),
                );
            }
            DefinitionStatus::Archived => {}
        }
        if definition.status != DefinitionStatus::Archived {
            // Manual trigger. It stays visible in every live state so it can be
            // found; when it cannot start work, the reason is stated here and
            // just below the action row.
            let run_blocked = (!definition.approval_current())
                .then_some("Review and enable this version before running it.");
            actions = actions.child(
                button("run-automation", "Run manually")
                    .icon(assets::IconName::Play)
                    .when(definition.status == DefinitionStatus::Enabled, |b| {
                        b.primary().font_weight(FontWeight::MEDIUM)
                    })
                    .disabled(run_blocked.is_some())
                    .tooltip(
                        run_blocked
                            .unwrap_or("Queue one manual period. The schedule does not move."),
                    )
                    .on_click(cx.listener(move |app, _, _, cx| app.run_automation_now(id, cx))),
            );
        }
        if let Some(occurrence) = active {
            actions = actions.child(
                danger_button("stop-automation", "Stop this run")
                    .icon(assets::IconName::CircleX)
                    .tooltip("Cancel the active period and its scheduled retries; future triggers continue.")
                    .on_click(cx.listener(move |app, _, _, cx| app.stop_automation_now(id, cx))),
            );
            let _ = occurrence;
        }
        // Configuration lives behind one Settings dialog. Edit and Replan sit in
        // that dialog next to the settings they change, so the page keeps only
        // execution actions.
        actions = actions.child(
            button("automation-settings", "Settings")
                .icon(assets::IconName::Settings2)
                .tooltip("Review the task, schedule, run limits and approved execution snapshot.")
                .on_click(
                    cx.listener(move |app, _, window, cx| app.open_automation_settings(window, cx)),
                ),
        );
        // Copy and Archive are deliberately not surfaced yet. The handlers stay
        // for the list-level workflows that will own them.

        let mut panel = col()
            .flex_1()
            .h_full()
            .min_h_0()
            .min_w_0()
            .id("automation-detail-scroll")
            .track_scroll(&self.automation_ui.scroll)
            .overflow_y_scroll()
            .p_6()
            .gap_4();
        panel = panel
            // Back is the app-level breadcrumb action; the page does not repeat it.
            .child(
                col().min_w_0().gap_1()
                    .child(
                        div()
                            .w_full()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(relative(1.3))
                            .child(selectable(definition.name.clone())),
                    )
                    .child(muted(schedule_summary(&definition.schedule), cx))
                    .child(overview_meta_line(definition, &project, cx))
                    .when(definition.shape.is_quick(), |d| {
                        d.child(overview_note(
                            "One informational answer per period, stored under Automations — no project, topic or planning chain.",
                            cx,
                        ))
                    }),
            )
            // Planning is durable: a closed wizard, a list visit and a restart
            // keep the state and its review path visible.
            .when(
                self.effective_planning_state(id)
                    != crate::automation_planning_state::AutomationPlanningState::None,
                |d| {
                    let state = self.effective_planning_state(id);
                    let (tone, title, body, action) = match state {
                        crate::automation_planning_state::AutomationPlanningState::Planning => (
                            crate::views::NoticeTone::Info,
                            "Planning the research plan…",
                            "You can leave this page; planning continues in the background. When it finishes, review the generated goals and steps before enabling this draft.",
                            "Review planning",
                        ),
                        crate::automation_planning_state::AutomationPlanningState::PlanReady => (
                            crate::views::NoticeTone::Info,
                            "Plan ready to review",
                            "The planner finished this request. Review the goals and steps before enabling the draft; nothing runs until you approve it.",
                            "Review plan",
                        ),
                        _ => (
                            crate::views::NoticeTone::Warning,
                            "Planning needs attention",
                            "This draft has a planning request that did not produce a reviewed plan. Review it and retry against the same request; the saved draft stays editable.",
                            "Review or retry",
                        ),
                    };
                    d.child(
                        col().gap_2().child(crate::views::notice_panel(tone, title, body, cx))
                            .child(
                                quiet_button("review-automation-planning", action)
                                    .icon(assets::IconName::ListOrdered)
                                    .tooltip("Return to the planning step and review or retry the saved request.")
                                    .on_click(cx.listener(move |app, _, window, cx| {
                                        app.review_automation_planning(id, window, cx)
                                    })),
                            ),
                    )
                },
            )
            // Feedback belongs at the top of the content, never between actions.
            .child(self.automation_feedback(cx))
            // Attention is about this automation's state, so it stays visible
            // even while the settings are folded away.
            .when_some(definition.attention_reason.clone(), |d, reason| {
                d.child(crate::views::notice(
                    crate::views::NoticeTone::Warning,
                    reason,
                    cx,
                ))
            })
            .child(actions)
            .when(definition.status == DefinitionStatus::Draft, |d| {
                d.child(overview_note(
                    "Pause and Run manually unlock once you review and enable this draft.",
                    cx,
                ))
            })
            // Separate the definition and its actions from the run history.
            .child(div().w_full().h(px(1.)).bg(rgb(colors.line)));

        // History is a compact, newest-first list. Each row opens its result in
        // a dialog; nothing expands inside the page.
        // In live mode the indexed reader owns the visible page order and
        // pagination. The in-memory store remains the row payload source, but
        // the page query is the actual history path (no full-history scan).
        let paged_ids: Option<Vec<u64>> = self
            .history_cache
            .get(&id)
            .filter(|cache| !cache.rows.is_empty())
            .map(|cache| cache.rows.iter().map(|row| row.id).collect());
        let paged_total = self
            .history_cache
            .get(&id)
            .map(|cache| cache.total)
            .unwrap_or(0);
        let paged_more = self
            .history_cache
            .get(&id)
            .is_some_and(|cache| cache.next.is_some());
        let mut occurrences: Vec<_> = match &paged_ids {
            Some(ids) => ids
                .iter()
                .filter_map(|id| self.store.occurrence(*id))
                .collect(),
            None => self
                .store
                .all_occurrences()
                .iter()
                .filter(|o| o.automation_id == id)
                .collect(),
        };
        if paged_ids.is_none() {
            occurrences.sort_by(|a, b| compare_newest_first(a, b));
        }
        let total = if paged_ids.is_some() {
            paged_total.max(occurrences.len())
        } else {
            occurrences.len()
        };
        let shown = if paged_ids.is_some() {
            occurrences.clone()
        } else {
            occurrences
                .iter()
                .take(self.automation_ui.history_limit)
                .copied()
                .collect::<Vec<_>>()
        };
        if occurrences.is_empty() {
            panel = panel.child(overview_note(
                "No runs yet. Enabling or Run manually will create the first period.",
                cx,
            ));
        } else {
            panel = panel.child(
                row()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(selectable(format!("History · {total}"))),
                    )
                    .child(overview_note(
                        "Newest activity first. Unread results are marked with a dot.",
                        cx,
                    )),
            );
            let mut history = col().gap_2();
            for occurrence in shown {
                history = history.child(self.automation_occurrence_row(occurrence, false, cx));
            }
            panel = panel.child(history);
            if paged_ids.is_some() {
                if paged_more {
                    panel = panel.child(
                        quiet_button("load-more-history", "Load more history".to_string())
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.load_more_automation_history(id, cx);
                            })),
                    );
                }
            } else if occurrences.len() > self.automation_ui.history_limit {
                let remaining = occurrences.len() - self.automation_ui.history_limit;
                panel = panel.child(
                    quiet_button(
                        "load-more-history",
                        format!("Load more · {remaining} remaining"),
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.automation_ui.history_limit += 20;
                        cx.notify();
                    })),
                );
            }
            // Keep log cleanup visually separate from the run list.
            panel = panel.child(div().w_full().h(px(1.)).bg(rgb(colors.line)));
            panel = panel.child(
                row()
                    .gap_2()
                    .child(
                        quiet_button("automation-cleanup-preview", "Log cleanup preview")
                            .icon(assets::IconName::ListOrdered)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.automation_cleanup_preview(id, cx)
                            })),
                    )
                    .when(self.automation_ui.show_cleanup, |d| {
                        d.child(
                            primary_button("automation-cleanup-apply", "Clear old detailed logs")
                                .icon(assets::IconName::Archive)
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.automation_cleanup_apply(id, cx)
                                })),
                        )
                    }),
            );
            if !self.automation_ui.cleanup_preview.is_empty() {
                panel = panel.child(overview_note(
                    self.automation_ui.cleanup_preview.clone(),
                    cx,
                ));
            }
        }

        // App-wide behaviour is fine print at the end of the page, not a
        // per-automation setting inside the overview card.
        panel = panel.child(overview_note(
            "All automations run only while the app is open and the computer is awake — nothing runs in the background, and the app never wakes the computer. When the app is open again, catch-up follows the automation's setting: “Only the latest missed period” can run the newest missed period if it is within the allowed delay, while “Skip all missed periods” skips them; time spent paused is never made up.",
            cx,
        ));
        panel.map(observed).into_any_element()
    }

    /// Read-only settings in a modal. Edit and Replan live here because they act
    /// on exactly these settings, which keeps the page itself about runs.
    pub(crate) fn automation_settings_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(id) = self.automation_ui.selected else {
            return div().into_any_element();
        };
        let Some(definition) = self.store.automation(id) else {
            return div().into_any_element();
        };
        let colors = palette(cx);
        let paused = definition.status == DefinitionStatus::Paused;
        let archived = definition.status == DefinitionStatus::Archived;
        let next = next_trigger_labels(definition, Self::automation_now());
        let mut attempts: Vec<(u32, u64, String)> = self
            .store
            .research_requests()
            .iter()
            .filter(|request| request.automation_id == Some(id))
            .flat_map(|request| {
                self.store
                    .request_attempts(request.id)
                    .into_iter()
                    .map(move |run| (request.revision, run.id, run.status.clone()))
            })
            .collect();
        attempts.sort_by_key(|(revision, run, _)| (*revision, *run));
        let mut plan_history = col()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(rgb(colors.subtle_line))
            .id("planning-attempt-history")
            .child(overview_heading("Plan versions"));
        if attempts.is_empty() {
            plan_history = plan_history.child(overview_note("No planning attempts yet.", cx));
        } else {
            plan_history = plan_history.child(overview_note(
                "Read-only planning history. Saved planner output is not a research report.",
                cx,
            ));
            for (revision, run_id, status) in attempts {
                let opener_focus = self.automation_ui.planning_opener_focus(run_id, cx);
                plan_history = plan_history.child(
                    row()
                        .gap_2()
                        .items_center()
                        .child(div().flex_1().child(selectable(format!(
                            "Revision {revision} · run {run_id} · {status}"
                        ))))
                        .child(
                            quiet_button(format!("planning-attempt-{run_id}"), "Review")
                                .icon(assets::IconName::ArrowRight)
                                .tooltip("Open this planning attempt")
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    app.open_planning_attempt(run_id, window, cx)
                                }))
                                .track_focus(&opener_focus),
                        ),
                );
            }
        }
        let mut body = col().id("automation-settings-body").flex_1().min_h_0().overflow_y_scroll().gap_3()
            .track_scroll(&self.automation_ui.settings_scroll)
            .child(overview_heading("What it does"))
            .child(
                overview_body(definition.objective.clone(), cx)
                    .id("automation-objective")
                    .map(observed),
            )
            .child(overview_heading("Expected result"))
            .child(overview_body(definition.expected_result.clone(), cx))
            .when_some(
                definition
                    .template
                    .as_ref()
                    .filter(|_| definition.shape == AutomationShape::Research),
                |d, template| {
                    d.child(overview_heading("Approved research plan"))
                        .child(overview_body(
                            format!(
                                "{} goal(s) · runs unchanged each period; no replanning\n{}",
                                template.goals.len(),
                                template.summary()
                            ),
                            cx,
                        ))
                },
            )
            .child(plan_history.map(observed))
            .child(
                row().items_center().gap_2()
                    .child(overview_heading("Next runs"))
                    .when(paused, |d| {
                        d.child(chip(
                            "Paused",
                            crate::views::stage_color_for_definition(definition.status, cx),
                            cx,
                        ))
                    }),
            )
            .child(
                if paused {
                    crate::views::observed(
                        overview_note(
                            "Nothing runs until you resume. Resuming schedules the next run from that moment, and time spent paused is not backfilled.",
                            cx,
                        )
                        .id("next-runs-paused"),
                    )
                } else if archived {
                    overview_note("Archived — this automation no longer runs.", cx)
                        .into_any_element()
                } else {
                    overview_body(
                        if next.is_empty() { "No future trigger".into() } else { next.join("\n") },
                        cx,
                    )
                    .into_any_element()
                },
            )
            .child(
                col().gap_2().pt_3().border_t_1().border_color(rgb(colors.subtle_line))
                    .child(overview_heading("Run policy"))
                    .child(overview_note(run_policy_summary(&definition.policy), cx)),
            );
        if let Some(approval) = &definition.approval {
            let required = definition
                .template
                .as_ref()
                .map(|template| template.kinds())
                .unwrap_or_else(|| vec![crate::research::ResearchKind::Advise]);
            let models = ResearchKinds::labels()
                .iter()
                .enumerate()
                .filter(|(index, _)| {
                    index != &0
                        && crate::research::ResearchKind::ALL
                            .get(*index)
                            .is_some_and(|kind| required.contains(kind))
                })
                .map(|(index, label)| {
                    format!(
                        "{label}: {} · {}",
                        approval.models[index],
                        approval.agents[index].clone().unwrap_or_else(|| self
                            .store
                            .research_config()
                            .default_agent
                            .clone())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            body = body.child(
                col().pt_3().gap_2().border_t_1().border_color(rgb(colors.subtle_line))
                    .child(overview_heading("Approved execution snapshot"))
                    .child(overview_body(models, cx))
                    .child(overview_note("Each period runs through the same research pipeline as a manual run, with the agent and model shown above. Global model changes do not alter this enabled automation; editing the plan or the agent requires a new version and approval.", cx)),
            );
        }
        let footer = row().justify_between().items_center().gap_2()
            .child(
                if definition.shape == AutomationShape::Research && !archived {
                    quiet_button("replan-automation", "Replan")
                        .icon(assets::IconName::RefreshCw)
                        .tooltip("Plan the research again once. The new plan starts a new version you must approve.")
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.record_origin();
                            let _ = app.close_settings_overlay();
                            app.replan_automation(id, window, cx);
                        }))
                        .into_any_element()
                } else {
                    div().into_any_element()
                },
            )
            .child(
                row().gap_2()
                    .child(
                        button("close-automation-settings", "Close")
                            .on_click(cx.listener(|app, _, window, cx| app.close_automation_settings(window, cx))),
                    )
                    .when(!archived, |d| {
                        d.child(
                            primary_button("edit-automation", "Edit")
                                .icon(assets::IconName::Pencil)
                                .tooltip("Create a new version. You must review and enable it again.")
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    app.record_origin();
                                    let _ = app.close_settings_overlay();
                                    app.edit_automation(id, window, cx);
                                })),
                        )
                    }),
            );
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x00000080))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                col()
                    .w(px(680.))
                    .max_w_full()
                    .max_h(relative(0.9))
                    .p_6()
                    .gap_4()
                    .map(|d| crate::appearance::floating_surface(d, cx))
                    .child(
                        row()
                            .justify_between()
                            .items_start()
                            .gap_4()
                            .child(
                                col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_lg()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(selectable("Settings")),
                                    )
                                    .child(muted(schedule_summary(&definition.schedule), cx)),
                            )
                            .child(
                                tool_icon(
                                    "close-automation-settings-icon",
                                    assets::IconName::Close,
                                    "Close settings",
                                )
                                .on_click(cx.listener(
                                    |app, _, window, cx| app.close_automation_settings(window, cx),
                                )),
                            ),
                    )
                    .child(body.map(observed))
                    .child(footer),
            )
            .focus_trap("automation-settings-focus", &self.modal_focus)
            .into_any_element()
    }

    fn automation_occurrence_row(
        &self,
        occurrence: &AutomationOccurrence,
        show_automation: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = palette(cx);
        let id = occurrence.id;
        let epoch = self.store.dirty_epoch();
        self.automation_ui
            .run_index
            .borrow_mut()
            .ensure(epoch, &self.store);
        let suffix = {
            let index = self.automation_ui.run_index.borrow();
            crate::result_presentation::present_occurrence_indexed(
                &self.store,
                occurrence,
                index.index(),
            )
            .status_suffix()
        }
        .or_else(|| {
            // A lazy body has a durable report but no hydrated text yet, so the
            // presentation cannot classify it. Say that the report is saved
            // rather than implying the row has no evidence qualifier.
            let run_id = occurrence
                .final_run_id
                .or_else(|| occurrence.run_ids.last().copied())?;
            self.store
                .run_body_is_lazy(run_id)
                .then_some("saved report")
        });
        let status = match suffix {
            Some(suffix) => format!("{} · {suffix}", occurrence.status.label()),
            None => occurrence.status.label().to_string(),
        };
        let color = occurrence_status_color(occurrence.status, &colors);
        // Latest activity is the shared presentation key: a retry request first,
        // then actual start, manual request, scheduled trigger or reference time.
        let activity = occurrence_activity(occurrence);
        let when = format_in_zone_precise(activity.utc, &occurrence.timezone);
        let activity_text = format!("{} {when}", activity.kind.label());
        let definition = show_automation
            .then(|| self.store.automation(occurrence.automation_id))
            .flatten();
        let tint = definition.map(|_| {
            self.automation_ui.result_tints.tint(
                occurrence.automation_id,
                gpui_kit::component::Theme::global(cx).is_dark(),
            )
        });
        // The trigger and the skipped-period count are secondary facts: an icon
        // keeps them out of the row's reading order without hiding them.
        let (trigger_icon, trigger_name, trigger_label) = match occurrence.source {
            crate::automation::TriggerSource::Manual => {
                (assets::IconName::MousePointerClick, "Manual run", "Manual")
            }
            crate::automation::TriggerSource::Scheduled => (
                assets::IconName::CalendarClock,
                "Scheduled run",
                "Scheduled",
            ),
        };
        let definition_name = definition
            .map(|definition| definition.name.clone())
            .unwrap_or_else(|| "Automation period".into());
        let row_label = format!("{activity_text} · {status} · {definition_name}");
        let trigger_hint = if occurrence.skipped_count > 0 {
            format!(
                "{trigger_name} · {} earlier periods were summarized",
                occurrence.skipped_count
            )
        } else {
            trigger_name.into()
        };
        // Keep the action outside the flexible text column. Metadata can wrap
        // independently instead of pushing the only explicit action offscreen.
        let mut body = col().flex_1().min_w_0().gap_1();
        if let (Some(definition), Some(tint)) = (definition, tint) {
            body = body.child(
                row()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_shrink_0()
                            .px(px(5.))
                            .py(px(1.))
                            .rounded(px(4.))
                            .bg(rgb(colors.side))
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(colors.secondary))
                            .child(selectable(definition.shape.label())),
                    )
                    .child(
                        div()
                            .id(format!("occurrence-name-{id}"))
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgba(tint.text))
                            .truncate()
                            .tooltip({
                                let name = definition.name.clone();
                                move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(name.clone())
                                        .build(window, cx)
                                }
                            })
                            .child(selectable(definition.name.clone()))
                            .map(observed),
                    ),
            );
        }
        body = body.child(
            row()
                .w_full()
                .min_w_0()
                .flex_wrap()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .id(format!("occurrence-activity-{id}"))
                        .min_w_0()
                        .text_sm()
                        .child(selectable(activity_text))
                        .map(observed),
                )
                .child(
                    div()
                        .id(format!("occurrence-trigger-{id}"))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_color(rgb(colors.muted))
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(trigger_hint.clone())
                                .build(window, cx)
                        })
                        .child(gpui_kit::component::Icon::new(trigger_icon).size(px(14.)))
                        .child(div().text_size(px(11.)).child(selectable(trigger_label)))
                        .map(observed),
                )
                .child(
                    div()
                        .id(format!("occurrence-status-{id}"))
                        .aria_label(status.clone())
                        .min_w_0()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(color))
                        .child(selectable(status))
                        .map(observed),
                ),
        );
        row()
            .id(format!("automation-occurrence-{id}"))
            .aria_label(row_label)
            .w_full()
            .min_w_0()
            .px_2()
            .py_2()
            .gap_2()
            .items_center()
            .rounded(px(crate::appearance::GROUP_RADIUS))
            .bg(rgb(colors.panel))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(colors.hover)))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(8.))
                    .h(px(8.))
                    .rounded_full()
                    .when(!occurrence.unread, |d| d.opacity(0.0))
                    .bg(rgb(colors.accent)),
            )
            .child(body)
            .child(
                div().flex_shrink_0().child(
                    tool_icon(
                        format!("open-occurrence-{id}"),
                        assets::IconName::ChevronRight,
                        "Open this period's result",
                    )
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.open_occurrence_result(id, window, cx)
                    })),
                ),
            )
            .on_click(cx.listener(move |app, _, window, cx| {
                app.open_occurrence_result(id, window, cx)
            }))
            .map(observed)
            .into_any_element()
    }

    /// Keep short reports in the outer flow; virtualize long documents by block.
    fn automation_report_reader(
        &self, key: String, markdown: String, height: Pixels, cx: &mut Context<Self>,
    ) -> AnyElement {
        use gpui_kit::base::text::{TextView, TextViewState};
        let long = markdown.len() > 4096 || markdown.lines().count() > 40;
        let view = if long {
            let mut readers = self.automation_ui.report_readers.borrow_mut();
            // Retain only the active period's readers, including Back navigation.
            let prefix = format!("automation-result-{}-", self.period_overlay().unwrap_or(0));
            readers.retain(|key, _| key.starts_with(&prefix));
            let state = readers.entry(key.clone()).or_insert_with(||
                cx.new(|cx| TextViewState::markdown(&markdown, cx).scrollable(true))
            ).clone();
            state.update(cx, |state, cx| state.set_text(&markdown, cx));
            TextView::new(&state).scrollable(true).h(height)
        } else {
            TextView::markdown(key.clone(), markdown)
        }.markdown_extensions(crate::rich_markdown::extensions()).w_full()
            .on_link_click(|url, _, _, cx| {
                if url.starts_with("https://") || url.starts_with("http://") { cx.open_url(url); }
            });
        if !long { return view.into_any_element(); }
        let state = self.automation_ui.report_readers.borrow()[&key].clone();
        let outer = self.automation_ui.dialog_scroll.clone();
        div().id(format!("{key}-reader")).relative().w_full().h(height).flex_shrink_0()
            .child(canvas(|_, _, _| (), move |bounds, _, window, _| {
                let visible = bounds.intersect(&window.content_mask().bounds);
                let state = state.clone();
                let outer = outer.clone();
                // Register before the child so capture runs before its bounce handler.
                window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture || !visible.contains(&event.position) { return; }
                    let delta = event.delta.pixel_delta(window.line_height());
                    if delta.y == px(0.) || delta.x.abs() > delta.y.abs() { return; }
                    // Reveal a clipped reader before scrolling text inside it.
                    let reveal = if delta.y < px(0.) && bounds.bottom() > visible.bottom() {
                        delta.y.max(visible.bottom() - bounds.bottom())
                    } else if delta.y > px(0.) && bounds.top() < visible.top() {
                        delta.y.min(visible.top() - bounds.top())
                    } else { px(0.) };
                    let outer_next = (outer.offset().y + reveal).clamp(-outer.max_offset().y, px(0.));
                    if reveal != px(0.) && outer_next != outer.offset().y {
                        outer.set_offset(point(outer.offset().x, outer_next));
                        window.refresh();
                        cx.stop_propagation();
                        return;
                    }
                    let list = state.read(cx).list_state().clone();
                    let offset = list.scroll_px_offset_for_scrollbar();
                    let max = list.max_offset_for_scrollbar();
                    let next = (offset.y + delta.y).clamp(-max.y, px(0.));
                    let at_edge = if delta.y < px(0.) { offset.y <= -max.y + px(0.5) } else { offset.y >= px(-0.5) };
                    if !at_edge {
                        list.set_offset_from_scrollbar(point(offset.x, next));
                        state.update(cx, |_, cx| cx.notify());
                    } else {
                        outer.set_offset(point(outer.offset().x, (outer.offset().y + delta.y).clamp(-outer.max_offset().y, px(0.))));
                    }
                    window.refresh();
                    cx.stop_propagation();
                });
            }).absolute().inset_0()).child(view).map(observed).into_any_element()
    }

    fn automation_occurrence_detail(
        &self,
        id: u64,
        presentation: &crate::result_presentation::ResultPresentation,
        reader_height: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(occurrence) = self.store.occurrence(id) else {
            return div().into_any_element();
        };
        let colors = palette(cx);
        let mut panel = col()
            .id("automation-occurrence-detail")
            .p_4()
            .gap_3()
            .rounded(px(crate::appearance::GROUP_RADIUS))
            .bg(rgb(colors.side));
        if let Some(reason) = &occurrence.reason {
            panel = panel.child(crate::views::notice(
                crate::views::NoticeTone::Warning,
                reason.detail.clone(),
                cx,
            ));
        }
        if let Some(definition) = self.store.automation(occurrence.automation_id)
            && definition.version != occurrence.definition_version
        {
            panel = panel.child(crate::views::notice(
                crate::views::NoticeTone::Warning,
                format!(
                    "This period keeps its original approved version v{}. The automation is now v{}; retrying runs the original plan, not the current template. To use the current version, close this result and choose Run manually on the automation page.",
                    occurrence.definition_version, definition.version
                ),
                cx,
            ));
        }
        if matches!(
            occurrence.status,
            OccurrenceStatus::Queued | OccurrenceStatus::WaitingRetry
        ) {
            let retry = if occurrence.status == OccurrenceStatus::WaitingRetry {
                " The retry starts automatically and does not take a research slot."
            } else {
                " The research queue and its limit are separate."
            };
            panel = panel.child(overview_note(
                format!(
                    "Automation slots: {} of {} running.{retry}",
                    self.automation_slots_used(),
                    self.store.automation_concurrency_limit()
                ),
                cx,
            ));
        }
        if presentation.freshness.regenerating {
            panel = panel.child(overview_note(
                "A new report is being generated from existing evidence. The previous report remains visible and is labelled Regenerating.",
                cx,
            ));
        }
        let final_run = occurrence
            .final_run_id
            .or_else(|| occurrence.run_ids.last().copied());
        let body_state = final_run.and_then(|run_id| self.run_body_state.get(&run_id));
        let lazy_final_run = final_run
            .filter(|run_id| self.store.run_body_is_lazy(*run_id))
            .filter(|_| {
                !matches!(
                    body_state,
                    Some(crate::history_reader::BodyLoad::Absent)
                        | Some(crate::history_reader::BodyLoad::Failed { .. })
                )
            });
        if lazy_final_run.is_some() {
            let message = if occurrence.status == OccurrenceStatus::Succeeded {
                "Loading the saved report body… The period is complete; only its saved result text is being fetched."
            } else {
                "Loading a saved step result… This period is still in progress; completed steps keep their saved results."
            };
            panel = panel.child(
                crate::views::notice(crate::views::NoticeTone::Info, message, cx)
                    .id("automation-report-loading")
                    .aria_label("Loading saved report")
                    .map(observed),
            );
        } else if matches!(
            body_state,
            Some(crate::history_reader::BodyLoad::Failed { .. })
        ) {
            let occurrence_id = occurrence.id;
            panel = panel.child(
                col().gap_2()
                    .child(
                        crate::views::notice(
                            crate::views::NoticeTone::Warning,
                            "The saved report body could not be loaded. Dependent work will not use an empty body; retry just this report read.",
                            cx,
                        )
                        .id("automation-report-unavailable")
                        .aria_label("Saved report body unavailable")
                        .map(observed),
                    )
                    .child(
                        quiet_button("automation-report-retry", "Retry loading report")
                            .icon(assets::IconName::RefreshCw)
                            .tooltip("Retry reading this saved report body only; persistence and the period are unchanged.")
                            .on_click(cx.listener(move |app, _, _, cx| {
                                if app.retry_occurrence_body(occurrence_id) {
                                    cx.notify();
                                }
                            })),
                    ),
            );
        } else if matches!(
            body_state,
            Some(crate::history_reader::BodyLoad::Absent)
        ) {
            panel = panel.child(
                crate::views::notice(
                    crate::views::NoticeTone::Warning,
                    "No saved report body exists for this run. Existing step results remain available; this run is not presented as complete.",
                    cx,
                )
                .id("automation-report-missing")
                .aria_label("Saved report body missing")
                .map(observed),
            );
        } else if !presentation.has_final_report {
            panel = panel.child(
                crate::views::notice(
                    crate::views::NoticeTone::Info,
                    "No final report yet. Completed steps and any earlier results remain available below; live progress is not a validated answer.",
                    cx,
                )
                .id("automation-no-final-notice")
                .aria_label("No final report yet")
                .map(observed),
            );
        }
        let single_complete_report = presentation.reports.len() == 1
            && presentation.has_final_report
            && presentation.quality == crate::result_presentation::EvidenceQuality::Complete;
        let show_state = !single_complete_report
            || presentation.freshness.stale_input
            || presentation.freshness.regenerating
            || presentation.freshness.historical
            || presentation.freshness.legacy_contract;
        let state_label = presentation.state_label();
        if show_state {
            panel = panel.child(
                div()
                    .id("automation-result-state")
                    .aria_label(state_label.clone())
                    .text_xs()
                    .child(selectable(state_label))
                    .map(observed),
            );
        }
        if presentation.reports.is_empty() && presentation.has_reader_body() {
            panel = panel.child(
                div().p_3().rounded(px(crate::appearance::GROUP_RADIUS)).bg(rgb(colors.panel)).child(
                    self.automation_report_reader(
                        format!("automation-result-{id}-fallback"),
                        presentation.reader_markdown.clone(), reader_height, cx,
                    ),
                ),
            );
            panel = panel.child(self.automation_result_sources(
                id,
                "fallback",
                &presentation.sources,
                cx,
            ));
        }
        for report in &presentation.reports {
            let result = &report.presentation;
            if result.run_id.is_none()
                && result.kind == crate::result_presentation::ResultKind::None
            {
                panel = panel.child(muted(
                    format!("{} · No final report yet", report.owner_label),
                    cx,
                ));
                continue;
            }
            if !single_complete_report {
                let mut heading = format!("{} · {}", report.owner_label, result.kind.label());
                if let Some(suffix) = result.status_suffix() {
                    heading.push_str(&format!(" · {suffix}"));
                }
                panel = panel.child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(selectable(heading)),
                );
            }
            // The version warning above already explains the frozen instructions.
            let freshness = crate::result_presentation::Freshness {
                previous_version: false,
                ..result.freshness
            }.labels();
            if !freshness.is_empty() {
                panel = panel.child(muted(freshness.join(" · "), cx));
            }
            if result.notice.is_some() {
                panel = panel.child(crate::views::notice(
                    crate::views::NoticeTone::Warning,
                    crate::research::EVIDENCE_GAP_NOTICE,
                    cx,
                ));
            }
            if result.has_reader_body() {
                let run_key = result.run_id.unwrap_or(0);
                panel = panel.child(
                    div()
                        .id(format!("automation-report-body-{id}-{run_key}"))
                        .aria_label("Retained report body")
                        .p_3().rounded(px(crate::appearance::GROUP_RADIUS)).bg(rgb(colors.panel)).child(
                        self.automation_report_reader(
                            format!("automation-result-{id}-{run_key}"),
                            result.reader_markdown.clone(), reader_height, cx,
                        ),
                    ).map(observed),
                );
                if !result.sources.is_empty() {
                    panel = panel.child(self.automation_result_sources(
                        id,
                        &format!("report-{run_key}"),
                        &result.sources,
                        cx,
                    ));
                }
            } else if result.quality == crate::result_presentation::EvidenceQuality::Rejected {
                panel = panel.child(muted(
                    "The saved output did not pass report validation. Raw output remains in Technical details.",
                    cx,
                ));
            } else if result.quality == crate::result_presentation::EvidenceQuality::NotValidated
                && result.kind != crate::result_presentation::ResultKind::LiveProgress
            {
                panel = panel.child(muted(
                    "This attempt has no validated reader report. Raw output remains in Technical details.",
                    cx,
                ));
            }
        }
        if !presentation.progress.is_empty() {
            panel = panel.child(
                div()
                    .id("automation-live-progress")
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(selectable("Live progress · not a validated answer"))
                    .map(observed),
            );
            for progress in &presentation.progress {
                panel = panel.child(
                    col()
                        .p_3()
                        .gap_1()
                        .rounded(px(crate::appearance::GROUP_RADIUS))
                        .border_1()
                        .border_color(rgb(colors.line))
                        .child(label(
                            format!("{} · {}", progress.owner_label, progress.step_label),
                            cx,
                        ))
                        .child(selectable(progress.text.clone())),
                );
            }
        }
        // Steps remain normal research entities but are reached only from here.
        // Their full titles are shown, not just identifiers, so a research
        // automation's split work is readable at a glance.
        panel = panel.child(self.automation_occurrence_steps(occurrence, cx));
        let runs: Vec<_> = self
            .store
            .runs()
            .iter()
            .filter(|run| run.automation_occurrence_id == Some(id))
            .collect();
        panel = panel.child(
            quiet_button("automation-technical", "Technical details")
                .px_0()
                .self_start()
                .icon(if self.automation_ui.show_technical {
                    assets::IconName::ChevronDown
                } else {
                    assets::IconName::ChevronRight
                })
                .on_click(cx.listener(|app, _, _, cx| {
                    app.automation_ui.show_technical = !app.automation_ui.show_technical;
                    cx.notify();
                })),
        );
        if self.automation_ui.show_technical {
            let mut technical = col().pl_6().gap_3();
            technical = technical.child(
                col().id("automation-period-diagnostics").gap_1()
                    .child(label(occurrence_context(
                        self.store.automation(occurrence.automation_id), occurrence, &self.store,
                    ), cx))
                    .child(label("Saved task instructions", cx))
                    .child(selectable(occurrence.snapshot.objective.clone()))
                    .child(label("Saved expected result", cx))
                    .child(selectable(occurrence.snapshot.expected_result.clone()))
                    .map(observed),
            );
            let selected = self.automation_ui.technical_run
                .and_then(|id| runs.iter().copied().find(|run| run.id == id))
                .or_else(|| runs.last().copied());
            if runs.len() > 1 {
                let mut tabs = row().flex_wrap().gap_1();
                for run in &runs {
                    let run_id = run.id;
                    tabs = tabs.child(select_button(
                        format!("automation-run-tab-{run_id}"),
                        format!("Run #{run_id} · {}", run.status),
                        selected.is_some_and(|selected| selected.id == run_id),
                        cx,
                    ).on_click(cx.listener(move |app, _, _, cx| {
                        app.automation_ui.technical_run = Some(run_id);
                        app.automation_ui.technical_scroll = ScrollHandle::new();
                        cx.notify();
                    })));
                }
                technical = technical.child(tabs);
            }
            if let Some(run) = selected {
                let logs = run
                    .logs
                    .iter()
                    .rev()
                    .take(40)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                let inner = self.automation_ui.technical_scroll.clone();
                let outer = self.automation_ui.dialog_scroll.clone();
                // Capture before either scroll container handles the wheel.
                let scroll_guard = canvas(|_, _, _| (), move |bounds, _, window, _| {
                    let visible = bounds.intersect(&window.content_mask().bounds);
                    let inner = inner.clone();
                    let outer = outer.clone();
                    window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                        if phase != DispatchPhase::Capture || !visible.contains(&event.position) { return; }
                        let delta = event.delta.pixel_delta(window.line_height());
                        if delta.y == px(0.) || delta.x.abs() > delta.y.abs() { return; }
                        let reveal = if delta.y < px(0.) && bounds.bottom() > visible.bottom() {
                            delta.y.max(visible.bottom() - bounds.bottom())
                        } else if delta.y > px(0.) && bounds.top() < visible.top() {
                            delta.y.min(visible.top() - bounds.top())
                        } else { px(0.) };
                        let outer_next = (outer.offset().y + reveal).clamp(-outer.max_offset().y, px(0.));
                        if reveal != px(0.) && outer_next != outer.offset().y {
                            outer.set_offset(point(outer.offset().x, outer_next));
                        } else {
                            let offset = inner.offset();
                            let next = (offset.y + delta.y).clamp(-inner.max_offset().y, px(0.));
                            if next != offset.y {
                                inner.set_offset(point(offset.x, next));
                            } else {
                                outer.set_offset(point(outer.offset().x,
                                    (outer.offset().y + delta.y).clamp(-outer.max_offset().y, px(0.))));
                            }
                        }
                        window.refresh();
                        cx.stop_propagation();
                    });
                }).absolute().inset_0();
                technical = technical.child(
                    col()
                        .p_3()
                        .gap_1()
                        .rounded(px(crate::appearance::GROUP_RADIUS))
                        .border_1()
                        .border_color(rgb(colors.line))
                        .child(label(
                            format!(
                                "Run #{} · {} · {} · {} attempt(s)",
                                run.id,
                                run.status,
                                run.model,
                                run.auto_retry_attempt + 1
                            ),
                            cx,
                        ))
                        .child(label(
                            format!(
                                "Session: {}",
                                run.session_id
                                    .clone()
                                    .unwrap_or_else(|| "not assigned".into())
                            ),
                            cx,
                        ))
                        .child(
                            div().relative().w_full().child(scroll_guard).child(div()
                                .id(format!("automation-run-logs-{}", run.id))
                                .w_full()
                                .min_h_0()
                                .max_h(reader_height)
                                .overflow_y_scroll()
                                .track_scroll(&self.automation_ui.technical_scroll)
                                .restrict_scroll_to_axis()
                                .child(label(logs, cx)).map(observed)),
                        ),
                );
            }
            panel = panel.child(technical);
        }
        panel.map(observed).into_any_element()
    }

    fn automation_result_sources(
        &self,
        occurrence_id: u64,
        key: &str,
        sources: &[crate::research::Source],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if sources.is_empty() {
            return div().into_any_element();
        }
        let disclosure_id = format!("automation-sources-{occurrence_id}-{key}");
        let expanded = self.automation_ui.expanded_sources.contains(&disclosure_id);
        let toggle_id = disclosure_id.clone();
        let mut section = col().gap_1().child(
            quiet_button(disclosure_id, format!("Sources · {}", sources.len()))
                .px_0().self_start()
                .icon(if expanded { assets::IconName::ChevronDown } else { assets::IconName::ChevronRight })
                .on_click(cx.listener(move |app, _, _, cx| {
                    if !app.automation_ui.expanded_sources.remove(&toggle_id) {
                        app.automation_ui.expanded_sources.insert(toggle_id.clone());
                    }
                    cx.notify();
                })),
        );
        if !expanded { return section.into_any_element(); }
        let mut list = col().pl_6().gap_1();
        for (index, source) in sources.iter().enumerate() {
            let url = source.url.clone();
            let source_id = format!("automation-source-{occurrence_id}-{key}-{index}");
            list = list.child(
                col()
                    .gap_1()
                    .child(
                        link_button(source_id, source.title.clone())
                            .icon(assets::IconName::ExternalLink)
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                    )
                    .child(muted(
                        format!("{} · published {}", source.url, source.published_at),
                        cx,
                    )),
            );
        }
        section = section.child(list);
        section.into_any_element()
    }

    /// The period's split research work as a readable list: status, kind and
    /// identifier on the left, full step title, and one Open action. Resume
    /// sits above the list, mirroring the ordinary goal progress page.
    fn automation_occurrence_steps(
        &self,
        occurrence: &AutomationOccurrence,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = palette(cx);
        let goals: Vec<u64> = if occurrence.research_goal_ids.is_empty() {
            occurrence.research_goal_id.into_iter().collect()
        } else {
            occurrence.research_goal_ids.clone()
        };
        let mut task_ids: Vec<u64> = goals
            .iter()
            .flat_map(|goal| self.store.research_children(*goal))
            .collect();
        if task_ids.is_empty() {
            return div().into_any_element();
        }
        task_ids.sort_by(|a, b| match (self.store.task(*a), self.store.task(*b)) {
            (Some(a), Some(b)) => crate::display_order::siblings_in_display_order(a, b),
            _ => a.cmp(b),
        });
        let total = task_ids.len();
        let completed = task_ids
            .iter()
            .filter(|task| self.store.research_result_available_or_pending(**task))
            .count();
        let busy = occurrence.status.is_active() || goals.iter().any(|goal| self.goal_busy(*goal));
        let retry_blocker = self.store.occurrence_retry_blocker(occurrence);
        let runnable = retry_blocker.is_none();
        let resumable = occurrence.status.is_active()
            || matches!(
                occurrence.status,
                OccurrenceStatus::Failed
                    | OccurrenceStatus::Cancelled
                    | OccurrenceStatus::Interrupted
            );
        let occurrence_id = occurrence.id;
        let heading = if completed == total {
            format!("Research complete · {completed}/{total} steps")
        } else {
            format!("Research steps · {completed}/{total}")
        };
        let expanded = completed < total || self.automation_ui.show_completed_steps;
        let mut list = col().gap_0p5().when(completed == total, |d| d.pl_6());
        for task_id in task_ids {
            let Some(task) = self.store.task(task_id) else {
                continue;
            };
            let status = self.display_task_status(task_id);
            let execution = status.execution;
            let kind = match task.research.as_ref().map(|research| research.kind) {
                Some(crate::research::ResearchKind::Collect) => "Evidence",
                Some(crate::research::ResearchKind::Analyze) => "Analysis",
                Some(crate::research::ResearchKind::Advise) => "Final report",
                _ => "Step",
            };
            let identifier = self.store.identifier(task);
            let title = task.title.clone();
            list = list.child(
                row()
                    .id(format!("automation-step-row-{task_id}"))
                    .px_2()
                    .py_1()
                    .gap_2()
                    .items_center()
                    .rounded(px(crate::appearance::GROUP_RADIUS))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(colors.hover)))
                    .tooltip({
                        let title = title.clone();
                        move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(title.clone())
                                .build(window, cx)
                        }
                    })
                    .on_click(
                        cx.listener(move |app, _, window, cx| {
                            app.request_task(task_id, false, window, cx)
                        }),
                    )
                    .child(execution.icon_view(format!("automation-step-icon-{task_id}"), cx))
                    .child(
                        div()
                            .id(format!("automation-step-status-{task_id}"))
                            .aria_label(status.label())
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(rgb(execution.color(cx)))
                            .child(selectable(status.label()))
                            .map(observed),
                    )
                    .child(label(format!("{kind} · {identifier}"), cx).flex_shrink_0())
                    .child(
                        div()
                            .id(format!("automation-step-title-{task_id}"))
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .aria_label(title.clone())
                            .tooltip({
                                let title = title.clone();
                                move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(title.clone())
                                        .build(window, cx)
                                }
                            })
                            .child(selectable(title.clone())),
                    )
                    .child(
                        quiet_button(format!("automation-step-{task_id}"), "Open")
                            .icon(assets::IconName::ChevronRight)
                            .accessibility_label(format!("Open {identifier} · {title}"))
                            .tooltip(format!("Open {identifier} · {title}"))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                cx.stop_propagation();
                                app.request_task(task_id, false, window, cx)
                            })),
                    ),
            );
        }
        col()
            .when(completed < total, |d| d.pl_6())
            .gap_2()
            .child(
                row()
                    .justify_between()
                    .flex_wrap()
                    .gap_2()
                    .child(if completed == total {
                        quiet_button("automation-completed-steps", heading)
                            .px_0()
                            .icon(if expanded { assets::IconName::ChevronDown } else { assets::IconName::ChevronRight })
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.automation_ui.show_completed_steps = !app.automation_ui.show_completed_steps;
                                cx.notify();
                            })).into_any_element()
                    } else {
                        div().font_weight(FontWeight::SEMIBOLD)
                            .child(selectable(heading)).into_any_element()
                    })
                    .when(completed < total && resumable, |d| {
                        // An ended period can still have a background session
                        // shutting down. That is not "running"; say "Stopping…"
                        // so the control cannot be read as live period work.
                        let winding_down = occurrence.status.is_terminal() && busy;
                        let retry_reason = retry_blocker.clone().unwrap_or_default();
                        d.child(
                            primary_button(
                                "resume-automation-steps",
                                if winding_down {
                                    "Stopping…"
                                } else if busy {
                                    "Running…"
                                } else if occurrence.status.is_active() {
                                    "Continue period"
                                } else {
                                    "Retry period"
                                },
                            )
                            .icon(assets::IconName::Play)
                            .disabled(busy || !runnable)
                            .tooltip(if !runnable {
                                retry_reason
                            } else if winding_down {
                                "The last step's background session is still shutting down. Retry becomes available when it finishes; it cannot extend the period.".to_string()
                            } else if busy {
                                "Steps are already running or queued.".to_string()
                            } else if occurrence.status.is_active() {
                                "Continue this period's unfinished steps using its original approved version. The schedule and definition are unchanged.".to_string()
                            } else {
                                format!(
                                    "Retry this period in place using approved version v{}. Finished steps keep their results; only unfinished work runs again.",
                                    occurrence.definition_version
                                )
                            })
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.retry_automation_occurrence(occurrence_id, cx)
                            })),
                        )
                    }),
            )
            .when(expanded, |d| d.child(list))
            .into_any_element()
    }

    /// A period's report, sources and technical detail open in one modal so the
    /// history stays a compact list.
    pub(crate) fn automation_result_dialog(
        &self,
        presentation: &crate::result_presentation::ResultPresentation,
        reader_height: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(id) = self.period_overlay() else {
            return div().into_any_element();
        };
        let Some(occurrence) = self.store.occurrence(id) else {
            return div().into_any_element();
        };
        let status_label = match presentation.status_suffix() {
            Some(suffix) => format!("{} · {suffix}", occurrence.status.label()),
            None => occurrence.status.label().to_string(),
        };
        let activity = occurrence_activity(occurrence);
        let activity_text = format!(
            "{} {}",
            activity.kind.label(),
            format_in_zone_precise(activity.utc, &occurrence.timezone)
        );
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x00000080))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                col()
                    .w(px(860.))
                    .max_w_full()
                    .max_h(relative(0.9))
                    .p_6()
                    .gap_4()
                    .map(|d| crate::appearance::floating_surface(d, cx))
                    .child(
                        row()
                            .justify_between()
                            .items_start()
                            .gap_4()
                            .child(
                                col()
                                    .gap_1()
                                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(
                                        selectable(format!(
                                            "Period {} · {}",
                                            occurrence.id, status_label
                                        )),
                                    ))
                                    .child(muted(
                                        format!("{activity_text} · {}", occurrence.source.label()),
                                        cx,
                                    )),
                            )
                            .child(
                                row()
                                    .gap_2()
                                    .items_center()
                                    .when(occurrence.status.is_active(), |d| {
                                        d.child(
                                            danger_button(
                                                "cancel-automation-result",
                                                "Cancel period",
                                            )
                                            .icon(assets::IconName::CircleX)
                                            .tooltip("Cancel this period and its scheduled retry")
                                            .on_click(
                                                cx.listener(move |app, _, _, cx| {
                                                    app.cancel_automation_occurrence_ui(id, cx)
                                                }),
                                            ),
                                        )
                                    })
                                    .child(
                                        quiet_button(
                                            "mark-automation-read",
                                            if occurrence.unread {
                                                "Mark read"
                                            } else {
                                                "Mark unread"
                                            },
                                        )
                                        .icon(assets::IconName::Check)
                                        .on_click(
                                            cx.listener(move |app, _, _, cx| {
                                                app.toggle_automation_read(id, cx)
                                            }),
                                        ),
                                    )
                                    .child(
                                        tool_icon(
                                            "close-automation-result",
                                            assets::IconName::Close,
                                            "Close result",
                                        )
                                        .on_click(
                                            cx.listener(|app, _, window, cx| {
                                                app.close_occurrence_result(window, cx)
                                            }),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("automation-result-body")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.automation_ui.dialog_scroll)
                            // A native wheel can update the offset without waking
                            // a newly installed cached overlay. Request a paint,
                            // not a semantic app update: keep report/status caches.
                            .on_scroll(|_, window, _| window.refresh())
                            .child(self.automation_occurrence_detail(id, presentation, reader_height, cx))
                            .map(observed),
                    )
                    .child(row().justify_end().child(
                        button("close-automation-result-footer", "Close")
                            .track_focus(&self.dialog_first_focus)
                            .on_click(cx.listener(
                                |app, _, window, cx| app.close_occurrence_result(window, cx),
                            )),
                    )),
            )
            .track_focus(&self.modal_focus)
            .on_key_down(cx.listener(|app, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    app.close_occurrence_result(window, cx);
                    cx.stop_propagation();
                }
            }))
            .focus_trap("automation-result-focus", &self.modal_focus)
            .into_any_element()
    }

    /// Read-only planning history. Raw planner output is labeled as a plan
    /// attempt, never as a validated research report.
    pub(crate) fn planning_attempt_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(run_id) = self.planning_attempt_overlay() else {
            return div().into_any_element();
        };
        let Some(run) = self.store.runs().iter().find(|run| run.id == run_id) else {
            return div()
                .id("planning-attempt-dialog")
                .absolute()
                .inset_0()
                .occlude()
                .bg(rgba(0x00000080))
                .flex()
                .items_center()
                .justify_center()
                .child(muted("This planning attempt is no longer available.", cx))
                .map(observed);
        };
        let request = self.store.request_for_run(run_id);
        let owner = run.planning_owner.as_ref();
        let objective = request
            .map(|request| request.input.as_str())
            .unwrap_or("Planning request unavailable");
        let expected = request
            .and_then(|request| request.expected_result.as_deref())
            .unwrap_or("Not captured for this historical request");
        let revision = owner
            .map(|owner| owner.revision)
            .or_else(|| request.map(|request| request.revision));
        let output = if run.output.trim().is_empty() {
            "No planner output was saved for this attempt.".to_string()
        } else {
            let mut text: String = run.output.chars().take(20_000).collect();
            if run.output.chars().count() > 20_000 {
                text.push_str("\n\n[Output truncated for display]");
            }
            text
        };
        let objective_id = format!("planning-attempt-objective-{run_id}");
        let expected_id = format!("planning-attempt-expected-{run_id}");
        let status_id = format!(
            "planning-attempt-status-{run_id}-{status}",
            status = run.status.replace(' ', "-")
        );
        let revision_id = format!(
            "planning-attempt-revision-{run_id}-{}",
            revision
                .map(|value| value.to_string())
                .unwrap_or_else(|| "legacy".into())
        );
        div().id("planning-attempt-dialog").absolute().inset_0().occlude().bg(rgba(0x00000080)).flex().items_center().justify_center().p_4()
            .child(
                col().w(px(860.)).max_w_full().max_h(relative(0.9)).p_6().gap_4().map(|d| crate::appearance::floating_surface(d, cx))
                    .child(
                        row().justify_between().items_start().gap_4()
                            .child(col().gap_1()
                                .child(div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .id(status_id)
                                    .aria_label(run.status.clone())
                                    .child(selectable(format!(
                                        "Planning attempt {} · {}",
                                        run.id, run.status
                                    )))
                                    .map(observed))
                                .child(muted(format!(
                                    "Revision {} · {} · plan history, not a research report",
                                    revision.map(|value| value.to_string()).unwrap_or_else(|| "legacy".into()),
                                    run.ended_at.clone().or(Some(run.started_at.clone())).unwrap_or_default()
                                ), cx)
                                    .id(revision_id)
                                    .aria_label(format!(
                                        "Revision {}",
                                        revision.map(|value| value.to_string()).unwrap_or_else(|| "legacy".into())
                                    ))
                                    .map(observed)))
                            .child(
                                tool_icon("close-planning-attempt", assets::IconName::Close, "Close planning attempt")
                                    .on_click(cx.listener(|app, _, window, cx| app.close_planning_attempt(window, cx))),
                            ),
                    )
                    .child(
                        div().id("planning-attempt-body").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().gap_3()
                            .child(overview_heading("Objective"))
                            .child(
                                div()
                                    .id(objective_id)
                                    .aria_label(objective.to_string())
                                    .child(selectable(objective.to_string()))
                                    .map(observed),
                            )
                            .child(overview_heading("Expected result"))
                            .child(
                                div()
                                    .id(expected_id)
                                    .aria_label(expected.to_string())
                                    .child(selectable(expected.to_string()))
                                    .map(observed),
                            )
                            .child(overview_heading("Saved planner output"))
                            .child(selectable(output)),
                    )
                    .child(
                        row().justify_end()
                            .child(
                                button("close-planning-attempt-footer", "Close")
                                    .on_click(cx.listener(|app, _, window, cx| app.close_planning_attempt(window, cx))),
                            ),
                    ),
            )
            .focus_trap("planning-attempt-focus", &self.modal_focus)
            .map(observed)
    }

    fn automation_wizard(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let step = self.automation_ui.step;
        let mut content = col()
            .id("automation-wizard-scroll")
            .track_scroll(&self.automation_ui.scroll)
            .flex_1()
            .w_full()
            .h_full()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .p_6()
            .gap_4();
        content =
            content.child(
                row()
                    .w_full()
                    .justify_between()
                    .child(
                        col()
                            .gap_1()
                            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(
                                selectable(if self.automation_ui.editing.is_some() {
                                    "Edit automation"
                                } else {
                                    "New automation"
                                }),
                            ))
                            .child(muted(
                                format!(
                                    "Step {step} of 3 · Creating or saving never starts research."
                                ),
                                cx,
                            )),
                    )
                    .child(quiet_button("cancel-automation-wizard", "Cancel").on_click(
                        cx.listener(|app, _, window, cx| {
                            app.request_cancel_wizard(window, cx);
                        }),
                    )),
            );
        content = content.child(self.automation_feedback(cx));

        if step == 1 {
            content = content.child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(selectable("Automation type")),
            );
            content = content.child(
                row().flex_wrap().gap_2()
                    .child(select_button("automation-shape-quick", "Quick task", self.automation_ui.shape == AutomationShape::Quick, cx)
                        .tooltip("One instruction, one short answer each period. No project or planning needed.")
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.select_automation_shape(AutomationShape::Quick, cx);
                        })))
                    .child(select_button("automation-shape-research", "Research automation", self.automation_ui.shape == AutomationShape::Research, cx)
                        .tooltip("Describe it once; review the generated goals, then run the same approved plan each period.")
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.select_automation_shape(AutomationShape::Research, cx);
                        }))),
            );
            content = content.child(muted(if self.automation_ui.shape.is_quick() {
                "Quick tasks live under Automations and keep one short answer per period. They never create a project or topic."
            } else {
                "Research automations plan once, when you create them. Each period runs that approved plan inside its own topic under Automations."
            }, cx));
            content = content.child(self.automation_field(
                "Name (optional)",
                "automation-name",
                &self.automation_ui.name,
                cx,
            ));
            content = content.child(self.automation_area(
                if self.automation_ui.shape.is_quick() {
                    "What should be checked each period?"
                } else {
                    "Describe the research once"
                },
                "automation-objective",
                &self.automation_ui.objective,
                cx,
            ));
            if self.automation_ui.shape.is_quick() {
                content = content.child(self.automation_area(
                    "Expected answer (optional)",
                    "automation-expected",
                    &self.automation_ui.expected,
                    cx,
                ));
                content = content.child(muted(
                    "Example: Check today's fuel prices in Shenzhen, Guangdong and answer with the current price and source. Left empty, the name and expected answer are derived from this instruction.",
                    cx,
                ));
            } else {
                content = content.child(self.automation_area(
                    "Expected result (optional)",
                    "automation-expected",
                    &self.automation_ui.expected,
                    cx,
                ));
                content = content.child(muted(
                    "Captured with the research description as one immutable planning revision. Changing either field requires Replan and a new review.",
                    cx,
                ));
            }
            content = content.child(muted(
                "Use automations for information checks and local reports. The selected agent keeps its normal permissions; review results before acting.",
                cx,
            ));
        } else if step == 2 {
            content = content.child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(selectable("Schedule")),
            );
            let mut kinds = row().flex_wrap().gap_2();
            for kind in AutomationKind::ALL {
                kinds = kinds.child(
                    select_button(
                        format!("automation-kind-{:?}", kind),
                        kind.label(),
                        self.automation_ui.kind == kind,
                        cx,
                    )
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.automation_ui.kind = kind;
                        cx.notify();
                    })),
                );
            }
            content = content.child(kinds);
            if self.automation_ui.kind == AutomationKind::Interval {
                content = content
                    .child(
                        row()
                            .gap_2()
                            .child(
                                select_button(
                                    "automation-unit-minutes",
                                    "Minutes",
                                    !self.automation_ui.interval_in_hours,
                                    cx,
                                )
                                .on_click(cx.listener(
                                    |app, _, _, cx| {
                                        app.automation_ui.interval_in_hours = false;
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                select_button(
                                    "automation-unit-hours",
                                    "Hours",
                                    self.automation_ui.interval_in_hours,
                                    cx,
                                )
                                .on_click(cx.listener(
                                    |app, _, _, cx| {
                                        app.automation_ui.interval_in_hours = true;
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                    .child(self.automation_field(
                        if self.automation_ui.interval_in_hours {
                            "Hours between runs"
                        } else {
                            "Minutes between runs"
                        },
                        "automation-interval",
                        &self.automation_ui.interval,
                        cx,
                    ));
            } else {
                content = content.child(
                    row()
                        .gap_3()
                        .child(self.automation_field(
                            "Hour (00–23)",
                            "automation-hour",
                            &self.automation_ui.hour,
                            cx,
                        ))
                        .child(self.automation_field(
                            "Minute (00–59)",
                            "automation-minute",
                            &self.automation_ui.minute,
                            cx,
                        )),
                );
                if self.automation_ui.kind == AutomationKind::Weekly {
                    let mut days = row().flex_wrap().gap_2();
                    for (index, name) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                        .iter()
                        .enumerate()
                    {
                        let selected = self.automation_ui.weekdays[index];
                        days = days.child(
                            select_button(format!("automation-day-{index}"), *name, selected, cx)
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.automation_ui.weekdays[index] =
                                        !app.automation_ui.weekdays[index];
                                    cx.notify();
                                })),
                        );
                    }
                    content = content.child(days);
                }
            }
            content = content
                .child(self.automation_field("IANA time zone", "automation-timezone", &self.automation_ui.timezone, cx))
                .child(muted("Use a city time zone such as Asia/Shanghai. Fixed UTC offsets are rejected so daylight-saving changes are handled correctly.", cx));
            let now = Self::automation_now();
            match self
                .automation_ui
                .schedule(cx)
                .and_then(|schedule| schedule.preview(now, now, 3))
            {
                Ok(preview) if !preview.is_empty() => {
                    let timezone = self
                        .automation_ui
                        .timezone
                        .read(cx)
                        .value()
                        .trim()
                        .to_string();
                    content = content.child(
                        col()
                            .p_3()
                            .gap_1()
                            .rounded(px(crate::appearance::GROUP_RADIUS))
                            .bg(rgb(colors.panel))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(selectable("Next three triggers")),
                            )
                            .child(label(
                                preview
                                    .iter()
                                    .map(|value| format_in_zone(*value, &timezone))
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                                cx,
                            )),
                    );
                }
                Ok(_) => {}
                Err(error) => {
                    content = content.child(crate::views::notice(
                        crate::views::NoticeTone::Danger,
                        error,
                        cx,
                    ));
                }
            }
            content = content
                .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Missed periods")))
                .child(
                    row().flex_wrap().gap_2()
                        .child(select_button("automation-catchup-latest", "Only the latest missed period", self.automation_ui.catch_up == CatchUp::LatestOnly, cx)
                            .on_click(cx.listener(|app, _, _, cx| { app.automation_ui.catch_up = CatchUp::LatestOnly; cx.notify(); })))
                        .child(select_button("automation-catchup-skip", "Skip all missed periods", self.automation_ui.catch_up == CatchUp::SkipAll, cx)
                            .on_click(cx.listener(|app, _, _, cx| { app.automation_ui.catch_up = CatchUp::SkipAll; cx.notify(); }))),
                )
                .child(
                    row().justify_between()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Advanced limits")))
                        .child(quiet_button("automation-toggle-advanced",
                            if self.automation_ui.show_advanced { "Hide" } else { "Show" })
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.automation_ui.show_advanced = !app.automation_ui.show_advanced;
                                cx.notify();
                            }))),
                )
                .when(self.automation_ui.show_advanced, |d| d.child(
                    row().flex_wrap().gap_3()
                        .child(self.automation_field("Allowed delay (minutes)", "automation-delay", &self.automation_ui.delay, cx))
                        .child(self.automation_field("Time limit (minutes)", "automation-timeout", &self.automation_ui.timeout, cx))
                        .child(self.automation_field("Pause after N failures", "automation-failures", &self.automation_ui.failures, cx))
                        .child(self.automation_field("Log retention (days)", "automation-retention", &self.automation_ui.retention, cx)),
                ))
                .child(crate::views::observed(
                    div()
                        .id("automation-schedule-note")
                        .aria_label("Times use the schedule and time zone above.")
                        .child(muted("Times use the schedule and time zone above.", cx)),
                ));
            if !self.automation_ui.preview_error.is_empty() {
                content = content.child(crate::views::notice(
                    crate::views::NoticeTone::Danger,
                    self.automation_ui.preview_error.clone(),
                    cx,
                ));
            }
        } else {
            let quick = self.automation_ui.shape.is_quick();
            let review_heading = if quick {
                "Review and authorization"
            } else {
                "Review generated plan"
            };
            content = content.child(crate::views::observed(
                div()
                    .id("automation-review-heading")
                    .aria_label(review_heading)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(selectable(review_heading)),
            ));
            if quick {
                let objective = self
                    .automation_ui
                    .objective
                    .read(cx)
                    .value()
                    .trim()
                    .to_string();
                let expected = self
                    .automation_ui
                    .expected
                    .read(cx)
                    .value()
                    .trim()
                    .to_string();
                content = content.child(crate::views::observed(
                    col()
                        .id("automation-quick-authorization")
                        .aria_label(format!("{objective}\n{expected}"))
                        .p_3()
                        .gap_2()
                        .rounded(px(crate::appearance::GROUP_RADIUS))
                        .border_1()
                        .border_color(rgb(colors.line))
                        .bg(rgb(colors.panel))
                        .child(overview_heading("Instruction"))
                        .child(selectable(objective))
                        .when(!expected.is_empty(), |view| {
                            view.child(overview_heading("Expected answer"))
                                .child(selectable(expected))
                        }),
                ));
                if self.automation_ui.plan.is_none() {
                    content = content.child(crate::views::notice(
                        crate::views::NoticeTone::Danger,
                        "Return to the schedule and continue to review this task.",
                        cx,
                    ));
                }
            } else if let Some(template) = &self.automation_ui.plan {
                for (goal_index, goal) in template.goals.iter().enumerate() {
                    content = content.child(crate::views::observed(
                        col()
                            .id(format!("automation-plan-goal-{goal_index}"))
                            .p_3()
                            .gap_2()
                            .rounded(px(crate::appearance::GROUP_RADIUS))
                            .border_1()
                            .border_color(rgb(colors.line))
                            .bg(rgb(colors.panel))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable(
                                format!(
                                    "Goal {}/{} · {}",
                                    goal_index + 1,
                                    template.goals.len(),
                                    goal.title
                                ),
                            )))
                            .child(muted(goal.brief.clone(), cx))
                            .children(goal.steps.iter().enumerate().map(|(index, step)| {
                                col()
                                    .gap_1()
                                    .child(div().child(selectable(format!(
                                        "{}. {} · {}",
                                        index + 1,
                                        step.title,
                                        match step.kind {
                                            crate::research::ResearchKind::Collect =>
                                                "Evidence collection",
                                            crate::research::ResearchKind::Analyze => "Analysis",
                                            crate::research::ResearchKind::Advise =>
                                                "Final report",
                                            crate::research::ResearchKind::Goal => "Plan",
                                        }
                                    ))))
                                    .child(muted(step.instructions.clone(), cx))
                                    .child(muted(
                                        format!(
                                            "Depends on: {}",
                                            if step.depends_on.is_empty() {
                                                "none".into()
                                            } else {
                                                step.depends_on
                                                    .iter()
                                                    .map(|i| (i + 1).to_string())
                                                    .collect::<Vec<_>>()
                                                    .join(", ")
                                            }
                                        ),
                                        cx,
                                    ))
                                    .into_any_element()
                            })),
                    ));
                }
                content = content.child(muted(
                    "Each period repeats this plan unchanged after you enable it. Planning is never repeated unless you choose Replan.",
                    cx,
                ));
            } else if self.automation_ui.pending_planning.is_some() {
                content = content.child(crate::views::notice_panel(
                    crate::views::NoticeTone::Info,
                    "Saving the reviewed request…",
                    "The planner starts after this exact draft version is durable. No second click is needed.",
                    cx,
                ));
            } else if self.automation_ui.planning_automation.is_some() {
                content = content.child(crate::views::notice_panel(
                    crate::views::NoticeTone::Info,
                    "Planning your research plan…",
                    "The planner splits the request into goals and steps once. You will review it before anything is enabled. This can take a minute.",
                    cx,
                ));
            } else {
                content = content.child(crate::views::notice_panel(
                    crate::views::NoticeTone::Danger,
                    "No plan generated yet",
                    "Retry planning to generate one.",
                    cx,
                ));
                content = content.child(
                    primary_button("automation-retry-planning", "Retry planning")
                        .icon(assets::IconName::RefreshCw)
                        .on_click(cx.listener(|app, _, _, cx| {
                            match app.start_automation_planning(cx) {
                                Ok(AutomationPlanningStart::Started(automation)) => {
                                    app.automation_ui.planning_automation = Some(automation);
                                    app.automation_ui.feedback =
                                        Some(Ok("Planning started…".into()));
                                }
                                Ok(AutomationPlanningStart::WaitingForDraft) => {
                                    app.automation_ui.feedback = Some(Ok(
                                        "Saving… the planner starts after the draft is durable."
                                            .into(),
                                    ));
                                }
                                Err(error) => app.automation_ui.feedback = Some(Err(error)),
                            }
                            cx.notify();
                        })),
                );
            }
            content = content
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(selectable("Schedule")),
                )
                .child(muted(
                    format!(
                        "{} · {}",
                        self.automation_ui.kind.label(),
                        self.automation_ui.timezone.read(cx).value().trim()
                    ),
                    cx,
                ))
                .child(label(
                    self.automation_ui
                        .preview
                        .iter()
                        .map(|value| {
                            self.automation_ui
                                .schedule(cx)
                                .map(|schedule| format_in_zone(*value, &schedule.timezone))
                                .unwrap_or_else(|_| value.to_string())
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    cx,
                ));
            content = content.child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(selectable("Execution")),
            );
            content = content.child(muted(
                if quick {
                    "Enabling approves this instruction, agent, model, time zone and limits for repeated execution. Each period runs one step in its own folder under the Automations area, through the same research pipeline and agent as a manual run."
                } else {
                    "Enabling approves this plan: every goal and step, the agent, models, time zone and limits. Each period creates a new topic under Automations with its own folder, session and browser Space, and never replans."
                },
                cx,
            ));
            content = content.child(muted(
                "Missed periods run only the newest one by default; it is skipped if older than the allowed delay. Runs happen only while the app is open and the computer is awake.",
                cx,
            ));
        }

        let mut footer = row().w_full().justify_between().gap_3();
        footer = footer.child(when(
            step > 1,
            quiet_button("automation-back-step", "Back")
                .icon(assets::IconName::ArrowLeft)
                .on_click(cx.listener(|app, _, _, cx| app.automation_wizard_back(cx))),
            div().into_any_element(),
        ));
        let plan_ready = self.automation_ui.plan.is_some();
        let mut actions = row().gap_2();
        // Saving the reviewed plan as a draft stays available at every step, so a
        // plan can be kept without enabling it.
        actions = actions.child(
            button(
                "save-automation-draft",
                if step < 3 {
                    "Save draft"
                } else {
                    "Save draft & finish"
                },
            )
            .icon(assets::IconName::ClipboardCheck)
            .on_click(cx.listener(|app, _, _, cx| {
                app.save_automation_draft(cx);
            })),
        );
        if step < 3 {
            let quick = self.automation_ui.shape.is_quick();
            let planning_now = step == 2
                && self.automation_ui.shape == AutomationShape::Research
                && self.automation_ui.plan.is_none();
            actions = actions.child(
                primary_button(
                    "automation-next-step",
                    if planning_now { "Generate plan" } else { "Continue" },
                )
                    .icon(assets::IconName::ArrowRight)
                    .tooltip(if planning_now {
                        "Saves a draft and plans the research once. You review it before anything is enabled."
                    } else if quick {
                        if step == 1 {
                            "Continue to the schedule"
                        } else {
                            "Continue to review and authorize this task"
                        }
                    } else if step == 1 {
                        "Continue to the schedule"
                    } else {
                        "Continue to review the generated plan"
                    })
                    .on_click(cx.listener(|app, _, window, cx| app.automation_wizard_next(window, cx))),
            );
        } else {
            let quick = self.automation_ui.shape.is_quick();
            actions = actions.child(
                primary_button("enable-automation", "Enable automation")
                    .icon(assets::IconName::Play)
                    .disabled(!plan_ready)
                    .tooltip(if plan_ready {
                        if quick {
                            "Approve this instruction and start future periods"
                        } else {
                            "Approve this plan and start future periods"
                        }
                    } else if quick {
                        "Return to the schedule and continue to authorize this task"
                    } else {
                        "Review the generated plan before enabling"
                    })
                    .on_click(cx.listener(|app, _, _, cx| app.enable_automation(cx))),
            );
        }
        footer = footer.child(actions);
        col()
            .flex_1()
            .w_full()
            .h_full()
            .min_h_0()
            .min_w_0()
            .child(content.into_any_element())
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .px_6()
                    .pb_6()
                    .pt_3()
                    .border_t_1()
                    .border_color(rgb(colors.line))
                    .child(footer),
            )
            .into_any_element()
    }

    fn automation_field(
        &self,
        title: &str,
        id: &str,
        input: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        col()
            .gap_1()
            .w_full()
            .min_w_0()
            .child(label(title.to_string(), cx))
            .child(Input::new(input).id(id.to_string()).w_full())
            .into_any_element()
    }

    fn automation_area(
        &self,
        title: &str,
        id: &str,
        input: &Entity<TextareaState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        col()
            .gap_1()
            .w_full()
            .min_w_0()
            .child(label(title.to_string(), cx))
            .child(
                div()
                    .id(id.to_string())
                    .w_full()
                    .min_w_0()
                    .child(Textarea::new(input).w_full())
                    .map(observed),
            )
            .into_any_element()
    }
}

fn when(condition: bool, yes: impl IntoElement, no: impl IntoElement) -> AnyElement {
    if condition {
        yes.into_any_element()
    } else {
        no.into_any_element()
    }
}

struct ResearchKinds;
impl ResearchKinds {
    fn labels() -> [&'static str; 4] {
        ["Plan", "Collect", "Analyze", "Report"]
    }
}

#[cfg(test)]
mod tests {
    use super::{
        contrast_against, format_in_zone_precise, result_tint_color, ResultTints,
        DARK_RESULT_PANEL_RGB, LIGHT_RESULT_PANEL_RGB, RESULT_NAME_MIN_CONTRAST,
    };
    use gpui_kit::{
        div, init, list, point, px, size, AppContext as _, Context, Entity, IntoElement,
        ListAlignment, ListOffset, ListState, ParentElement as _, Render, ScrollDelta,
        ScrollEvent, Styled as _, TestAppContext, VisualTestContext, Window,
    };
    use gpui_kit::test::TestWindowExt;
    use crate::app::TaskApp;
    use crate::automation::{
        AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy, AutomationShape,
        OccurrenceStatus, TimeWindow, TriggerSource,
    };
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    use crate::domain::Store;

    #[test]
    fn result_scroll_bridge_keeps_list_offset_authoritative() {
        use super::{result_scroll_step, ResultScrollStep};

        // External restore, including a large item_ix=0 offset.
        assert_eq!(
            result_scroll_step(false, -50_000.0, 0.0, 0.0, None, 0),
            ResultScrollStep::Apply(-50_000.0)
        );
        // While restoring, a stale current offset must not be written back.
        assert_eq!(
            result_scroll_step(false, -50_000.0, 0.0, 0.0, Some(-50_000.0), 1),
            ResultScrollStep::Apply(-50_000.0)
        );
        // The list caught up: settle the handle to the list's offset.
        assert_eq!(
            result_scroll_step(false, -50_000.0, 0.0, -50_000.0, Some(-50_000.0), 2),
            ResultScrollStep::Settle {
                handle: -50_000.0,
                synced: -50_000.0
            }
        );
        // If the requested offset is beyond the list maximum, settle at the
        // actual current offset after bounded attempts.
        assert_eq!(
            result_scroll_step(false, -50_000.0, 0.0, -3_000.0, Some(-50_000.0), 6),
            ResultScrollStep::Settle {
                handle: -3_000.0,
                synced: -3_000.0
            }
        );
        // User scrolling writes the new list offset back; it never reapplies
        // the old handle value.
        assert_eq!(
            result_scroll_step(false, 0.0, 0.0, -120.0, None, 0),
            ResultScrollStep::WriteBack {
                handle: -120.0,
                synced: -120.0
            }
        );
        assert_eq!(
            result_scroll_step(false, -120.0, -120.0, -120.0, None, 0),
            ResultScrollStep::Idle
        );
    }

    #[test]
    fn result_list_key_tracks_middle_rows_not_only_endpoints() {
        use super::ResultListKey;
        let base = ResultListKey {
            filter: None,
            limit: 20,
            ids: vec![10, 9, 8, 7],
            items: 21,
            structure: 0,
        };
        let middle_replaced = ResultListKey {
            ids: vec![10, 9, 99, 7],
            ..ResultListKey {
                filter: None,
                limit: 20,
                ids: Vec::new(),
                items: 21,
                structure: 0,
            }
        };
        assert_ne!(base, middle_replaced);
        let footer_added = ResultListKey {
            items: 22,
            structure: 4,
            ..ResultListKey {
                filter: None,
                limit: 20,
                ids: vec![10, 9, 8, 7],
                items: 21,
                structure: 0,
            }
        };
        assert_ne!(base, footer_added);
        assert_eq!(
            base,
            ResultListKey {
                filter: None,
                limit: 20,
                ids: vec![10, 9, 8, 7],
                items: 21,
                structure: 0,
            }
        );
    }

    fn results_store() -> Store {
        let template = crate::automation_execution::build_template(
            "Capacity check",
            "Short report",
        )
        .unwrap();
        let mut seed = Store::empty();
        seed.project_mut(1).unwrap().directory = "/tmp".into();
        seed.set_default_model("qa/model".into());
        seed.research_config_mut().models = std::array::from_fn(|_| "qa/model".into());
        let automation = seed
            .create_automation_draft(
                1,
                AutomationShape::Research,
                "Capacity check".into(),
                "Capacity check".into(),
                "Short report".into(),
                Schedule {
                    spec: ScheduleSpec::Interval { minutes: 60 },
                    timezone: "UTC".into(),
                },
                AutomationPolicy::default(),
                Some(template.clone()),
                1,
            )
            .unwrap();
        let definition = seed.automation(automation).unwrap().clone();
        let mut occurrences = Vec::new();
        for index in 0..25u64 {
            occurrences.push(AutomationOccurrence {
                id: index + 1,
                automation_id: automation,
                definition_version: 1,
                source: TriggerSource::Scheduled,
                trigger_key: format!("qa-{index}"),
                scheduled_for: Some(index as i64),
                requested_at: None,
                actual_started_at: Some(index as i64 + 1),
                timezone: "UTC".into(),
                reference_time: index as i64,
                data_window: TimeWindow::unresolved("qa"),
                snapshot: AutomationOccurrenceSnapshot {
                    objective: "Capacity check".into(),
                    expected_result: "Short report".into(),
                    shape: Some(AutomationShape::Research),
                    template: template.clone(),
                    models: std::array::from_fn(|_| "qa/model".into()),
                    agents: Default::default(),
                    materials: Vec::new(),
                    policy: AutomationPolicy::default(),
                    approval_version: 1,
                    approval_signature: "qa".into(),
                },
                status: OccurrenceStatus::Succeeded,
                reason: None,
                research_goal_id: None,
                research_goal_ids: Vec::new(),
                research_topic_id: None,
                run_ids: Vec::new(),
                final_run_id: None,
                unread: false,
                skipped_count: 0,
                skipped_from: None,
                skipped_to: None,
                created_at: index as i64,
                deadline_at: None,
                retry_requested_at: None,
            });
        }
        Store::empty()
            .with_loaded_tracked(vec![], vec![definition], occurrences)
            .unwrap()
    }

    fn lazy_results_store() -> Store {
        let mut store = results_store();
        let run_id = 1;
        store.push_run(crate::domain::AgentRun {
            id: run_id,
            task_id: 1,
            task_title: "Capacity sample".into(),
            directory: "/tmp".into(),
            prompt: String::new(),
            started_at: String::new(),
            queued_at: String::new(),
            workspace_source: None,
            ended_at: None,
            session_id: None,
            exit_code: None,
            agent: "test".into(),
            model: "test".into(),
            stage: crate::domain::Stage::Done,
            status: "Succeeded".into(),
            logs: Vec::new(),
            output: String::new(),
            attachments: Vec::new(),
            research_kind: None,
            sources: Vec::new(),
            input_signature: String::new(),
            automation_occurrence_id: Some(25),
            planning_owner: None,
            deadline_at: None,
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
        });
        {
            let mut occurrence = store.occurrence_mut(25).unwrap();
            occurrence.run_ids = vec![run_id];
            occurrence.final_run_id = Some(run_id);
        }
        store.mark_lazy_body(run_id);
        store
    }

    struct ResultsHarness(Entity<TaskApp>);
    impl Render for ResultsHarness {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(self.0.update(cx, |app, cx| app.automation_results(cx)))
        }
    }

    #[gpui_kit::test]
    fn lazy_result_row_keeps_saved_report_qualifier(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|_, cx| init(cx));
        let store = lazy_results_store();
        let (_harness, cx) = cx.add_window_view(|window, cx| {
            let app = cx.new(|cx| {
                let mut app = TaskApp::new_mock(window, cx);
                app.store = store;
                app.automation_ui.history_limit = 20;
                app
            });
            ResultsHarness(app)
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
        let snapshot = cx
            .update(|window, _| window.try_find("occurrence-status-25"))
            .expect("lazy report row must render");
        assert_eq!(
            snapshot.label(),
            Some("Succeeded · saved report"),
            "a saved but unhydrated body must keep an evidence-availability qualifier"
        );
    }

    #[gpui_kit::test]
    fn results_page_renders_header_rows_and_load_more_after_scroll(
        cx: &mut TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        cx.update(|_, cx| init(cx));
        let store = results_store();
        let (harness, cx) = cx.add_window_view(|window, cx| {
            let app = cx.new(|cx| {
                let mut app = TaskApp::new_mock(window, cx);
                app.store = store;
                app.automation_ui.history_limit = 20;
                app
            });
            ResultsHarness(app)
        });
        let app = cx.update(|_, cx| harness.read(cx).0.clone());
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
        assert!(cx
            .update(|window, _| window.try_find("automation-results-title"))
            .is_some(), "results title must render");
        assert!(cx
            .update(|window, _| window.try_find("automation-occurrence-25"))
            .is_some(), "first result row must render");
        let first_row = cx
            .update(|window, _| window.try_find("automation-occurrence-25").unwrap().bounds());
        assert!(cx
            .update(|window, _| window.try_find("load-more-automation-results"))
            .is_none(), "Load more starts below the viewport");
        let body = cx
            .update(|window, _| window.try_find("automation-results-scroll").unwrap().bounds());
        let wheel = point(body.center().x, body.center().y);
        let offset = |cx: &mut VisualTestContext| {
            cx.update(|_, cx| {
                app.read(cx)
                    .automation_ui
                    .result_list
                    .scroll_px_offset_for_scrollbar()
                    .y
                    .as_f32()
            })
        };
        let before = offset(cx);
        cx.simulate_event(ScrollEvent {
            position: wheel,
            delta: ScrollDelta::Pixels(point(px(0.), px(-300.))),
            ..Default::default()
        });
        cx.update(|window, cx| window.render_frame(cx));
        let after = offset(cx);
        assert!(
            after <= before - 200.0,
            "one wheel event must apply its pixel delta, before={before} after={after}"
        );
        for _ in 0..7 {
            cx.simulate_event(ScrollEvent {
                position: wheel,
                delta: ScrollDelta::Pixels(point(px(0.), px(-300.))),
                ..Default::default()
            });
        }
        cx.update(|window, cx| window.render_frame(cx));
        let moved = cx.update(|window, _| {
            window
                .try_find("automation-occurrence-25")
                .map(|snapshot| snapshot.bounds().top() < first_row.top())
                .unwrap_or(true)
        });
        assert!(moved, "wheel must displace or unmount the top rows");
        assert!(cx
            .update(|window, _| window.try_find("load-more-automation-results"))
            .is_some(), "Load more must render after scrolling");

        let list_top = |cx: &mut VisualTestContext| {
            cx.update(|window, _| window.try_find("automation-results-scroll").unwrap().bounds().top().as_f32())
        };
        let top_visible = |cx: &mut VisualTestContext| {
            let top = list_top(cx);
            let mut best: Option<(u64, f32)> = None;
            for id in (1..=25u64).rev() {
                if let Some(snapshot) =
                    cx.update(|window, _| window.try_find(format!("automation-occurrence-{id}")))
                {
                    let candidate = snapshot.bounds().top().as_f32();
                    if candidate >= top - 1.0
                        && best.is_none_or(|(_, current)| candidate < current)
                    {
                        best = Some((id, candidate));
                    }
                }
            }
            best
        };
        let anchored = top_visible(cx).expect("a row must be visible before Load more");
        cx.update(|window, cx| window.click("load-more-automation-results", cx));
        cx.update(|window, cx| window.render_frame(cx));
        let after_append = top_visible(cx).expect("a row must remain visible after Load more");
        assert_eq!(anchored.0, after_append.0, "Load more must keep the same top row");
        assert!(
            (anchored.1 - after_append.1).abs() <= 2.0,
            "Load more must keep the top row anchored: {} -> {}",
            anchored.1,
            after_append.1
        );
    }

    #[gpui_kit::test]
    fn result_list_restores_large_offset_and_resets(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let state = ListState::new(0, ListAlignment::Top, px(64.));
        state.reset_with_uniform_height(40, px(40.));
        struct TestView(ListState);
        impl Render for TestView {
            fn render(
                &mut self,
                _: &mut Window,
                _: &mut Context<Self>,
            ) -> impl IntoElement {
                list(self.0.clone(), |_, _, _| {
                    div().h(px(40.)).w_full().into_any_element()
                })
                .w_full()
                .h_full()
            }
        }
        let draw = |cx: &mut VisualTestContext| {
            let _ = cx.draw(
                point(px(0.), px(0.)),
                size(px(100.), px(200.)),
                |_, cx| cx.new(|_| TestView(state.clone())).into_any_element(),
            );
        };
        draw(cx);
        // Arbitrary pixel restore uses scroll_by; item_ix 0 with a large
        // offset_in_item is clamped upstream to the first item height.
        state.scroll_by(px(800.));
        draw(cx);
        let top = state.logical_scroll_top();
        let offset = state.scroll_px_offset_for_scrollbar().y;
        assert!(top.item_ix > 0, "800 px must displace the top item");
        assert!(
            offset <= px(-100.),
            "800 px scroll must leave a pixel offset, got {offset:?}"
        );

        state.reset(40);
        // Reset clears measured heights; the bridge retries after the first
        // post-reset layout, which this intermediate draw represents.
        draw(cx);
        state.scroll_by(px(400.));
        draw(cx);
        let offset = state.scroll_px_offset_for_scrollbar().y;
        assert!(
            offset <= px(-100.),
            "400 px scroll after reset must displace the list, got {offset:?}"
        );
    }

    fn incomplete_results_store() -> Store {
        const PLAN: &str = r#"{"tasks":[{"title":"Collect evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze findings","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Write report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#;
        let mut store = Store::mock();
        store.project_at_mut(0).unwrap().directory = "/tmp".into();
        store.research_config_mut().models =
            ["qa/model", "qa/model", "qa/model", "qa/model"].map(String::from);
        let goal = store
            .create_research_goal(
                "Incomplete evidence fixture".into(),
                "Exercise an intermediate evidence row.".into(),
                1,
            )
            .unwrap();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        // Build the Analyze run directly: the row must start as a saved-report
        // placeholder even though the prerequisite collect step has no result.
        let run_id = 1;
        let signature = store
            .research_signature_present(steps[1])
            .unwrap_or_default();
        // A cancelled task keeps the evidence out of the stale-input filter while
        // the missing final report still makes the period incomplete.
        store.task_mut(steps[1]).unwrap().stage = crate::domain::Stage::Cancelled;
        store.push_run(crate::domain::AgentRun {
            id: run_id,
            task_id: steps[1],
            task_title: "Analyze findings".into(),
            directory: "/tmp".into(),
            prompt: String::new(),
            started_at: String::new(),
            queued_at: String::new(),
            workspace_source: None,
            ended_at: None,
            session_id: None,
            exit_code: None,
            agent: "test".into(),
            model: "test".into(),
            stage: crate::domain::Stage::Done,
            status: "Succeeded".into(),
            logs: Vec::new(),
            output: String::new(),
            attachments: Vec::new(),
            research_kind: Some(crate::research::ResearchKind::Analyze),
            sources: Vec::new(),
            input_signature: signature,
            automation_occurrence_id: Some(1),
            planning_owner: None,
            deadline_at: None,
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
        });
        let template = crate::automation_execution::build_template(
            "Incomplete evidence fixture",
            "Short report",
        )
        .unwrap();
        store.push_occurrence(AutomationOccurrence {
            id: 1,
            automation_id: 99,
            definition_version: 1,
            source: TriggerSource::Manual,
            trigger_key: "manual:1".into(),
            scheduled_for: None,
            requested_at: Some(100),
            actual_started_at: Some(101),
            timezone: "UTC".into(),
            reference_time: 100,
            data_window: TimeWindow::unresolved("fixture"),
            snapshot: AutomationOccurrenceSnapshot {
                objective: "Incomplete evidence fixture".into(),
                expected_result: "Short report".into(),
                shape: Some(AutomationShape::Research),
                template,
                models: ["qa/model"; 4].map(String::from),
                agents: Default::default(),
                materials: Vec::new(),
                policy: AutomationPolicy::default(),
                approval_version: 1,
                approval_signature: "fixture".into(),
            },
            status: OccurrenceStatus::Succeeded,
            reason: None,
            research_goal_id: Some(goal),
            research_goal_ids: vec![goal],
            research_topic_id: None,
            run_ids: vec![run_id],
            final_run_id: Some(run_id),
            unread: false,
            skipped_count: 0,
            skipped_from: None,
            skipped_to: None,
            created_at: 99,
            deadline_at: None,
            retry_requested_at: None,
        });
        store.mark_lazy_body(run_id);
        store
    }

    #[gpui_kit::test]
    fn lazy_row_transitions_to_incomplete_after_hydration(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|_, cx| init(cx));
        let store = incomplete_results_store();
        let (harness, cx) = cx.add_window_view(|window, cx| {
            let app = cx.new(|cx| {
                let mut app = TaskApp::new_mock(window, cx);
                app.store = store;
                app.automation_ui.history_limit = 20;
                app
            });
            ResultsHarness(app)
        });
        let app = cx.update(|_, cx| harness.read(cx).0.clone());
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
        let lazy = cx
            .update(|window, _| window.try_find("occurrence-status-1"))
            .expect("lazy intermediate row renders");
        assert_eq!(lazy.label(), Some("Succeeded · saved report"));

        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                let body = "{\"status\":\"complete\",\"report\":\"Analyze findings with evidence.\",\"sources\":[{\"title\":\"Fixture source\",\"url\":\"https://example.com/fixture\",\"published_at\":\"unknown\"}]}";
                assert!(app.store.hydrate_run_body(1, body.to_string()));
                cx.notify();
            });
        });
        cx.update(|window, cx| window.render_frame(cx));
        let hydrated = cx
            .update(|window, _| window.try_find("occurrence-status-1"))
            .expect("hydrated intermediate row renders");
        assert_eq!(hydrated.label(), Some("Succeeded · incomplete"));
    }

    #[gpui_kit::test]
    fn compact_route_switch_wheel_moves_immediately(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|_, cx| init(cx));
        let store = results_store();
        let (app, cx) = cx.add_window_view(|window, cx| {
            let mut app = TaskApp::new_mock(window, cx);
            app.store = store;
            app.automation_ui.history_limit = 20;
            app
        });
        cx.simulate_resize(size(px(1000.), px(680.)));
        cx.update(|window, cx| window.render_frame(cx));
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.set_scope_identity(crate::app::Scope::All);
                cx.notify();
            });
        });
        cx.update(|window, cx| window.render_frame(cx));
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.set_scope_identity(crate::app::Scope::AutomationRuns);
                cx.notify();
            });
        });
        cx.update(|window, cx| window.render_frame(cx));
        let body = cx
            .update(|window, _| window.try_find("automation-results-scroll").unwrap().bounds());
        let before = cx.update(|_, cx| {
            app.read(cx)
                .automation_ui
                .result_list
                .scroll_px_offset_for_scrollbar()
                .y
                .as_f32()
        });
        // Wheel immediately after the route mount: no extra settling frame.
        cx.simulate_event(ScrollEvent {
            position: body.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-300.))),
            ..Default::default()
        });
        cx.update(|window, cx| window.render_frame(cx));
        let after = cx.update(|_, cx| {
            app.read(cx)
                .automation_ui
                .result_list
                .scroll_px_offset_for_scrollbar()
                .y
                .as_f32()
        });
        assert!(
            after <= before - 200.0,
            "compact route-mount wheel must apply its delta, before={before} after={after}"
        );
    }

    #[test]
    fn result_plan_preserves_empty_filter_rows_and_load_more() {
        use super::{AutomationResultsItem, AutomationResultsPlan};

        let empty = AutomationResultsPlan::new(vec![], 0, 0, 0, 0, None, 20);
        assert!(matches!(empty.items[0], AutomationResultsItem::Header));
        assert!(matches!(empty.items[1], AutomationResultsItem::EmptyAll));

        let filtered = AutomationResultsPlan::new(vec![], 0, 10, 0, 0, Some(1), 20);
        assert!(matches!(filtered.items[1], AutomationResultsItem::EmptyFiltered));

        let rows: Vec<(u64, usize)> = (0..20).map(|index| (index as u64 + 1, index)).collect();
        let first_page = AutomationResultsPlan::new(rows.clone(), 20, 20, 0, 0, None, 20);
        assert_eq!(first_page.items.len(), 21);
        assert!(first_page
            .items
            .iter()
            .all(|item| !matches!(item, AutomationResultsItem::LoadMore)));

        let more = AutomationResultsPlan::new(rows, 40, 40, 2, 1, None, 20);
        assert_eq!(more.items.len(), 22);
        assert!(matches!(more.items[21], AutomationResultsItem::LoadMore));
        assert_eq!(more.queued, 2);
        assert_eq!(more.waiting, 1);
    }

    #[test]
    fn occurrence_times_keep_second_precision() {
        // Two periods can start in the same minute; the list must tell them apart.
        assert_eq!(
            format_in_zone_precise(1_700_000_007, "Asia/Shanghai"),
            "2023-11-15 06:13:27"
        );
        assert_eq!(format_in_zone_precise(0, "UTC"), "1970-01-01 00:00:00");
    }

    #[test]
    fn automation_tints_stay_stable_and_distinct_within_a_session() {
        let tints = ResultTints::seeded(42);
        let first = tints.tint(1, false);
        assert_eq!(first.text, tints.tint(1, false).text);
        let mut colors = vec![first.text];
        for automation in 2..=10 {
            let tint = tints.tint(automation, false);
            assert!(
                !colors.contains(&tint.text),
                "golden-angle assignments must not repeat in the first ten automations"
            );
            colors.push(tint.text);
        }
        for color in colors {
            assert_eq!(color & 0xff, 0xff, "names use opaque text colours");
        }
        // Text stays readable in both themes: the same automation gets a darker
        // colour on light backgrounds and a lighter one on dark backgrounds.
        let light = tints.tint(1, false).text;
        let dark = tints.tint(1, true).text;
        assert_ne!(light, dark);
    }

    #[test]
    fn every_supported_hue_keeps_body_text_contrast() {
        for hue in 0..360 {
            for saturation in [0.62, 0.74, 0.86] {
                for preferred in [0.30, 0.38, 0.44] {
                    let light = result_tint_color(hue as f32, saturation, preferred, false);
                    let ratio = contrast_against(light.text, LIGHT_RESULT_PANEL_RGB);
                    assert!(
                        ratio >= RESULT_NAME_MIN_CONTRAST,
                        "light hue {hue} sat {saturation} L {preferred}: contrast {ratio:.2}"
                    );
                }
                for preferred in [0.64, 0.72, 0.78] {
                    let dark = result_tint_color(hue as f32, saturation, preferred, true);
                    let ratio = contrast_against(dark.text, DARK_RESULT_PANEL_RGB);
                    assert!(
                        ratio >= RESULT_NAME_MIN_CONTRAST,
                        "dark hue {hue} sat {saturation} L {preferred}: contrast {ratio:.2}"
                    );
                }
            }
        }
    }

    #[test]
    fn luminance_and_contrast_match_known_srgb_values() {
        // independent oracle values for opaque RGBA inputs and RGB backgrounds
        assert_eq!(super::relative_luminance(0x000000ff), 0.0);
        assert!((super::relative_luminance(0xffffffff) - 1.0).abs() < 1e-6);
        assert!((super::relative_luminance(0xff0000ff) - 0.2126).abs() < 1e-4);
        assert!((super::relative_luminance(0x00ff00ff) - 0.7152).abs() < 1e-4);
        assert!((super::relative_luminance(0x0000ffff) - 0.0722).abs() < 1e-4);
        assert!((super::relative_luminance(0x808080ff) - 0.2158605).abs() < 1e-4);
        // white text on black background is 21:1; equal colors are 1:1
        assert!((super::contrast_against(0xffffffff, 0x000000) - 21.0).abs() < 1e-4);
        assert!((super::contrast_against(0x000000ff, 0xffffff) - 21.0).abs() < 1e-4);
        assert!((super::contrast_against(0x123456ff, 0x123456) - 1.0).abs() < 1e-6);
        // the supported light panel must decode as white, not cyan
        assert_eq!(super::relative_luminance((LIGHT_RESULT_PANEL_RGB << 8) | 0xff), 1.0);
        assert!((super::relative_luminance((DARK_RESULT_PANEL_RGB << 8) | 0xff) - 0.0152).abs() < 0.002);
    }

    #[test]
    fn assigned_light_tints_stay_legible_distinct_and_opaque() {
        let tints = ResultTints::seeded(42);
        let mut assigned = Vec::new();
        for automation in 1..=30 {
            let light = tints.tint(automation, false);
            let ratio = contrast_against(light.text, LIGHT_RESULT_PANEL_RGB);
            assert!(ratio >= RESULT_NAME_MIN_CONTRAST, "assignment {automation}: {ratio:.2}");
            assert_eq!(light.text & 0xff, 0xff);
            assert_eq!(light.text, tints.tint(automation, false).text);
            assert!(
                !assigned.contains(&light.text),
                "first thirty assignments must stay distinguishable"
            );
            assigned.push(light.text);
            let dark = tints.tint(automation, true);
            assert!(contrast_against(dark.text, DARK_RESULT_PANEL_RGB) >= RESULT_NAME_MIN_CONTRAST);
        }
    }

    #[test]
    fn a_new_session_changes_the_starting_hue_but_keeps_rows_consistent() {
        let first = ResultTints::seeded(1);
        let second = ResultTints::seeded(2);
        let first_color = first.tint(9, false).text;
        let second_color = second.tint(9, false).text;
        assert_eq!(first_color, first.tint(9, false).text);
        assert_eq!(second_color, second.tint(9, false).text);
    }
}
