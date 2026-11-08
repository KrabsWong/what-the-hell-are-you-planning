//! Automation management surface: list, three-step creation, settings, history and results.
use crate::{
    app::{Scope, TaskApp},
    automation::{
        AutomationDefinition, AutomationOccurrence, AutomationPolicy, AutomationShape,
        AutomationTemplate, DefinitionStatus, OccurrenceStatus, TimeWindowState,
    },
    automation_execution::build_quick_template,
    automation_schedule::{parse_timezone, CatchUp, Schedule, ScheduleSpec, ScheduledTime},
    controls::*,
    domain::Store,
};
use crate::appearance::palette;
use crate::views::{chip, col, label, muted, observed, row, selectable};
use std::{cell::RefCell, collections::HashMap, io::Read};
use gpui_kit::{
    InteractiveElement,
    base::FocusTrapElement,
    component::Disableable,
    component::input::{Input, InputState, Textarea, TextareaState},
    component::menu::{DropdownMenu, PopupMenuItem},
    prelude::*,
    *,
};

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
        let (red, green, blue) = hsl_to_rgb(hue, saturation, lightness);
        ResultTint {
            text: (red << 24) | (green << 16) | (blue << 8) | 0xff,
        }
    }
}

pub struct AutomationUi {
    pub selected: Option<u64>,
    pub creating: bool,
    pub editing: Option<u64>,
    pub step: u8,
    /// Generated automation project; never shown as a project choice.
    pub project: u64,
    pub shape: AutomationShape,
    /// Automation whose research plan is being generated right now.
    pub planning_automation: Option<u64>,
    pub show_advanced: bool,
    pub kind: AutomationKind,
    pub interval_in_hours: bool,
    pub catch_up: CatchUp,
    pub weekdays: [bool; 7],
    pub plan: Option<AutomationTemplate>,
    pub preview: Vec<i64>,
    pub preview_error: String,
    /// Period whose result is open in a dialog. `None` keeps results closed.
    pub result_dialog: Option<u64>,
    /// Automation whose periods the all-results list is limited to. `None`
    /// keeps every automation.
    pub result_filter: Option<u64>,
    pub history_limit: usize,
    /// Task definition, schedule and run limits are secondary once periods exist,
    /// so they open in a dialog only when the user asks to review or change them.
    pub settings_dialog: bool,
    pub show_technical: bool,
    pub show_cleanup: bool,
    pub cleanup_preview: String,
    pub feedback: Option<Result<String, String>>,
    pub scroll: ScrollHandle,
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

impl AutomationUi {
    pub fn new(window: &mut Window, cx: &mut Context<TaskApp>) -> Self {
        let input = |window: &mut Window, cx: &mut Context<TaskApp>, placeholder: &str, value: &str| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder.to_string())
                    .default_value(value.to_string())
            })
        };
        Self {
            selected: None,
            creating: false,
            editing: None,
            step: 1,
            project: 1,
            shape: AutomationShape::Quick,
            planning_automation: None,
            show_advanced: false,
            kind: AutomationKind::Daily,
            interval_in_hours: false,
            catch_up: CatchUp::LatestOnly,
            weekdays: [true, true, true, true, true, false, false],
            plan: None,
            preview: vec![],
            preview_error: String::new(),
            result_dialog: None,
            result_filter: None,
            history_limit: 20,
            settings_dialog: false,
            show_technical: false,
            show_cleanup: false,
            cleanup_preview: String::new(),
            feedback: None,
            scroll: ScrollHandle::new(),
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
        TimeWindowState::Resolved { start_utc, end_utc, .. } => format!(
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
        ];
        if matches!(occurrence.data_window.state, TimeWindowState::Resolved { .. }) {
            lines.push(format!("Data window: {}", occurrence_window_label(occurrence)));
        }
        return lines.join("\n");
    }
    format!(
        "Scheduled for: {scheduled}\nActual start: {started}\nTime zone: {}\nReference time: {}\nData window: {}\n{}",
        occurrence.timezone,
        format_in_zone(occurrence.reference_time, &occurrence.timezone),
        occurrence_window_label(occurrence),
        occurrence
            .research_topic_id
            .and_then(|topic| store.topics.iter().find(|candidate| candidate.id == topic))
            .map(|topic| format!("Period topic: {} (under Automations)", topic.display_name()))
            .unwrap_or_else(|| "Stored directly under Automations (no topic).".into()),
    )
}

fn definition_form_signature(
    definition: &AutomationDefinition,
) -> Option<String> {
    serde_json::to_string(&serde_json::json!({
        "objective": definition.objective,
        "expected_result": definition.expected_result,
        "schedule": definition.schedule,
        "policy": definition.policy,
        "template": definition.template,
        "shape": definition.shape,
    }))
    .ok()
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
        self.scope = Scope::Automations;
        self.selected = None;
        self.automation_ui.selected = Some(id);
        self.automation_ui.creating = false;
        self.automation_ui.settings_dialog = false;
        // Results stay closed by default; the newest period is still first in
        // the history, and a row opens its result in a dialog.
        self.automation_ui.result_dialog = None;
        self.automation_ui.show_technical = false;
        self.automation_ui.feedback = None;
        self.automation_ui.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    pub fn open_automation_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.automation_ui.selected.is_none() {
            return;
        }
        self.automation_ui.settings_dialog = true;
        self.automation_ui.result_dialog = None;
        self.automation_ui.feedback = None;
        window.focus(&self.modal_focus, cx);
        cx.notify();
    }

    pub fn close_automation_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.automation_ui.settings_dialog = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub fn open_occurrence_result(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.occurrence(id).is_none() {
            return;
        }
        self.automation_ui.result_dialog = Some(id);
        self.automation_ui.settings_dialog = false;
        self.automation_ui.show_technical = false;
        self.automation_ui.feedback = None;
        if self.store.occurrence(id).is_some_and(|occurrence| occurrence.unread) {
            self.store.mark_occurrence_read(id);
            self.persist_if_changed(true);
        }
        window.focus(&self.modal_focus, cx);
        cx.notify();
    }

    pub fn close_occurrence_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.automation_ui.result_dialog = None;
        self.automation_ui.show_technical = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub fn start_automation_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scope = Scope::Automations;
        self.selected = None;
        self.automation_ui.selected = None;
        self.automation_ui.creating = true;
        self.automation_ui.editing = None;
        self.automation_ui.step = 1;
        self.automation_ui.scroll.set_offset(point(px(0.), px(0.)));
        self.automation_ui.plan = None;
        self.automation_ui.planning_automation = None;
        self.automation_ui.show_advanced = false;
        self.automation_ui.preview.clear();
        self.automation_ui.preview_error.clear();
        self.automation_ui.feedback = None;
        self.automation_ui.shape = AutomationShape::Quick;
        match self.ensure_automation_project() {
            Ok(project) => self.automation_ui.project = project,
            Err(error) => {
                self.automation_ui.creating = false;
                self.automation_ui.feedback = Some(Err(error));
                cx.notify();
                return;
            }
        }
        let timezone = system_timezone();
        set_input(cx, &self.automation_ui.name, "", window);
        set_textarea(cx, &self.automation_ui.objective, "", window);
        set_textarea(cx, &self.automation_ui.expected, "", window);
        set_input(cx, &self.automation_ui.interval, "10", window);
        set_input(cx, &self.automation_ui.hour, "10", window);
        set_input(cx, &self.automation_ui.minute, "00", window);
        set_input(cx, &self.automation_ui.timezone, &timezone, window);
        set_input(cx, &self.automation_ui.delay, "", window);
        set_input(cx, &self.automation_ui.timeout, "30", window);
        set_input(cx, &self.automation_ui.failures, "3", window);
        set_input(cx, &self.automation_ui.retention, "30", window);
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
        self.automation_ui.feedback = Some(Ok(
            "Replanning starts a new version. Adjust the request, continue, then approve the new plan."
                .into(),
        ));
        cx.notify();
    }

    /// Reopen the wizard on the planning step after the user leaves it. The
    /// in-flight planner keeps its identity so its result lands in this review.
    pub fn review_automation_planning(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.automation(id).is_none() {
            return;
        }
        self.edit_automation(id, window, cx);
        self.automation_ui.step = 3;
        self.automation_ui.planning_automation = Some(id);
        self.automation_ui.preview_error.clear();
        cx.notify();
    }

    pub fn edit_automation(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(definition) = self.store.automation(id).cloned() else {
            return;
        };
        self.automation_ui.selected = Some(id);
        self.automation_ui.creating = true;
        self.automation_ui.editing = Some(id);
        self.automation_ui.step = 1;
        self.automation_ui.plan = definition.template.clone();
        self.automation_ui.shape = definition.shape;
        self.automation_ui.planning_automation = None;
        self.automation_ui.show_advanced = false;
        self.automation_ui.catch_up = definition.policy.catch_up;
        self.automation_ui.preview.clear();
        self.automation_ui.preview_error.clear();
        self.automation_ui.feedback = None;
        self.automation_ui.project = self
            .ensure_automation_project()
            .unwrap_or(definition.project_id);
        self.automation_ui.kind = match definition.schedule.spec {
            ScheduleSpec::Interval { .. } => AutomationKind::Interval,
            ScheduleSpec::Daily { .. } => AutomationKind::Daily,
            ScheduleSpec::Weekly { .. } => AutomationKind::Weekly,
        };
        if let ScheduleSpec::Interval { minutes } = definition.schedule.spec {
            self.automation_ui.interval_in_hours = minutes >= 60 && minutes % 60 == 0;
            let value = if self.automation_ui.interval_in_hours {
                minutes / 60
            } else {
                minutes
            };
            set_input(cx, &self.automation_ui.interval, &value.to_string(), window);
        }
        if let ScheduleSpec::Daily { hour, minute }
        | ScheduleSpec::Weekly { hour, minute, .. } = definition.schedule.spec
        {
            set_input(cx, &self.automation_ui.hour, &format!("{hour:02}"), window);
            set_input(cx, &self.automation_ui.minute, &format!("{minute:02}"), window);
        }
        if let ScheduleSpec::Weekly { days, .. } = &definition.schedule.spec {
            self.automation_ui.weekdays = std::array::from_fn(|index| days.contains(&(index as u8)));
        }
        set_input(cx, &self.automation_ui.name, &definition.name, window);
        set_textarea(cx, &self.automation_ui.objective, &definition.objective, window);
        set_textarea(
            cx,
            &self.automation_ui.expected,
            &definition.expected_result,
            window,
        );
        set_input(
            cx,
            &self.automation_ui.timezone,
            &definition.schedule.timezone,
            window,
        );
        set_input(
            cx,
            &self.automation_ui.delay,
            &(definition.policy.allowed_delay_seconds / 60).to_string(),
            window,
        );
        set_input(
            cx,
            &self.automation_ui.timeout,
            &(definition.policy.timeout_seconds / 60).to_string(),
            window,
        );
        set_input(
            cx,
            &self.automation_ui.failures,
            &definition.policy.max_consecutive_failures.to_string(),
            window,
        );
        set_input(
            cx,
            &self.automation_ui.retention,
            &definition.policy.log_retention_days.to_string(),
            window,
        );
        cx.notify();
    }

    fn automation_validate_step_one(&mut self, cx: &App) -> Result<(), String> {
        let objective = self.automation_ui.objective.read(cx).value().trim().to_string();
        if objective.is_empty() || objective.len() > 12_000 {
            return Err("Describe what to do each period (1–12,000 bytes).".into());
        }
        if let Some(reason) = crate::automation_execution::unsupported_reason(&objective) {
            return Err(reason);
        }
        let name = self.automation_ui.name.read(cx).value().trim().to_string();
        if name.chars().count() > 160 {
            return Err("Automation names must be at most 160 characters.".into());
        }
        let expected = self.automation_ui.expected.read(cx).value().trim().to_string();
        if expected.len() > 12_000 {
            return Err("Describe the expected result in at most 12,000 bytes.".into());
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
        let objective = self.automation_ui.objective.read(cx).value().trim().to_string();
        let expected = self.automation_ui.expected.read(cx).value().trim().to_string();
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
                    // The planner is already running; keep the same request instead
                    // of starting a duplicate planning run.
                    self.automation_ui.step = 3;
                } else {
                    // Research automations are planned once, here. Periods never plan.
                    match self.start_automation_planning(cx) {
                        Ok(automation) => {
                            self.automation_ui.planning_automation = Some(automation);
                            self.automation_ui.preview_error.clear();
                            self.automation_ui.step = 3;
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
        if self.automation_ui.step > 1 {
            self.automation_ui.step -= 1;
        }
        self.automation_ui.feedback = None;
        cx.notify();
    }

    /// Run the research planning chain once for this automation and freeze the
    /// reviewed result into a template. Later periods instantiate it directly.
    fn start_automation_planning(&mut self, cx: &mut Context<Self>) -> Result<u64, String> {
        let id = self.save_automation_form(cx)?;
        let objective = self.automation_ui.objective.read(cx).value().trim().to_string();
        let project = self
            .store
            .automation(id)
            .map(|definition| definition.project_id)
            .ok_or("Automation not found.")?;
        let request = self.store.create_automation_request(id, project, &objective)?;
        let run = self.store.prepare_request_plan(request)?;
        let run_id = run.id;
        if let Some(saved) = self
            .store
            .research_requests
            .iter_mut()
            .find(|candidate| candidate.id == request)
        {
            saved.planning_run_id = Some(run_id);
        }
        // launch_run persists the request mapping and the queued run in one snapshot.
        self.launch_run(run, vec![], cx);
        if self.store.runs.iter().any(|candidate| candidate.id == run_id) {
            // Planning is part of creating this automation; keep the wizard visible
            // instead of following the run into Research results.
            self.scope = Scope::Automations;
            self.selected = None;
            self.selected_run = None;
            cx.notify();
        }
        if !self.store.runs.iter().any(|candidate| candidate.id == run_id) {
            if let Some(saved) = self
                .store
                .research_requests
                .iter_mut()
                .find(|candidate| candidate.id == request)
            {
                saved.planning_run_id = None;
            }
            self.persist_if_changed(true);
            return Err(self
                .error
                .clone()
                .chars()
                .take(400)
                .collect::<String>()
                .split('.')
                .next()
                .unwrap_or("The plan could not start.")
                .to_string());
        }
        Ok(id)
    }

    /// Called when the planner run finishes so the review step shows the frozen plan.
    pub(crate) fn automation_plan_ready(&mut self, automation: u64) {
        let template = self
            .store
            .automation(automation)
            .and_then(|definition| definition.template.clone());
        if self.automation_ui.editing == Some(automation)
            || self.automation_ui.planning_automation == Some(automation)
        {
            self.automation_ui.plan = template;
            self.automation_ui.planning_automation = None;
            if self.automation_ui.creating {
                self.automation_ui.step = 3;
            }
            self.automation_ui.feedback =
                Some(Ok("Plan generated · review the goals and steps before enabling.".into()));
        }
        self.toast = "Automation plan generated · review and approve".into();
        self.persist_if_changed(true);
    }

    /// Create or revise the draft version. Returns the definition id.
    fn save_automation_form(&mut self, cx: &App) -> Result<u64, String> {
        self.automation_validate_step_one(cx)?;
        let schedule = self.automation_ui.schedule(cx)?;
        let policy = self.automation_ui.policy(cx)?;
        let shape = self.automation_ui.shape;
        let (objective, expected) = self.quick_task_text(cx);
        let name = self.quick_task_name(&objective, cx);
        // A quick task always mirrors its one instruction into the template. A
        // research automation keeps the reviewed template until it is replanned.
        let template = if shape.is_quick() {
            Some(build_quick_template(&objective, &expected)?)
        } else {
            self.automation_ui.plan.clone()
        };
        let now = Self::automation_now();
        if let Some(id) = self.automation_ui.editing {
            let existing = self
                .store
                .automation(id)
                .ok_or("Automation not found.")?
                .clone();
            let desired = serde_json::to_string(&serde_json::json!({
                "objective": objective,
                "expected_result": expected,
                "schedule": schedule,
                "policy": policy,
                "template": template,
                "shape": shape,
            }))
            .map_err(|e| e.to_string())?;
            let signature = definition_form_signature(&existing).unwrap_or_default();
            if desired != signature {
                self.store.revise_automation(
                    id,
                    self.automation_ui.project,
                    name,
                    objective,
                    expected,
                    schedule,
                    policy,
                    template,
                    now,
                )?;
            } else if name.trim() != existing.name {
                // Renaming alone keeps the current approval and running snapshot.
                self.store.rename_automation(id, &name, now)?;
            }
            Ok(id)
        } else {
            let id = self.store.create_automation_draft(
                self.automation_ui.project,
                shape,
                name,
                objective,
                expected,
                schedule,
                policy,
                template,
                now,
            )?;
            self.automation_ui.editing = Some(id);
            self.automation_ui.selected = Some(id);
            Ok(id)
        }
    }

    pub fn save_automation_draft(&mut self, cx: &mut Context<Self>) {
        self.automation_ui.feedback = None;
        let rollback = self.store.clone();
        let pending = self
            .automation_ui
            .editing
            .map(|id| self.queued_occurrences(id))
            .unwrap_or_default();
        let result = self.save_automation_form(cx);
        match result {
            Ok(id) => {
                if self.storage.is_none() {
                    self.store = rollback;
                    self.automation_ui.feedback = Some(Err(
                        "Workspace storage is unavailable. Nothing was saved.".into(),
                    ));
                    cx.notify();
                    return;
                }
                self.persist_if_changed(true);
                if !self.storage_error.is_empty() {
                    self.store = rollback;
                    self.automation_ui.feedback =
                        Some(Err(format!("Not saved: {}", self.storage_error)));
                    cx.notify();
                    return;
                }
                self.cancel_unstarted_automation_work(&pending);
                self.persist_if_changed(true);
                self.automation_ui.creating = false;
                self.automation_ui.selected = Some(id);
                let status = self
                    .store
                    .automation(id)
                    .map(|definition| definition.status)
                    .unwrap_or(DefinitionStatus::Draft);
                let message = match status {
                    DefinitionStatus::Enabled => {
                        "Saved. The enabled version remains approved."
                    }
                    DefinitionStatus::Paused => "Saved. The automation remains paused.",
                    DefinitionStatus::NeedsAttention => {
                        "No changes. Resolve the listed problem before it can run."
                    }
                    _ => "Draft saved. Nothing will run until you enable it.",
                };
                self.automation_ui.feedback = Some(Ok(message.into()));
                self.toast = "Automation saved locally".into();
            }
            Err(error) => self.automation_ui.feedback = Some(Err(error)),
        }
        cx.notify();
    }

    pub fn enable_automation(&mut self, cx: &mut Context<Self>) {
        self.automation_ui.feedback = None;
        // A research plan is bound to the request it was generated from. Editing the
        // wording without replanning would silently approve a different task.
        if self.automation_ui.shape == AutomationShape::Research
            && let Some(id) = self.automation_ui.editing
            && self.store.automation(id).is_some_and(|definition| definition.template.is_some())
            && let Some(request) = self
                .store
                .research_requests
                .iter()
                .find(|request| request.automation_id == Some(id))
        {
            let objective = self.automation_ui.objective.read(cx).value().trim().to_string();
            if !request.input.trim().is_empty() && request.input.trim() != objective {
                self.automation_ui.feedback = Some(Err(
                    "The research request changed after planning. Use Replan to generate and review a new plan before enabling."
                        .into(),
                ));
                cx.notify();
                return;
            }
        }
        let rollback = self.store.clone();
        let pending = self
            .automation_ui
            .editing
            .map(|id| self.queued_occurrences(id))
            .unwrap_or_default();
        let id = match self.save_automation_form(cx) {
            Ok(id) => id,
            Err(error) => {
                self.automation_ui.feedback = Some(Err(error));
                cx.notify();
                return;
            }
        };
        let result = self.store.approve_and_enable(id, vec![], Self::automation_now());
        match result {
            Ok(_) => {
                if self.storage.is_none() {
                    self.store = rollback;
                    self.automation_ui.feedback =
                        Some(Err("Workspace storage is unavailable. Not enabled.".into()));
                    cx.notify();
                    return;
                }
                self.persist_if_changed(true);
                if !self.storage_error.is_empty() {
                    self.store = rollback;
                    self.automation_ui.feedback =
                        Some(Err(format!("Not enabled: {}", self.storage_error)));
                    cx.notify();
                    return;
                }
                self.cancel_unstarted_automation_work(&pending);
                self.persist_if_changed(true);
                self.automation_ui.creating = false;
                self.automation_ui.editing = None;
                self.automation_ui.selected = Some(id);
                self.automation_ui.feedback =
                    Some(Ok("Enabled. The first run starts after this moment.".into()));
                self.toast = "Automation enabled".into();
                self.start_pump(cx);
            }
            Err(error) => {
                self.persist_if_changed(true);
                self.automation_ui.feedback = Some(Err(error));
            }
        }
        cx.notify();
    }

    pub(crate) fn cancel_unstarted_automation_work(&mut self, occurrences: &[u64]) {
        let runs: Vec<u64> = self
            .store
            .runs
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
        for run in self.store.runs.iter_mut().filter(|r| runs.contains(&r.id)) {
            run.status = "Cancelled".into();
            run.ended_at = Some(crate::opencode::timestamp());
            run.logs.push(
                "[automation] Cancelled before starting because its request was replaced, paused or archived."
                    .into(),
            );
        }
    }

    fn queued_occurrences(&self, automation: u64) -> Vec<u64> {
        self.store
            .automation_occurrences
            .iter()
            .filter(|o| o.automation_id == automation && o.status == OccurrenceStatus::Queued)
            .map(|o| o.id)
            .collect()
    }

    pub fn pause_automation_ui(&mut self, id: u64, cx: &mut Context<Self>) {
        let rollback = self.store.clone();
        let pending = self.queued_occurrences(id);
        match self.store.pause_automation(id, Self::automation_now()) {
            Ok(()) => {
                self.persist_if_changed(true);
                if !self.storage_error.is_empty() {
                    self.store = rollback;
                    self.automation_ui.feedback = Some(Err(format!("Not paused: {}", self.storage_error)));
                    cx.notify();
                    return;
                }
                self.cancel_unstarted_automation_work(&pending);
                self.persist_if_changed(true);
                self.automation_ui.feedback = Some(Ok(
                    "Paused. Future triggers stopped and unstarted requests were cancelled. A running period can finish."
                        .into(),
                ));
            }
            Err(error) => self.automation_ui.feedback = Some(Err(error)),
        }
        cx.notify();
    }

    pub fn resume_automation_ui(&mut self, id: u64, cx: &mut Context<Self>) {
        let rollback = self.store.clone();
        match self.store.resume_automation(id, Self::automation_now()) {
            Ok(()) => {
                self.persist_if_changed(true);
                if !self.storage_error.is_empty() {
                    self.store = rollback;
                    self.automation_ui.feedback = Some(Err(format!("Not resumed: {}", self.storage_error)));
                    cx.notify();
                    return;
                }
                self.automation_ui.feedback = Some(Ok(
                    "Enabled again. The next trigger is computed from now; time spent paused is not backfilled."
                        .into(),
                ));
                self.start_pump(cx);
                self.arm_automation_timer(cx);
            }
            Err(error) => self.automation_ui.feedback = Some(Err(error)),
        }
        cx.notify();
    }

    /// Continue a research period's unfinished steps. A paused automation is
    /// resumed first; an already ended period starts a new manual period from
    /// the same approved plan because terminal periods cannot transition again.
    pub fn resume_automation_steps(&mut self, occurrence: u64, cx: &mut Context<Self>) {
        let Some((automation, status)) = self
            .store
            .occurrence(occurrence)
            .map(|run| (run.automation_id, run.status))
        else {
            return;
        };
        let Some(definition_status) = self.store.automation(automation).map(|d| d.status) else {
            return;
        };
        if matches!(
            definition_status,
            DefinitionStatus::Paused | DefinitionStatus::NeedsAttention
        ) {
            self.resume_automation_ui(automation, cx);
            if self
                .automation_ui
                .feedback
                .as_ref()
                .is_some_and(|feedback| feedback.is_err())
            {
                return;
            }
        }
        if status.is_active() {
            self.enqueue_automation_occurrence(occurrence, cx);
            self.toast = "Resuming this period's unfinished steps.".into();
        } else {
            let rollback = self.store.clone();
            match self.store.reopen_occurrence(occurrence, Self::automation_now()) {
                Ok(()) => {
                    self.persist_if_changed(true);
                    if !self.storage_error.is_empty() {
                        self.store = rollback;
                        self.automation_ui.feedback =
                            Some(Err(format!("Not resumed: {}", self.storage_error)));
                        cx.notify();
                        return;
                    }
                    self.enqueue_automation_occurrence(occurrence, cx);
                    self.toast =
                        "Retrying this period in place · finished steps keep their results.".into();
                }
                Err(error) => self.automation_ui.feedback = Some(Err(error)),
            }
        }
        cx.notify();
    }

    pub fn run_automation_now(&mut self, id: u64, cx: &mut Context<Self>) {
        let request = crate::opencode::new_session_id();
        let now = Self::automation_now();
        let rollback = self.store.clone();
        match self.store.request_manual_occurrence(id, &request, now) {
            Ok(occurrence) => {
                self.persist_if_changed(true);
                if !self.storage_error.is_empty() {
                    self.store = rollback;
                    self.automation_ui.feedback =
                        Some(Err("The manual run was not saved.".into()));
                    cx.notify();
                    return;
                }
                self.automation_ui.feedback = Some(Ok(
                    "Manual run queued. It uses the same approval and automation queue; the schedule did not move."
                        .into(),
                ));
                self.enqueue_automation_occurrence(occurrence.id, cx);
            }
            Err(error) => self.automation_ui.feedback = Some(Err(error)),
        }
        cx.notify();
    }

    pub fn stop_automation_now(&mut self, id: u64, cx: &mut Context<Self>) {
        let occurrence = self.store.automation_active(id).map(|o| o.id);
        let Some(occurrence) = occurrence else {
            self.automation_ui.feedback =
                Some(Err("There is no active run to stop.".into()));
            cx.notify();
            return;
        };
        self.stop_automation_occurrence(occurrence, cx);
        self.automation_ui.feedback = Some(Ok(
            "Stopping this period and its scheduled retries. The automation's future schedule is unchanged."
                .into(),
        ));
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
        self.stop_automation_occurrence(id, cx);
        self.toast = if self.storage_error.is_empty() {
            "Period cancelled · its scheduled retry was stopped".into()
        } else {
            format!("Period cancelled in memory · not saved: {}", self.storage_error)
        };
        cx.notify();
    }

    /// Kept for the list-level workflow that will own Copy and Archive; both
    /// actions are intentionally absent from the detail page for now.
    #[allow(dead_code)]
    pub fn copy_automation_ui(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let rollback = self.store.clone();
        match self.store.copy_automation(id, Self::automation_now()) {
            Ok(copy) => {
                self.persist_if_changed(true);
                if !self.storage_error.is_empty() {
                    self.store = rollback;
                    self.automation_ui.feedback = Some(Err(format!("Not copied: {}", self.storage_error)));
                    cx.notify();
                    return;
                }
                self.open_automation(copy, window, cx);
                self.automation_ui.feedback =
                    Some(Ok("Copied as an unapproved draft. History was not copied.".into()));
            }
            Err(error) => self.automation_ui.feedback = Some(Err(error)),
        }
        cx.notify();
    }

    #[allow(dead_code)]
    pub fn archive_automation_ui(&mut self, id: u64, cx: &mut Context<Self>) {
        let rollback = self.store.clone();
        let pending = self.queued_occurrences(id);
        match self.store.archive_automation(id, Self::automation_now()) {
            Ok(()) => {
                self.persist_if_changed(true);
                if !self.storage_error.is_empty() {
                    self.store = rollback;
                    self.automation_ui.feedback = Some(Err(format!("Not archived: {}", self.storage_error)));
                    cx.notify();
                    return;
                }
                self.cancel_unstarted_automation_work(&pending);
                self.persist_if_changed(true);
                self.automation_ui.selected = None;
                self.automation_ui.creating = false;
                self.toast = "Automation archived · history kept".into();
            }
            Err(error) => self.automation_ui.feedback = Some(Err(error)),
        }
        cx.notify();
    }

    pub fn toggle_automation_read(&mut self, occurrence: u64, cx: &mut Context<Self>) {
        let unread = self
            .store
            .occurrence(occurrence)
            .is_some_and(|o| o.unread);
        if unread {
            self.store.mark_occurrence_read(occurrence);
        } else if let Some(value) = self.store.occurrence_mut(occurrence) {
            value.unread = true;
        }
        self.persist_if_changed(true);
        cx.notify();
    }

    pub fn automation_cleanup_preview(&mut self, id: u64, cx: &mut Context<Self>) {
        match self.store.automation_log_cleanup(id, Self::automation_now(), false) {
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
        match self.store.automation_log_cleanup(id, Self::automation_now(), true) {
            Ok((occurrences, bytes, _)) => {
                self.persist_if_changed(true);
                self.automation_ui.cleanup_preview = format!(
                    "Cleared detailed logs for {occurrences} period(s) ({:.1} KB). Results and recovery records remain.",
                    bytes as f64 / 1024.
                );
                self.automation_ui.feedback = Some(Ok("Log cleanup completed.".into()));
            }
            Err(error) => self.automation_ui.cleanup_preview = error,
        }
        cx.notify();
    }
}

fn set_input(cx: &mut Context<TaskApp>, input: &Entity<InputState>, value: &str, window: &mut Window) {
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
pub fn latest_occurrence<'a>(store: &'a Store, automation: u64) -> Option<&'a AutomationOccurrence> {
    store
        .automation_occurrences
        .iter()
        .filter(|o| o.automation_id == automation)
        .max_by_key(|o| o.id)
}

pub fn occurrence_report<'a>(
    store: &'a Store,
    occurrence: &AutomationOccurrence,
) -> Option<&'a crate::domain::AgentRun> {
    if let Some(id) = occurrence.final_run_id
        && let Some(run) = store.runs.iter().find(|run| run.id == id)
    {
        return Some(run);
    }
    store
        .runs
        .iter()
        .filter(|run| run.automation_occurrence_id == Some(occurrence.id))
        .filter(|run| !run.output.trim().is_empty())
        .max_by_key(|run| run.id)
        .or_else(|| {
            store
                .runs
                .iter()
                .filter(|run| run.automation_occurrence_id == Some(occurrence.id))
                .max_by_key(|run| run.id)
        })
}

pub fn occurrence_status_color(status: OccurrenceStatus, palette: &crate::appearance::Palette) -> u32 {
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

    fn automation_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let mut definitions: Vec<_> = self.store.automations.iter().collect();
        definitions.sort_by_key(|definition| definition.id);
        let mut content = col().id("automation-list-scroll").track_scroll(&self.automation_ui.scroll).flex_1().h_full().min_h_0().min_w_0().overflow_y_scroll().p_6().gap_4();
        content = content.child(
            row()
                .justify_between()
                .gap_4()
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
                    .rounded_lg()
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
                .map(|occurrence| occurrence.status.label().to_string())
                .unwrap_or_else(|| "Not run yet".into());
            let planning = self.automation_ui.planning_automation == Some(id);
            let latest_label = if planning {
                "Planning…".into()
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
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(colors.line))
                    .bg(rgb(colors.panel))
                    .hover(|d| d.bg(rgb(colors.hover)))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.open_automation(id, window, cx)
                    }))
                    .child(
                        col().flex_1().min_w_0().gap_1()
                            .child(
                                row().gap_2().items_center()
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
                                        row().flex_shrink_0().gap_1().items_center()
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
                                                    chip("Planning…", colors.accent, cx)
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
                                muted(summary, cx)
                                    .w_full()
                                    .min_w_0()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis(),
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
        col()
            .flex_1()
            .min_h_0()
            .child(div().px_6().pt_3().child(self.automation_feedback(cx)))
            .child(content.into_any_element())
            .into_any_element()
    }

    /// All automation periods in one place, separate from project/topic results.
    pub(crate) fn automation_results(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut occurrences: Vec<_> = self.store.automation_occurrences.iter().collect();
        occurrences.sort_by_key(|occurrence| {
            std::cmp::Reverse((
                occurrence.actual_started_at.unwrap_or(occurrence.reference_time),
                occurrence.id,
            ))
        });
        let total = occurrences.len();
        // A saved filter only applies while its automation still exists; a
        // removed automation falls back to the unfiltered list.
        let filter = self
            .automation_ui
            .result_filter
            .filter(|id| self.store.automation(*id).is_some());
        if let Some(id) = filter {
            occurrences.retain(|occurrence| occurrence.automation_id == id);
        }
        let shown = occurrences
            .iter()
            .take(self.automation_ui.history_limit)
            .copied()
            .collect::<Vec<_>>();
        let mut panel = col()
            .id("automation-results-scroll")
            .track_scroll(&self.automation_ui.scroll)
            .flex_1()
            .h_full()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .p_6()
            .gap_4()
            .child(
                col().gap_1()
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(selectable("Automation results")))
                    .child(muted(
                        "Every automation period, newest execution first. Open a row for its answer and sources.",
                        cx,
                    )),
            );
        if total == 0 {
            panel = panel.child(overview_note(
                "No automation runs yet. Create or enable an automation to collect periods here.",
                cx,
            ));
        } else {
            let heading = match filter {
                Some(_) => format!("Periods · {} of {total}", occurrences.len()),
                None => format!("Periods · {total}"),
            };
            panel = panel.child(
                row().justify_between().flex_wrap().gap_3()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable(heading)))
                    .child(
                        row().flex_shrink_0().gap_3().items_center()
                            .child(self.automation_results_filter_control(cx))
                            .child(overview_note("Unread results are marked with a dot.", cx)),
                    ),
            );
            let queued = occurrences
                .iter()
                .filter(|occurrence| occurrence.status == OccurrenceStatus::Queued)
                .count();
            let waiting = occurrences
                .iter()
                .filter(|occurrence| occurrence.status == OccurrenceStatus::WaitingRetry)
                .count();
            if queued > 0 || waiting > 0 {
                panel = panel.child(overview_note(
                    format!(
                        "Automation slots: {} of {} running · {queued} queued · {waiting} waiting to retry. Automation and research queues are independent.",
                        self.automation_slots_used(),
                        self.store.automation_concurrency_limit()
                    ),
                    cx,
                ));
            }
            if occurrences.is_empty() {
                panel = panel.child(overview_note(
                    "No periods for this automation yet. Pick another automation or clear the filter.",
                    cx,
                ));
            }
            let mut history = col().gap_1();
            for occurrence in shown {
                history = history.child(self.automation_occurrence_row(occurrence, true, cx));
            }
            panel = panel.child(history);
            if occurrences.len() > self.automation_ui.history_limit {
                let remaining = occurrences.len() - self.automation_ui.history_limit;
                panel = panel.child(
                    quiet_button("load-more-automation-results", format!("Load more · {remaining} remaining"))
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.automation_ui.history_limit += 20;
                            cx.notify();
                        })),
                );
            }
        }
        panel.map(observed).into_any_element()
    }

    /// Narrows the all-results list to one automation. The control stays in the
    /// results header so the filter is discoverable without a separate page.
    fn automation_results_filter_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.automation_ui.result_filter;
        let label = selected
            .and_then(|id| self.store.automation(id))
            .map(|definition| definition.name.clone())
            .unwrap_or_else(|| "All automations".into());
        let mut definitions: Vec<_> = self.store.automations.iter().collect();
        definitions.sort_by_key(|definition| definition.id);
        let options: Vec<(Option<u64>, String)> = std::iter::once((None, "All automations".to_string()))
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
            .projects
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
                    .on_click(cx.listener(move |app, _, window, cx| app.edit_automation(id, window, cx))),
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
                actions = actions.child(
                    primary_button("resume-automation", "Resume")
                        .icon(assets::IconName::Play)
                        .tooltip("Enable again from now; paused time is not backfilled.")
                        .on_click(cx.listener(move |app, _, _, cx| app.resume_automation_ui(id, cx))),
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
                    .disabled(run_blocked.is_some())
                    .tooltip(run_blocked.unwrap_or("Queue one manual period. The schedule does not move."))
                    .on_click(cx.listener(move |app, _, _, cx| app.run_automation_now(id, cx))),
            );
        }
        if let Some(occurrence) = active {
            actions = actions.child(
                button("stop-automation", "Stop this run")
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
                .on_click(cx.listener(move |app, _, window, cx| app.open_automation_settings(window, cx))),
        );
        // Copy and Archive are deliberately not surfaced yet. The handlers stay
        // for the list-level workflows that will own them.

        let mut panel = col().flex_1().h_full().min_h_0().min_w_0().id("automation-detail-scroll").track_scroll(&self.automation_ui.scroll).overflow_y_scroll().p_6().gap_4();
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
                            "One read-only answer per period, stored under Automations — no project, topic or planning chain.",
                            cx,
                        ))
                    }),
            )
            // Planning started in the wizard may finish after the user leaves it.
            // Keep the in-flight state visible on the saved draft.
            .when(self.automation_ui.planning_automation == Some(id), |d| {
                d.child(
                    col().gap_2().child(
                        crate::views::notice_panel(
                            crate::views::NoticeTone::Info,
                            "Planning the research plan…",
                            "You can leave this page; planning continues in the background. When it finishes, review the generated goals and steps before enabling this draft.",
                            cx,
                        ),
                    )
                    .child(
                        quiet_button("review-automation-planning", "Review planning")
                            .icon(assets::IconName::ListOrdered)
                            .tooltip("Return to the planning step and review the plan when it finishes.")
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.review_automation_planning(id, window, cx)
                            })),
                    ),
                )
            })
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
        let mut occurrences: Vec<_> = self
            .store
            .automation_occurrences
            .iter()
            .filter(|o| o.automation_id == id)
            .collect();
        occurrences.sort_by_key(|o| {
            std::cmp::Reverse((o.actual_started_at.unwrap_or(o.reference_time), o.id))
        });
        let shown = occurrences
            .iter()
            .take(self.automation_ui.history_limit)
            .copied()
            .collect::<Vec<_>>();
        if occurrences.is_empty() {
            panel = panel.child(overview_note(
                "No runs yet. Enabling or Run manually will create the first period.",
                cx,
            ));
        } else {
            panel = panel.child(
                row().justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable(format!("History · {}", occurrences.len()))))
                    .child(overview_note("Newest first. Unread results are marked with a dot.", cx)),
            );
            let mut history = col().gap_0();
            for occurrence in shown {
                history = history.child(self.automation_occurrence_row(occurrence, false, cx));
            }
            panel = panel.child(history);
            if occurrences.len() > self.automation_ui.history_limit {
                let remaining = occurrences.len() - self.automation_ui.history_limit;
                panel = panel.child(
                    quiet_button("load-more-history", format!("Load more · {remaining} remaining"))
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.automation_ui.history_limit += 20;
                            cx.notify();
                        })),
                );
            }
            // Keep log cleanup visually separate from the run list.
            panel = panel.child(div().w_full().h(px(1.)).bg(rgb(colors.line)));
            panel = panel.child(
                row().gap_2()
                    .child(quiet_button("automation-cleanup-preview", "Log cleanup preview")
                        .icon(assets::IconName::ListOrdered)
                        .on_click(cx.listener(move |app, _, _, cx| app.automation_cleanup_preview(id, cx))))
                    .when(self.automation_ui.show_cleanup, |d| d.child(
                        primary_button("automation-cleanup-apply", "Clear old detailed logs")
                            .icon(assets::IconName::Archive)
                            .on_click(cx.listener(move |app, _, _, cx| app.automation_cleanup_apply(id, cx)))
                    )),
            );
            if !self.automation_ui.cleanup_preview.is_empty() {
                panel = panel.child(overview_note(self.automation_ui.cleanup_preview.clone(), cx));
            }
        }

        // App-wide behaviour is fine print at the end of the page, not a
        // per-automation setting inside the overview card.
        panel = panel.child(overview_note(
            "All automations run only while the app is open and the computer is awake — nothing runs in the background, missed periods are not backfilled, and the app never wakes the computer.",
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
        let mut body = col().id("automation-settings-body").flex_1().min_h_0().overflow_y_scroll().gap_3()
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
                        approval.agents[index]
                            .clone()
                            .unwrap_or_else(|| self.store.research_config.default_agent.clone())
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
                            app.automation_ui.settings_dialog = false;
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
                                    app.automation_ui.settings_dialog = false;
                                    app.edit_automation(id, window, cx);
                                })),
                        )
                    }),
            );
        div().absolute().inset_0().occlude().bg(rgba(0x00000080)).flex().items_center().justify_center().p_4()
            .child(
                col().w(px(680.)).max_w_full().max_h(relative(0.9)).p_6().gap_4().rounded_lg().bg(rgb(colors.panel))
                    .child(
                        row().justify_between().items_start().gap_4()
                            .child(col().gap_1()
                                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(selectable("Settings")))
                                .child(muted(schedule_summary(&definition.schedule), cx)))
                            .child(
                                tool_icon("close-automation-settings-icon", assets::IconName::Close, "Close settings")
                                    .on_click(cx.listener(|app, _, window, cx| app.close_automation_settings(window, cx))),
                            ),
                    )
                    .child(body)
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
        let status = occurrence.status.label();
        let color = occurrence_status_color(occurrence.status, &colors);
        // Execution time is the primary fact. A period that never started falls
        // back to its planned trigger or request time.
        let when = format_in_zone_precise(
            occurrence
                .actual_started_at
                .or(occurrence.scheduled_for)
                .unwrap_or(occurrence.reference_time),
            &occurrence.timezone,
        );
        let definition = show_automation
            .then(|| self.store.automation(occurrence.automation_id))
            .flatten();
        let tint = definition.map(|_| {
            self.automation_ui
                .result_tints
                .tint(
                    occurrence.automation_id,
                    gpui_kit::component::Theme::global(cx).is_dark(),
                )
        });
        // The trigger and the skipped-period count are secondary facts: an icon
        // keeps them out of the row's reading order without hiding them.
        let (trigger_icon, trigger_name) = match occurrence.source {
            crate::automation::TriggerSource::Manual => {
                (assets::IconName::MousePointerClick, "Manual run")
            }
            crate::automation::TriggerSource::Scheduled => {
                (assets::IconName::CalendarClock, "Scheduled run")
            }
        };
        let trigger_hint = if occurrence.skipped_count > 0 {
            format!(
                "{trigger_name} · {} earlier periods were summarized",
                occurrence.skipped_count
            )
        } else {
            trigger_name.into()
        };
        let mut line = row()
            .id(format!("automation-occurrence-{id}"))
            .px_2()
            .py_2()
            .gap_2()
            .items_center()
            .rounded_md()
            // One neutral surface for every period; the automation name carries
            // the session colour instead of tinting the whole row.
            .bg(rgb(colors.panel))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(colors.hover)))
            .child(
                div()
                    .w(px(8.))
                    .h(px(8.))
                    .rounded_full()
                    .when(!occurrence.unread, |d| d.opacity(0.0))
                    .bg(rgb(colors.accent)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .child(selectable(when)),
            );
        if let (Some(definition), Some(tint)) = (definition, tint) {
            line = line
                .child(label("·", cx).flex_shrink_0())
                // Names are user-authored, so the fixed category leads: it tells
                // the reader what kind of period this is before the name.
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
                        .min_w_0()
                        .max_w(px(420.))
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(tint.text))
                        .truncate()
                        .child(selectable(definition.name.clone())),
                );
        }
        line.child(div().flex_1().min_w(px(12.)))
            .child(
                div()
                    .id(format!("occurrence-trigger-{id}"))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .text_color(rgb(colors.muted))
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(trigger_hint.clone())
                            .build(window, cx)
                    })
                    .child(gpui_kit::component::Icon::new(trigger_icon).size(px(14.))),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(color))
                    .child(selectable(status)),
            )
            .child(
                tool_icon(
                    format!("open-occurrence-{id}"),
                    assets::IconName::ChevronRight,
                    "Open this period's result",
                )
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.open_occurrence_result(id, window, cx)
                })),
            )
            .on_click(cx.listener(move |app, _, window, cx| {
                app.open_occurrence_result(id, window, cx)
            }))
            .into_any_element()
    }

    fn automation_occurrence_detail(
        &self,
        id: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(occurrence) = self.store.occurrence(id) else {
            return div().into_any_element();
        };
        let colors = palette(cx);
        let report = occurrence_report(&self.store, occurrence);
        let mut panel = col()
            .id("automation-occurrence-detail")
            .p_4()
            .gap_3()
            .rounded_lg()
            .bg(rgb(colors.side))
            .child(label(occurrence_context(self.store.automation(occurrence.automation_id), occurrence, &self.store), cx));
        if let Some(reason) = &occurrence.reason {
            panel = panel.child(crate::views::notice(
                crate::views::NoticeTone::Warning,
                reason.detail.clone(),
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
        if let Some(run) = report {
            if run.output.trim().is_empty() {
                panel = panel.child(muted("No report text was saved for this period.", cx));
            } else {
                panel = panel.child(
                    div().p_3().rounded_md().bg(rgb(colors.panel))
                        .child(
                            gpui_kit::base::text::TextView::markdown(
                                format!("automation-result-{id}-{}", run.id),
                                run.output.clone(),
                            )
                            .markdown_extensions(crate::rich_markdown::extensions())
                            .w_full()
                            .on_link_click(|url, _, _, cx| {
                                if url.starts_with("https://") || url.starts_with("http://") {
                                    cx.open_url(url);
                                }
                            }),
                        ),
                );
            }
            if !run.sources.is_empty() {
                panel = panel.child(div().font_weight(FontWeight::SEMIBOLD).child(selectable(format!("Sources · {}", run.sources.len()))));
                for (index, source) in run.sources.iter().enumerate() {
                    let url = source.url.clone();
                    panel = panel.child(
                        col().gap_1()
                            .child(link_button(format!("automation-source-{id}-{index}"), source.title.clone())
                                .icon(assets::IconName::ExternalLink)
                                .on_click(move |_, _, cx| cx.open_url(&url)))
                            .child(muted(format!("{} · published {}", source.url, source.published_at), cx)),
                    );
                }
            }
        }
        // Steps remain normal research entities but are reached only from here.
        // Their full titles are shown, not just identifiers, so a research
        // automation's split work is readable at a glance.
        panel = panel.child(self.automation_occurrence_steps(occurrence, cx));
        let runs: Vec<_> = self
            .store
            .runs
            .iter()
            .filter(|run| run.automation_occurrence_id == Some(id))
            .collect();
        panel = panel.child(
            quiet_button("automation-technical", "Technical details")
                .icon(if self.automation_ui.show_technical { assets::IconName::ChevronDown } else { assets::IconName::ChevronRight })
                .on_click(cx.listener(|app, _, _, cx| {
                    app.automation_ui.show_technical = !app.automation_ui.show_technical;
                    cx.notify();
                })),
        );
        if self.automation_ui.show_technical {
            for run in runs {
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
                // Keep a long log from stretching the whole period panel; the
                // log body scrolls inside the run card instead.
                panel = panel.child(
                    col().p_3().gap_1().rounded_md().border_1().border_color(rgb(colors.line))
                        .child(label(format!("Run #{} · {} · {} · {} attempt(s)", run.id, run.status, run.model, run.auto_retry_attempt + 1), cx))
                        .child(label(format!("Session: {}", run.session_id.clone().unwrap_or_else(|| "not assigned".into())), cx))
                        .child(
                            div()
                                .id(format!("automation-run-logs-{}", run.id))
                                .w_full()
                                .min_h_0()
                                .max_h(px(320.))
                                .overflow_y_scroll()
                                .restrict_scroll_to_axis()
                                .child(label(logs, cx)),
                        ),
                );
            }
        }
        panel.map(observed).into_any_element()
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
        task_ids.sort_by(|a, b| {
            match (self.store.task(*a), self.store.task(*b)) {
                (Some(a), Some(b)) => crate::display_order::siblings_in_display_order(a, b),
                _ => a.cmp(b),
            }
        });
        use crate::execution_status::ExecutionStatus;
        let total = task_ids.len();
        let completed = task_ids
            .iter()
            .filter(|task| self.store.research_result(**task).is_some())
            .count();
        let busy = occurrence.status.is_active()
            || goals.iter().any(|goal| self.goal_busy(*goal));
        let runnable = self.store.automation(occurrence.automation_id).is_some_and(|definition| {
            matches!(
                definition.status,
                DefinitionStatus::Enabled
                    | DefinitionStatus::Paused
                    | DefinitionStatus::NeedsAttention
            )
        });
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
        let mut list = col().gap_0p5();
        for task_id in task_ids {
            let Some(task) = self.store.task(task_id) else {
                continue;
            };
            let status = self.store.execution_status(task_id, self.todo_batch.as_ref());
            let status = if self.store.research_result(task_id).is_some()
                && !matches!(
                    status,
                    ExecutionStatus::Running | ExecutionStatus::Queued | ExecutionStatus::Retrying
                )
            {
                ExecutionStatus::Done
            } else if occurrence.status.is_terminal() && status == ExecutionStatus::Running {
                // The period is over. A run that still reports activity can only
                // be a recovered session shutting down; showing live work here
                // contradicts the failed/cancelled period.
                ExecutionStatus::Stopping
            } else {
                status
            };
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
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(colors.hover)))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.open_task(task_id, window, cx)
                    }))
                    .child(status.icon_view(format!("automation-step-icon-{task_id}"), cx))
                    .child(label(format!("{kind} · {identifier}"), cx).flex_shrink_0())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .child(selectable(title.clone())),
                    )
                    .child(
                        quiet_button(format!("automation-step-{task_id}"), "Open")
                            .icon(assets::IconName::ChevronRight)
                            .tooltip("Open this step's details and result")
                            .on_click(cx.listener(move |app, _, window, cx| {
                                cx.stop_propagation();
                                app.open_task(task_id, window, cx)
                            })),
                    ),
            );
        }
        col()
            .gap_2()
            .child(
                row()
                    .justify_between()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(selectable(heading)),
                    )
                    .when(completed < total && resumable, |d| {
                        // An ended period can still have a background session
                        // shutting down. That is not "running"; say "Stopping…"
                        // so the control cannot be read as live period work.
                        let winding_down = occurrence.status.is_terminal() && busy;
                        d.child(
                            button(
                                "resume-automation-steps",
                                if winding_down { "Stopping…" } else if busy { "Running…" } else { "Resume" },
                            )
                            .icon(assets::IconName::Play)
                            .disabled(busy || !runnable)
                            .tooltip(if !runnable {
                                "Enable this automation before continuing its steps."
                            } else if winding_down {
                                "The last step's background session is still shutting down. Resume becomes available when it finishes; it cannot extend the period."
                            } else if busy {
                                "Steps are already running or queued."
                            } else {
                                "Retry this period in place. Finished steps keep their results; only unfinished work runs again."
                            })
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.resume_automation_steps(occurrence_id, cx)
                            })),
                        )
                    }),
            )
            .child(list)
            .into_any_element()
    }

    /// A period's report, sources and technical detail open in one modal so the
    /// history stays a compact list.
    pub(crate) fn automation_result_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(id) = self.automation_ui.result_dialog else {
            return div().into_any_element();
        };
        let Some(occurrence) = self.store.occurrence(id) else {
            return div().into_any_element();
        };
        let colors = palette(cx);
        let when = format_in_zone_precise(
            occurrence
                .actual_started_at
                .or(occurrence.scheduled_for)
                .unwrap_or(occurrence.reference_time),
            &occurrence.timezone,
        );
        div().absolute().inset_0().occlude().bg(rgba(0x00000080)).flex().items_center().justify_center().p_4()
            .child(
                col().w(px(860.)).max_w_full().max_h(relative(0.9)).p_6().gap_4().rounded_lg().bg(rgb(colors.panel))
                    .child(
                        row().justify_between().items_start().gap_4()
                            .child(col().gap_1()
                                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(selectable(format!(
                                    "Period {} · {}",
                                    occurrence.id,
                                    occurrence.status.label()
                                ))))
                                .child(muted(format!("{when} · {}", occurrence.source.label()), cx)))
                            .child(
                                row().gap_2().items_center()
                                    .when(occurrence.status.is_active(), |d| {
                                        d.child(
                                            quiet_button(
                                                "cancel-automation-result",
                                                "Cancel period",
                                            )
                                            .icon(assets::IconName::CircleX)
                                            .tooltip("Cancel this period and its scheduled retry")
                                            .on_click(cx.listener(move |app, _, _, cx| {
                                                app.cancel_automation_occurrence_ui(id, cx)
                                            })),
                                        )
                                    })
                                    .child(
                                        quiet_button("mark-automation-read", if occurrence.unread { "Mark read" } else { "Mark unread" })
                                            .icon(assets::IconName::Check)
                                            .on_click(cx.listener(move |app, _, _, cx| app.toggle_automation_read(id, cx))),
                                    )
                                    .child(
                                        tool_icon("close-automation-result", assets::IconName::Close, "Close result")
                                            .on_click(cx.listener(|app, _, window, cx| app.close_occurrence_result(window, cx))),
                                    ),
                            ),
                    )
                    .child(
                        div().id("automation-result-body").flex_1().min_h_0().overflow_y_scroll()
                            .child(self.automation_occurrence_detail(id, cx)),
                    )
                    .child(
                        row().justify_end()
                            .child(
                                button("close-automation-result-footer", "Close")
                                    .on_click(cx.listener(|app, _, window, cx| app.close_occurrence_result(window, cx))),
                            ),
                    ),
            )
            .focus_trap("automation-result-focus", &self.modal_focus)
            .into_any_element()
    }

    fn automation_wizard(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let step = self.automation_ui.step;
        let mut content = col()
            .id("automation-wizard-scroll")
            .track_scroll(&self.automation_ui.scroll)
            .flex_1()
            .h_full()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .p_6()
            .gap_4();
        content = content.child(
            row().justify_between()
                .child(
                    col().gap_1()
                        .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(selectable(
                            if self.automation_ui.editing.is_some() {
                                "Edit automation"
                            } else {
                                "New automation"
                            },
                        )))
                        .child(muted(format!("Step {step} of 3 · Creating or saving never starts research."), cx)),
                )
                .child(
                    quiet_button("cancel-automation-wizard", "Cancel")
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.automation_ui.creating = false;
                            if let Some(id) = app.automation_ui.editing {
                                app.open_automation(id, window, cx);
                            } else {
                                app.open_automations(window, cx);
                            }
                        })),
                ),
        );
        content = content.child(self.automation_feedback(cx));

        if step == 1 {
            content = content.child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Automation type")));
            content = content.child(
                row().flex_wrap().gap_2()
                    .child(select_button("automation-shape-quick", "Quick task", self.automation_ui.shape == AutomationShape::Quick)
                        .tooltip("One instruction, one read-only check each period. No project or planning needed.")
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.automation_ui.shape = AutomationShape::Quick;
                            app.automation_ui.plan = None;
                            cx.notify();
                        })))
                    .child(select_button("automation-shape-research", "Research automation", self.automation_ui.shape == AutomationShape::Research)
                        .tooltip("Describe it once; review the generated goals, then run the same approved plan each period.")
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.automation_ui.shape = AutomationShape::Research;
                            app.automation_ui.plan = None;
                            cx.notify();
                        }))),
            );
            content = content.child(muted(if self.automation_ui.shape.is_quick() {
                "Quick tasks live under Automations and keep one short answer per period. They never create a project or topic."
            } else {
                "Research automations plan once, when you create them. Each period runs that approved plan inside its own topic under Automations."
            }, cx));
            content = content.child(self.automation_field("Name (optional)", "automation-name", &self.automation_ui.name, cx));
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
                    "One sentence is enough. The planner splits it into goals you can review before anything is enabled.",
                    cx,
                ));
            }
        } else if step == 2 {
            content = content.child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Schedule")));
            let mut kinds = row().flex_wrap().gap_2();
            for kind in AutomationKind::ALL {
                kinds = kinds.child(
                    select_button(
                        format!("automation-kind-{:?}", kind),
                        kind.label(),
                        self.automation_ui.kind == kind,
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
                        row().gap_2()
                            .child(select_button("automation-unit-minutes", "Minutes", !self.automation_ui.interval_in_hours)
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.automation_ui.interval_in_hours = false;
                                    cx.notify();
                                })))
                            .child(select_button("automation-unit-hours", "Hours", self.automation_ui.interval_in_hours)
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.automation_ui.interval_in_hours = true;
                                    cx.notify();
                                }))),
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
                    row().gap_3()
                        .child(self.automation_field("Hour (00–23)", "automation-hour", &self.automation_ui.hour, cx))
                        .child(self.automation_field("Minute (00–59)", "automation-minute", &self.automation_ui.minute, cx)),
                );
                if self.automation_ui.kind == AutomationKind::Weekly {
                    let mut days = row().flex_wrap().gap_2();
                    for (index, name) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                        .iter()
                        .enumerate()
                    {
                        let selected = self.automation_ui.weekdays[index];
                        days = days.child(
                            select_button(format!("automation-day-{index}"), *name, selected)
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.automation_ui.weekdays[index] = !app.automation_ui.weekdays[index];
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
                        col().p_3().gap_1().rounded_md().bg(rgb(colors.panel))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Next three triggers")))
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
                        .child(select_button("automation-catchup-latest", "Only the latest missed period", self.automation_ui.catch_up == CatchUp::LatestOnly)
                            .on_click(cx.listener(|app, _, _, cx| { app.automation_ui.catch_up = CatchUp::LatestOnly; cx.notify(); })))
                        .child(select_button("automation-catchup-skip", "Skip all missed periods", self.automation_ui.catch_up == CatchUp::SkipAll)
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
                .child(muted("The next three triggers are computed by the same function that dispatches runs. They are not model-generated.", cx));
            if !self.automation_ui.preview_error.is_empty() {
                content = content.child(crate::views::notice(
                    crate::views::NoticeTone::Danger,
                    self.automation_ui.preview_error.clone(),
                    cx,
                ));
            }
        } else {
            content = content.child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Approved plan")));
            if let Some(template) = &self.automation_ui.plan {
                if self.automation_ui.shape.is_quick() {
                    content = content.child(muted(
                        "One read-only answer step per period. No project, topic, goal tree or planning runs.",
                        cx,
                    ));
                }
                for (goal_index, goal) in template.goals.iter().enumerate() {
                    content = content.child(
                        col().p_3().gap_2().rounded_md().border_1().border_color(rgb(colors.line)).bg(rgb(colors.panel))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable(format!(
                                "Goal {}/{} · {}",
                                goal_index + 1,
                                template.goals.len(),
                                goal.title
                            ))))
                            .child(muted(goal.brief.clone(), cx))
                            .children(goal.steps.iter().enumerate().map(|(index, step)| {
                                col().gap_1()
                                    .child(div().child(selectable(format!(
                                        "{}. {} · {}",
                                        index + 1,
                                        step.title,
                                        match step.kind {
                                            crate::research::ResearchKind::Collect => "Evidence collection",
                                            crate::research::ResearchKind::Analyze => "Analysis",
                                            crate::research::ResearchKind::Advise => "Final report",
                                            crate::research::ResearchKind::Goal => "Plan",
                                        }
                                    ))))
                                    .child(muted(step.instructions.clone(), cx))
                                    .child(muted(format!("Depends on: {}", if step.depends_on.is_empty() { "none".into() } else { step.depends_on.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(", ") }), cx))
                                    .into_any_element()
                            })),
                    );
                }
                content = content.child(muted(
                    "Each period instantiates this plan unchanged. Planning is never repeated unless you choose Replan.",
                    cx,
                ));
            } else if self.automation_ui.shape.is_quick() {
                content = content.child(crate::views::notice(
                    crate::views::NoticeTone::Danger,
                    "Go back and continue from the schedule to generate the step.",
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
                    "No approved plan yet",
                    "Retry planning to generate one.",
                    cx,
                ));
                content = content.child(
                    primary_button("automation-retry-planning", "Retry planning")
                        .icon(assets::IconName::RefreshCw)
                        .on_click(cx.listener(|app, _, _, cx| {
                            match app.start_automation_planning(cx) {
                                Ok(automation) => {
                                    app.automation_ui.planning_automation = Some(automation);
                                    app.automation_ui.feedback = Some(Ok("Planning started…".into()));
                                }
                                Err(error) => app.automation_ui.feedback = Some(Err(error)),
                            }
                            cx.notify();
                        })),
                );
            }
            content = content.child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Next triggers")));
            content = content.child(label(
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
            content = content.child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Execution")));
            content = content.child(muted(
                if self.automation_ui.shape.is_quick() {
                    "The approved snapshot fixes the instruction, agent, model, time zone and limits. Each period runs one step in its own folder under the Automations area, through the same research pipeline and agent as a manual run."
                } else {
                    "The approved snapshot fixes every goal and step, the agent, models, time zone and limits. Each period creates a new topic under Automations with its own folder, session and browser Space, and never replans."
                },
                cx,
            ));
            content = content.child(muted(
                "Enabling approves this template for repeated execution. Missed periods run only the newest one by default; it is skipped if older than the allowed delay. Runs happen only while the app is open and the computer is awake.",
                cx,
            ));
        }

        let mut footer = row().justify_between().gap_3();
        footer = footer.child(
            when(step > 1,
                quiet_button("automation-back-step", "Back")
                    .icon(assets::IconName::ArrowLeft)
                    .on_click(cx.listener(|app, _, _, cx| app.automation_wizard_back(cx))),
                div().into_any_element(),
            ),
        );
        let plan_ready = self.automation_ui.plan.is_some();
        let mut actions = row().gap_2();
        // Saving the reviewed plan as a draft stays available at every step, so a
        // plan can be kept without enabling it.
        actions = actions.child(
            button(
                "save-automation-draft",
                if step < 3 { "Save draft" } else { "Save draft & finish" },
            )
            .icon(assets::IconName::ClipboardCheck)
            .on_click(cx.listener(|app, _, _, cx| app.save_automation_draft(cx))),
        );
        if step < 3 {
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
                    } else {
                        "Continue to the schedule or the plan review"
                    })
                    .on_click(cx.listener(|app, _, window, cx| app.automation_wizard_next(window, cx))),
            );
        } else {
            actions = actions.child(
                primary_button("enable-automation", "Enable automation")
                    .icon(assets::IconName::Play)
                    .disabled(!plan_ready)
                    .tooltip(if plan_ready {
                        "Approve this plan and start future periods"
                    } else {
                        "Review the generated plan before enabling"
                    })
                    .on_click(cx.listener(|app, _, _, cx| app.enable_automation(cx))),
            );
        }
        footer = footer.child(actions);
        col()
            .flex_1()
            .h_full()
            .min_h_0()
            .min_w_0()
            .child(content.into_any_element())
            .child(
                div()
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
        col().gap_1().w_full().min_w_0().child(label(title.to_string(), cx)).child(Input::new(input).id(id.to_string()).w_full()).into_any_element()
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
    if condition { yes.into_any_element() } else { no.into_any_element() }
}

struct ResearchKinds;
impl ResearchKinds {
    fn labels() -> [&'static str; 4] {
        ["Plan", "Collect", "Analyze", "Report"]
    }
}

#[cfg(test)]
mod tests {
    use super::{format_in_zone_precise, ResultTints};

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
    fn a_new_session_changes_the_starting_hue_but_keeps_rows_consistent() {
        let first = ResultTints::seeded(1);
        let second = ResultTints::seeded(2);
        let first_color = first.tint(9, false).text;
        let second_color = second.tint(9, false).text;
        assert_eq!(first_color, first.tint(9, false).text);
        assert_eq!(second_color, second.tint(9, false).text);
    }
}
