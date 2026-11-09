use crate::app::Scope;
use crate::appearance::palette;
pub(crate) use crate::controls::*;
use crate::domain::{Priority, Task};
use crate::{app::*, domain::*};
use gpui_kit::base::{FocusTrapElement, h_resizable, resizable_panel};
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenuItem};
use gpui_kit::component::{
    Disableable, Selectable, Sizable,
    button::ButtonVariants,
    checkbox::Checkbox,
    input::{IndentInline, Input, OutdentInline, Textarea},
};
use gpui_kit::prelude::FluentBuilder;
#[cfg(any(test, feature = "qa"))]
use gpui_kit::test::TestSupportExt;
use gpui_kit::*;

pub(crate) fn observed<E: Element<PrepaintState = Option<Hitbox>> + InteractiveElement>(
    element: E,
) -> AnyElement {
    #[cfg(any(test, feature = "qa"))]
    {
        element.test_support().into_any_element()
    }
    #[cfg(not(any(test, feature = "qa")))]
    {
        element.into_any_element()
    }
}
use crate::navigation::CrumbTarget;

enum CollectionRow {
    Project(u64, String, usize),
    Topic(Option<u64>, String, usize),
    Task(Box<Task>),
}

pub(crate) fn row() -> Div {
    div().flex().items_center().gap_2()
}
pub(crate) fn col() -> Div {
    div().flex().flex_col()
}
/// Readable text that can be selected and copied with the mouse.
///
/// GPUI only selects text runs that opt in, so prose and values are rendered
/// through this helper instead of a bare string child. Controls that truncate
/// through a canvas (`single_line_text`) keep painting their own glyphs.
pub(crate) fn selectable(text: impl Into<SharedString>) -> impl IntoElement {
    let text = text.into();
    let id = text_element_id(&text);
    gpui_kit::base::SelectableText::new(id, text)
}

/// Stable element identity for a text run, so one label keeps its selection
/// state across frames without threading an id through every call site.
fn text_element_id(text: &str) -> ElementId {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    ElementId::Name(format!("selectable-{hash:016x}").into())
}

pub(crate) fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
    let colors = palette(cx);
    div().text_color(rgb(colors.muted)).child(selectable(text))
}
pub(crate) fn label(text: impl Into<SharedString>, cx: &App) -> Div {
    muted(text, cx).text_xs()
}
pub(crate) fn chip(text: impl Into<SharedString>, color: u32, cx: &App) -> Div {
    let colors = palette(cx);
    div()
        .px_2()
        .py_0p5()
        .rounded(px(4.))
        .bg(rgb(colors.panel))
        .text_xs()
        .text_color(rgb(color))
        .child(selectable(text))
}

/// Semantic tones for inline notices. Every notice uses a tinted surface of its
/// own tone so light and dark themes stay readable without extra branching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoticeTone {
    Success,
    Info,
    Warning,
    Danger,
}
impl NoticeTone {
    pub(crate) fn color(self, colors: &crate::appearance::Palette) -> u32 {
        match self {
            Self::Success => colors.success,
            Self::Info => colors.secondary,
            Self::Warning => colors.warning,
            Self::Danger => colors.danger,
        }
    }
    /// How much of the tone color is blended into the panel surface.
    fn tint(self) -> f32 {
        match self {
            Self::Success => 0.16,
            Self::Info => 0.08,
            Self::Warning => 0.16,
            Self::Danger => 0.14,
        }
    }
    pub(crate) fn surface(self, colors: &crate::appearance::Palette) -> u32 {
        blend_color(self.color(colors), colors.panel, self.tint())
    }
}

fn blend_color(foreground: u32, background: u32, weight: f32) -> u32 {
    let channel = |shift: u32| {
        let front = ((foreground >> shift) & 0xff) as f32;
        let back = ((background >> shift) & 0xff) as f32;
        ((front * weight + back * (1.0 - weight)).round() as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

pub(crate) fn notice(tone: NoticeTone, text: impl Into<SharedString>, cx: &App) -> Div {
    let colors = palette(cx);
    let color = tone.color(&colors);
    div()
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(rgb(color))
        .bg(rgb(tone.surface(&colors)))
        .text_color(rgb(color))
        .child(selectable(text))
}

pub(crate) fn notice_panel(
    tone: NoticeTone,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    cx: &App,
) -> Div {
    let colors = palette(cx);
    let color = tone.color(&colors);
    col()
        .p_3()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(rgb(color))
        .bg(rgb(tone.surface(&colors)))
        .text_color(rgb(color))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(selectable(title)),
        )
        .child(div().child(selectable(body)))
}
pub(crate) fn stage_color(stage: Stage, cx: &App) -> u32 {
    let colors = palette(cx);
    match stage {
        Stage::Backlog => colors.muted,
        Stage::Planned => colors.secondary,
        Stage::InProgress => colors.warning,
        Stage::Review => colors.review,
        Stage::Done => colors.success,
        Stage::Cancelled => colors.cancelled,
    }
}
fn task_state_icon(
    store: &Store,
    task: &Task,
    batch: Option<&crate::batch::TodoBatch>,
    cx: &App,
) -> AnyElement {
    let state = store.task_status(task.id, batch).execution;
    state.icon_view(format!("task-state-icon-{}", task.id), cx)
}
// All hierarchy levels use the same semantic card surfaces, including hover states.
pub(crate) fn board_card_colors(stage: Stage, cx: &App) -> (u32, u32, u32) {
    match (gpui_kit::component::Theme::global(cx).is_dark(), stage) {
        (false, Stage::Backlog) => (0xe8ebf0, 0xdde2e9, 0xc6cdd8),
        (false, Stage::Planned) => (0xdeebfa, 0xd0e2f7, 0xb1cce9),
        (false, Stage::InProgress) => (0xffedc9, 0xffe4ae, 0xe4c47e),
        (false, Stage::Review) => (0xeee2fa, 0xe4d4f5, 0xcfb7e8),
        (false, Stage::Done) => (0xdcefe5, 0xcce7d9, 0xa8d2be),
        (false, Stage::Cancelled) => (0xf2e0e2, 0xead2d6, 0xdab6bc),
        (true, Stage::Backlog) => (0x282e38, 0x313a48, 0x434d5e),
        (true, Stage::Planned) => (0x20354e, 0x294361, 0x3b6086),
        (true, Stage::InProgress) => (0x49391e, 0x594625, 0x7d6132),
        (true, Stage::Review) => (0x3a2b4d, 0x49365f, 0x674b82),
        (true, Stage::Done) => (0x203e33, 0x294e40, 0x3e705c),
        (true, Stage::Cancelled) => (0x442c33, 0x543740, 0x754c57),
    }
}
// Tree rows own hover feedback so the title and disclosure read as one item.
pub(crate) fn sidebar_variant(
    active: bool,
    cx: &App,
) -> gpui_kit::component::button::ButtonCustomVariant {
    let colors = palette(cx);
    gpui_kit::component::button::ButtonCustomVariant::new(cx)
        .foreground(
            rgb(if active {
                colors.sidebar_selected
            } else {
                colors.text
            })
            .into(),
        )
        .color(transparent_black())
        .hover(transparent_black())
        .active(transparent_black())
}

fn section(title: &str, cx: &App) -> Div {
    label(title.to_string(), cx).pt_3().pb_1().px_3()
}

impl TaskApp {
    pub(crate) fn layout_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        self.layout_switch_for(false, cx)
    }
    pub(crate) fn layout_switch_for(&self, steps: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let layout = if steps {
            self.steps_layout()
        } else {
            self.layout
        };
        row()
            .flex_shrink_0()
            .gap_0()
            .p(px(2.))
            .rounded(px(7.))
            .bg(rgb(colors.side))
            .child(
                row_button(
                    if steps {
                        "steps-list-view"
                    } else {
                        "list-view"
                    },
                    "List",
                )
                .icon(assets::IconName::Menu)
                .selected(layout == Layout::List)
                .when(layout == Layout::List, |b| b.bg(rgb(colors.panel)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if steps {
                        this.set_steps_layout(Layout::List);
                    } else {
                        this.layout = Layout::List;
                    }
                    cx.notify();
                })),
            )
            .child(
                row_button(
                    if steps {
                        "steps-board-view"
                    } else {
                        "board-view"
                    },
                    "Board",
                )
                .icon(assets::IconName::LayoutDashboard)
                .selected(layout == Layout::Board)
                .when(layout == Layout::Board, |b| b.bg(rgb(colors.panel)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if steps {
                        this.set_steps_layout(Layout::Board);
                    } else {
                        this.layout = Layout::Board;
                    }
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    fn task_location_label(&self, task: &Task) -> String {
        let id = self.store.identifier(task);
        match task.research.as_ref().map(|r| r.kind) {
            Some(crate::research::ResearchKind::Goal) => format!("Research goal · {id}"),
            Some(crate::research::ResearchKind::Collect) => format!("Evidence · {id}"),
            Some(crate::research::ResearchKind::Analyze) => format!("Analysis · {id}"),
            Some(crate::research::ResearchKind::Advise) => format!("Final report · {id}"),
            None => id,
        }
    }

    fn save_feedback_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        if let Some(feedback) = &self.save_feedback {
            let (message, tone) = match feedback {
                Ok(message) => (message, NoticeTone::Success),
                Err(message) => (message, NoticeTone::Danger),
            };
            let color = tone.color(&colors);
            return row()
                .id("save-feedback")
                .mx_4()
                .my_2()
                .flex_shrink_0()
                .p_4()
                .gap_3()
                .items_start()
                .rounded_lg()
                .bg(rgb(tone.surface(&colors)))
                .border_1()
                .border_color(rgb(color))
                .child(
                    div().text_lg().text_color(rgb(color)).child(
                        gpui_kit::component::Icon::new(if tone == NoticeTone::Danger {
                            assets::IconName::CircleAlert
                        } else {
                            assets::IconName::Check
                        })
                        .size_4(),
                    ),
                )
                .child(
                    col()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(
                            if tone == NoticeTone::Danger {
                                "Not saved"
                            } else {
                                "Saved"
                            },
                        ))
                        .child(div().text_sm().child(selectable(message.clone()))),
                )
                .child(
                    tool_icon(
                        "dismiss-save-feedback",
                        assets::IconName::Close,
                        "Dismiss save feedback",
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.save_feedback = None;
                        cx.notify();
                    })),
                )
                .map(observed)
                .into_any_element();
        }
        div().into_any_element()
    }
    pub fn set_appearance(
        &mut self,
        mode: crate::appearance::Appearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.apply_scoped_settings(
            crate::scoped_settings::SettingsChange::Appearance(mode),
            cx,
        ) {
            crate::appearance::apply(mode, Some(window), cx);
        }
        cx.notify();
    }
    /// Sidebar identity row at the bottom dock. It names the workspace, is the
    /// only entry point for the local data folder and replaces the separate
    /// Settings nav item; the menu's Settings item opens that page. Clicking the
    /// row opens a menu instead of acting as a plain label.
    fn workspace_switcher(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let path = self
            .storage
            .as_ref()
            .map(|storage| storage.path().to_path_buf());
        let location = path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "storage unavailable".into());
        let entity = cx.entity().downgrade();
        let reveal = path.clone();
        let copy = location.clone();
        let unavailable = path.is_none();
        let active = self.sidebar_scope() == Scope::Settings;
        let trigger = button("workspace-menu", "")
            .ghost()
            .flex_1()
            .min_w_0()
            .h_8()
            .px_3()
            .custom(sidebar_variant(active, cx).hover(rgb(colors.hover).into()))
            .when(active, |b| b.font_weight(FontWeight::SEMIBOLD))
            .accessibility_label("Personal workspace menu")
            .tooltip(format!(
                "Personal workspace · stored on this Mac at {location}"
            ))
            .child(
                row()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .child(
                        gpui_kit::component::Icon::new(assets::IconName::HardDrive)
                            .size(px(20.))
                            .flex_shrink_0()
                            .text_color(rgb(colors.muted)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_sm()
                            .child("Personal workspace"),
                    )
                    .child(
                        gpui_kit::component::Icon::new(assets::IconName::ChevronDown)
                            .size(px(14.))
                            .flex_shrink_0()
                            .text_color(rgb(colors.muted)),
                    ),
            );
        trigger
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                let reveal = reveal.clone();
                let mut menu = menu
                    .item(
                        PopupMenuItem::new("Reveal data folder")
                            .icon(assets::IconName::Folder)
                            .disabled(unavailable)
                            .on_click(move |_, _, cx| {
                                if let Some(path) = reveal.as_ref() {
                                    cx.reveal_path(path);
                                }
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Copy data folder path")
                            .icon(assets::IconName::Copy)
                            .disabled(unavailable)
                            .on_click({
                                let source = copy.clone();
                                let entity = entity.clone();
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        source.clone(),
                                    ));
                                    let _ = entity.update(cx, |app, cx| {
                                        app.toast = "Data folder path copied".into();
                                        cx.notify();
                                    });
                                }
                            }),
                    );
                menu = menu.item(PopupMenuItem::separator()).item(
                    PopupMenuItem::new("Settings…")
                        .icon(assets::IconName::Settings)
                        .on_click({
                            let entity = entity.clone();
                            move |_, w, cx| {
                                let _ = entity.update(cx, |app, cx| {
                                    app.record_origin();
                                    app.navigate(Scope::Settings, w, cx);
                                    // The popup item that triggered this action is
                                    // removed with the menu. Focus the page root so
                                    // Cmd+[ keeps reaching the app shortcut handler.
                                    w.focus(&app.focus, cx);
                                });
                            }
                        }),
                );
                menu
            })
            .into_any_element()
    }
    /// The retained footer surface. Its definite height lets the cached view
    /// reuse layout and paint; each real parent notification re-evaluates the
    /// whole-Store saved comparison and every command state.
    pub(crate) fn footer_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        row()
            .w_full()
            .min_w_0()
            .h(px(30.))
            .px_4()
            .bg(rgb(colors.side))
            .border_t_1()
            .border_color(rgb(colors.line))
            .justify_between()
            .gap_4()
            .text_xs()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(self.footer_activity(cx)),
            )
            .child(self.footer_commands(cx))
            .into_any_element()
    }
    /// Footer left: persistence and last-action feedback. Save failures use the
    /// danger tone and a retry action instead of borrowing the neutral message
    /// style, and the idle state is a real state rather than a fixed sentence.
    fn footer_activity(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let failed = !self.storage_error.is_empty();
        if self.dispatch_parked
            && let Some(message) = self.dispatch_admission_error.as_ref()
        {
            // A parked dispatch obligation is real work waiting for one explicit
            // retry. Keep it in the compact footer instead of a modal.
            return row()
                .gap_2()
                .child(
                    gpui_kit::component::Icon::new(assets::IconName::CircleAlert)
                        .size(px(14.))
                        .flex_shrink_0()
                        .text_color(rgb(colors.danger)),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(rgb(colors.danger))
                        .child(selectable(format!("Dispatch needs attention · {message}"))),
                )
                .child(
                    button("retry-dispatch-admission", "Retry dispatch")
                        .small()
                        .h(px(24.))
                        .px_2()
                        .tooltip("Retry the pending dispatch operation")
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.retry_dispatch_admission(cx);
                        })),
                )
                .into_any_element();
        }
        if failed && self.storage.is_some() {
            return row()
                .gap_2()
                .child(
                    gpui_kit::component::Icon::new(assets::IconName::CircleAlert)
                        .size(px(14.))
                        .flex_shrink_0()
                        .text_color(rgb(colors.danger)),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(rgb(colors.danger))
                        .child(selectable(format!("Not saved · {}", self.storage_error))),
                )
                .child(
                    button("retry-save", "Retry save")
                        .small()
                        .h(px(24.))
                        .px_2()
                        .tooltip("Write the workspace to local storage again")
                        .on_click(cx.listener(|app, _, w, cx| app.retry_save(w, cx))),
                )
                .into_any_element();
        }
        if self.storage.is_none() {
            return row()
                .gap_2()
                .child(
                    gpui_kit::component::Icon::new(assets::IconName::CircleAlert)
                        .size(px(14.))
                        .flex_shrink_0()
                        .text_color(rgb(colors.warning)),
                )
                .child(muted(
                    if self.storage_error.is_empty() {
                        "Storage unavailable · changes are only in memory".to_string()
                    } else {
                        format!("Not saved · {}", self.storage_error)
                    },
                    cx,
                ))
                .into_any_element();
        }
        if !self.error.is_empty() {
            return row()
                .gap_2()
                .child(
                    gpui_kit::component::Icon::new(assets::IconName::CircleAlert)
                        .size(px(14.))
                        .flex_shrink_0()
                        .text_color(rgb(colors.danger)),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(rgb(colors.danger))
                        .child(selectable(self.error.clone())),
                )
                .into_any_element();
        }
        let saving = self.has_unsaved_changes();
        let message = if saving {
            "Saving changes…".to_string()
        } else if self.toast.is_empty() {
            "All changes saved".to_string()
        } else {
            self.toast.clone()
        };
        row()
            .gap_2()
            .child(
                gpui_kit::component::Icon::new(if saving {
                    assets::IconName::CircleDot
                } else {
                    assets::IconName::CircleCheck
                })
                .size(px(14.))
                .flex_shrink_0()
                .text_color(rgb(if saving {
                    colors.accent
                } else {
                    colors.muted
                })),
            )
            .child(muted(message, cx))
            .into_any_element()
    }
    /// Footer right: every item here is a real command. Shortcut hints live in
    /// tooltips so the row cannot look like clickable text that does nothing.
    fn footer_commands(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let blocked_reason = self.undo_block_reason();
        let undo_hint = if let Some(reason) = blocked_reason.as_deref() {
            reason
        } else if self.undo.is_empty() {
            self.undo_invalidation
                .as_deref()
                .unwrap_or("Nothing to undo")
        } else {
            "Undo the last change"
        };
        row()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .child(
                button("undo", "Undo")
                    .small()
                    .h(px(24.))
                    .px_2()
                    .disabled(self.undo.is_empty() || blocked_reason.is_some())
                    .tooltip(undo_hint)
                    .on_click(cx.listener(|app, _, w, cx| app.undo_change(w, cx))),
            )
            .child(div().w(px(1.)).h(px(14.)).mx_1().bg(rgb(colors.line)))
            .child(
                button("footer-search", "Search")
                    .icon(assets::IconName::Search)
                    .small()
                    .h(px(24.))
                    .px_2()
                    .tooltip("Search all research and automations · ⌘K")
                    .on_click(cx.listener(|app, _, w, cx| app.focus_search(w, cx))),
            )
            .child(
                button("footer-new-research", "New research")
                    .icon(assets::IconName::Plus)
                    .small()
                    .h(px(24.))
                    .px_2()
                    .tooltip("Describe a research question · ⌘N")
                    .on_click(cx.listener(|app, _, w, cx| app.request_new_research(w, cx))),
            )
            .into_any_element()
    }
    /// Shared by the ⌘K handler and the footer Search button so the shortcut and
    /// the visible command cannot drift apart. Opening the overlay never rewrites
    /// the route, selection, list filters, scroll position or an open draft.
    pub(crate) fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_global_search(window, cx);
    }
    pub(crate) fn sidebar_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(feature = "qa")]
        crate::brand::note_sidebar_render();
        let colors = palette(cx);
        let shell = col()
            .w_full()
            .flex_shrink_0()
            .h_full()
            .bg(rgb(colors.side))
            .border_r_1()
            .border_color(rgb(colors.line))
            .p_3();
        let identity = row()
            .h(px(60.))
            .flex_shrink_0()
            .px_2()
            .gap_3()
            .child(observed(
                div()
                    .id("sidebar-brand-slot")
                    .size(px(32.))
                    .flex_shrink_0(),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(relative(1.3))
                    .child(selectable("What the hell are you planning?")),
            );
        let mut side = col()
            .id("sidebar-scroll")
            .track_scroll(&self.sidebar_scroll)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        // Automations lead the sidebar: it is the app's scheduled-work domain,
        // separate from the Research domain below it.
        let automation_count = self
            .store
            .automations()
            .iter()
            .filter(|automation| {
                !matches!(
                    automation.status,
                    crate::automation::DefinitionStatus::Archived
                )
            })
            .count();
        let automation_periods = self.store.all_occurrences().len();
        side = side
            .child(section("Automations", cx))
            .child(self.automation_child_nav(
                "all-automations",
                "All automations",
                Scope::Automations,
                automation_count,
                cx,
            ))
            .child(self.automation_child_nav(
                "automation-results",
                "Automation results",
                Scope::AutomationRuns,
                automation_periods,
                cx,
            ))
            .child(section("Research", cx));
        let count = self
            .store
            .tasks()
            .iter()
            .filter(|t| {
                !self.store.task_is_archived(t.id)
                    && t.parent_id.is_none()
                    && t.automation_id.is_none()
            })
            .count();
        side = side.child(self.nav("all", "All research", Scope::All, count, cx));
        let cycles: std::collections::BTreeSet<_> = self
            .store
            .tasks()
            .iter()
            .filter(|t| !self.store.task_is_archived(t.id))
            .filter_map(|t| t.cycle.as_deref())
            .collect();
        if !cycles.is_empty() {
            side = side.child(section("Cycles", cx));
        }
        for (i, name) in cycles.into_iter().enumerate() {
            let scope = Scope::Cycle(name.to_string());
            let count = self
                .store
                .tasks()
                .iter()
                .filter(|t| !self.store.task_is_archived(t.id) && t.cycle.as_deref() == Some(name))
                .count();
            side = side.child(self.nav(format!("cycle-{i}"), name, scope, count, cx));
        }
        // Projects are part of the Research domain; the left border and indent
        // keep them visually subordinate to All research.
        let mut projects_block = col()
            .gap_0()
            .ml_2()
            .pl_1()
            .border_l_1()
            .border_color(rgb(colors.line));
        projects_block = projects_block.child(
            row()
                .justify_between()
                .pl_1()
                .pr_3()
                .pt_2()
                .child(label("Projects", cx))
                .child(
                    tool_icon("new-project", assets::IconName::Plus, "Create project")
                        .tooltip("Create project")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.remember_dialog_opener(window, cx);
                            let input = cx.new(|cx| {
                                gpui_kit::component::input::InputState::new(window, cx)
                                    .placeholder("Project name")
                            });
                            input.update(cx, |s, cx| s.focus(window, cx));
                            this.new_project = Some(input);
                            this.error.clear();
                            cx.notify();
                        })),
                ),
        );
        let mut projects: Vec<_> = self.store.visible_projects().collect();
        projects.sort_by_key(|project| crate::display_order::newest_id(project.id));
        for project in projects {
            let count = self
                .store
                .tasks()
                .iter()
                .filter(|t| {
                    !self.store.task_is_archived(t.id)
                        && t.parent_id.is_none()
                        && t.project_id == project.id
                        && t.automation_id.is_none()
                        && !matches!(t.stage, Stage::Done | Stage::Cancelled)
                })
                .count();
            let pid = project.id;
            let expanded = self.expanded_sidebar_projects.contains(&pid);
            let active = self.sidebar_scope() == Scope::Project(pid);
            projects_block = projects_block.child(
                row()
                    .gap_0()
                    .rounded_md()
                    .hover(|d| d.bg(rgb(colors.hover)))
                    .child(
                        row_button(format!("project-{pid}"), "")
                            .custom(sidebar_variant(active, cx))
                            .when(active, |b| b.font_weight(FontWeight::SEMIBOLD))
                            .flex_1()
                            .min_w_0()
                            .h_8()
                            .px_3()
                            .tooltip(project.name.clone())
                            .accessibility_label(project.name.clone())
                            .child(
                                row()
                                    .w_full()
                                    .min_w_0()
                                    .gap_2()
                                    .child(
                                        gpui_kit::component::Icon::new(assets::IconName::Folder)
                                            .size(px(20.))
                                            .flex_shrink_0(),
                                    )
                                    .child(single_line_text(
                                        format!("sidebar-project-title-{pid}"),
                                        project.name.clone(),
                                    ))
                                    .child(label(
                                        if count > 0 {
                                            count.to_string()
                                        } else {
                                            String::new()
                                        },
                                        cx,
                                    )),
                            )
                            .on_click(cx.listener(move |a, _, w, cx| {
                                a.request_scope(Scope::Project(pid), w, cx)
                            })),
                    )
                    .child(
                        tool_icon(
                            format!("expand-project-{pid}"),
                            if expanded {
                                assets::IconName::ChevronDown
                            } else {
                                assets::IconName::ChevronRight
                            },
                            if expanded {
                                "Collapse project tasks"
                            } else {
                                "Expand project tasks"
                            },
                        )
                        .custom(sidebar_variant(active, cx))
                        .h_8()
                        .flex_shrink_0()
                        .on_click(cx.listener(move |a, _, _, cx| {
                            if a.expanded_sidebar_projects.contains(&pid) {
                                a.expanded_sidebar_projects.retain(|id| *id != pid);
                            } else {
                                a.expanded_sidebar_projects.push(pid);
                                a.remember_active_project(pid);
                            }
                            cx.notify();
                        })),
                    ),
            );
            if expanded {
                let mut project_tasks: Vec<_> = self
                    .store
                    .tasks()
                    .iter()
                    .filter(|t| t.project_id == pid && !self.store.task_is_archived(t.id))
                    .collect();
                project_tasks
                    .sort_by(|a, b| crate::display_order::tree_siblings_in_display_order(a, b));
                let mut list = col()
                    .gap_0()
                    .ml_4()
                    .pl_1()
                    .border_l_1()
                    .border_color(rgb(colors.line));
                let mut groups: Vec<_> = self
                    .store
                    .topics()
                    .iter()
                    .filter(|t| t.project_id == pid && !t.archived && t.automation_id.is_none())
                    .map(|t| Some(t.id))
                    .collect();
                // Topic IDs increase monotonically at creation, including legacy topics.
                groups.sort_by_key(|id| id.map(crate::display_order::newest_id));
                groups.push(None);
                for topic_id in groups {
                    let tasks: Vec<_> = project_tasks
                        .iter()
                        .copied()
                        .filter(|t| self.store.task_topic(t.id).map(|topic| topic.id) == topic_id)
                        .collect();
                    if let Some(topic) =
                        topic_id.and_then(|id| self.store.topics().iter().find(|t| t.id == id))
                    {
                        list = list.child(self.topic_sidebar_row(topic, cx));
                        if self.collapsed_sidebar_topics.contains(&topic.id) {
                            continue;
                        }
                    }
                    if tasks.is_empty()
                        && topic_id.is_none()
                        && self.store.topics().iter().all(|t| t.project_id != pid)
                    {
                        list = list.child(div().px_2().py_2().child(label("No tasks yet", cx)));
                    }
                    // Preserve the parent tree for research steps and manually added subtasks.
                    let mut pending: Vec<_> = tasks
                        .iter()
                        .rev()
                        .filter(|t| {
                            t.parent_id
                                .is_none_or(|id| !tasks.iter().any(|p| p.id == id))
                        })
                        .map(|t| (*t, 0usize))
                        .collect();
                    let mut ordered = Vec::new();
                    let mut visited = std::collections::BTreeSet::new();
                    while let Some((task, depth)) = pending.pop() {
                        if !visited.insert(task.id) {
                            continue;
                        }
                        ordered.push((task, depth));
                        if !self.collapsed_sidebar_goals.contains(&task.id) {
                            pending.extend(
                                tasks
                                    .iter()
                                    .rev()
                                    .filter(|t| t.parent_id == Some(task.id))
                                    .map(|t| (*t, depth + 1)),
                            );
                        }
                    }
                    for (task, depth) in ordered {
                        let nested = depth > 0;
                        let tid = task.id;
                        let has_steps = tasks.iter().any(|t| t.parent_id == Some(tid));
                        let status = self.display_task_status(tid);
                        let execution = status.execution;
                        let hint_heading =
                            format!("{} · {}", self.store.identifier(task), status.label(),);
                        let hint_title = task.title.clone();
                        let hint_reason = status.reason;
                        let active = self.selected().or(self.task_result_context()) == Some(tid);
                        let task_button = row_button(format!("sidebar-task-{tid}"), "")
                            .custom(sidebar_variant(active, cx))
                            .when(active, |b| b.font_weight(FontWeight::SEMIBOLD))
                            .flex_1()
                            .min_w_0()
                            .h_7()
                            .px_2()
                            .map(|mut button| {
                                button.interactivity().tooltip(move |window, cx| {
                                    let width =
                                        px(360.).min(window.viewport_size().width - px(64.));
                                    let heading = hint_heading.clone();
                                    let title = hint_title.clone();
                                    let reason = hint_reason.clone();
                                    component::tooltip::Tooltip::element(move |_, cx| {
                                        col()
                                            .w(width)
                                            .gap_2()
                                            .whitespace_normal()
                                            .child(label(heading.clone(), cx))
                                            .child(
                                                div()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(selectable(title.clone())),
                                            )
                                            .child(muted(reason.clone(), cx).text_sm())
                                    })
                                    .p_3()
                                    .build(window, cx)
                                });
                                button
                            })
                            .accessibility_label(format!(
                                "{} · {}",
                                task.title,
                                self.display_task_status(tid).label()
                            ))
                            .child(
                                row()
                                    .w_full()
                                    .min_w_0()
                                    .gap_2()
                                    .child(execution.icon_view(format!("task-state-icon-{tid}"), cx))
                                    .child(single_line_text(
                                        format!("sidebar-task-title-{tid}"),
                                        format!(
                                            "{} · {}",
                                            if nested {
                                                self.store
                                                    .identifier(task)
                                                    .rsplit('-')
                                                    .next()
                                                    .unwrap_or("")
                                                    .to_string()
                                            } else {
                                                self.store.identifier(task)
                                            },
                                            task.title
                                        ),
                                    )),
                            )
                            .on_click(cx.listener(move |a, _, w, cx| {
                                a.request_sidebar_task(pid, tid, w, cx);
                            }));
                        list = list.child(
                            row()
                                .gap_0()
                                .min_w_0()
                                .ml(px((depth as f32
                                    + if topic_id.is_some() { 1. } else { 0. })
                                    * 10.))
                                .when(nested, |d| {
                                    d.pl_1().border_l_1().border_color(rgb(colors.line))
                                })
                                .when(!nested, |d| d.rounded_md())
                                .hover(|d| d.bg(rgb(colors.hover)))
                                .id(format!("sidebar-task-row-{tid}"))
                                .when(active, |d| d.track_scroll(&self.sidebar_selected_row))
                                .child(task_button)
                                .when(has_steps, |d| {
                                    d.child(
                                        tool_icon(
                                            format!("sidebar-goal-toggle-{tid}"),
                                            if self.collapsed_sidebar_goals.contains(&tid) {
                                                assets::IconName::ChevronRight
                                            } else {
                                                assets::IconName::ChevronDown
                                            },
                                            "Expand or collapse research steps",
                                        )
                                        .custom(sidebar_variant(active, cx))
                                        .h_7()
                                        .flex_shrink_0()
                                        .on_click(
                                            cx.listener(move |a, _, _, cx| {
                                                if a.collapsed_sidebar_goals.contains(&tid) {
                                                    a.collapsed_sidebar_goals
                                                        .retain(|id| *id != tid);
                                                } else {
                                                    a.collapsed_sidebar_goals.push(tid);
                                                }
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                }),
                        );
                    }
                }
                projects_block = projects_block.child(list);
            }
        }
        side = side.child(projects_block);
        side = side
            .child(self.nav(
                "recovery",
                "Paused projects",
                Scope::Recovery,
                self.store.paused_project_goals().len(),
                cx,
            ))
            .child(self.nav(
                "runs",
                "Research results",
                Scope::Runs,
                self.store.progress_result_task_count(),
                cx,
            ))
            // Archive only covers ordinary research tasks and goals. Keep it in
            // the Research group so a long sidebar never makes it look like a
            // child of the Automations section.
            .child(
                self.nav(
                    "archive",
                    "Archive",
                    Scope::Archive,
                    self.store
                        .tasks()
                        .iter()
                        .filter(|t| {
                            t.automation_id.is_none()
                                && t.parent_id.is_none()
                                && self.store.task_is_archived(t.id)
                        })
                        .count(),
                    cx,
                ),
            );
        let appearance = self.store.appearance();
        shell
            .when(cfg!(target_os = "macos"), |d| {
                // Reserve space for the native traffic lights outside the scroll area.
                d.child(
                    gpui_kit::component::TitleBar::new()
                        .h(px(32.))
                        .pl_0()
                        .border_b_0()
                        .bg(rgb(colors.side)),
                )
            })
            .child(identity)
            .child(side.map(observed))
            .child(
                row()
                    .flex_shrink_0()
                    .gap_0()
                    .mt_1()
                    .pt_1()
                    .border_t_1()
                    .border_color(rgb(colors.line))
                    .child(self.workspace_switcher(cx))
                    .child(
                        tool_icon(
                            "toggle-appearance",
                            match appearance {
                                crate::appearance::Appearance::Light => assets::IconName::Sun,
                                crate::appearance::Appearance::Dark => assets::IconName::Moon,
                                crate::appearance::Appearance::System => assets::IconName::Monitor,
                            },
                            match appearance {
                                crate::appearance::Appearance::System => {
                                    "Following system · click for Light"
                                }
                                crate::appearance::Appearance::Light => "Light · click for Dark",
                                crate::appearance::Appearance::Dark => {
                                    "Dark · click to follow system"
                                }
                            },
                        )
                        .h_8()
                        .on_click(cx.listener(|a, _, w, cx| {
                            a.set_appearance(a.store.appearance().next(), w, cx);
                        })),
                    ),
            )
            .into_any_element()
    }
    fn nav(
        &self,
        id: impl Into<ElementId>,
        title: &str,
        scope: Scope,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = palette(cx);
        let active = self.sidebar_scope() == scope;
        let icon = match &scope {
            Scope::All => assets::IconName::Layers,
            Scope::Project(_) | Scope::Topic(_) => assets::IconName::Folder,
            Scope::Cycle(_) => assets::IconName::RefreshCw,
            Scope::Archive => assets::IconName::Archive,
            Scope::Runs => assets::IconName::ChartNoAxesCombined,
            Scope::Recovery => assets::IconName::Pause,
            Scope::Research => assets::IconName::Search,
            Scope::Automations => assets::IconName::Clock,
            Scope::AutomationRuns => assets::IconName::ChartNoAxesCombined,
            Scope::Settings => assets::IconName::Settings,
        };
        button(id, "")
            .ghost()
            .accessibility_label(title.to_string())
            .tooltip(if scope == Scope::All {
                "Research tasks and goals across your projects. Automations are in their own section below.".to_string()
            } else if scope == Scope::Runs {
                "Research tasks in progress or with saved results, counted once each. Planning and retry history do not add to this number.".to_string()
            } else if scope == Scope::Archive {
                "Archived research tasks and goals. Automation definitions are archived from their own page.".to_string()
            } else { title.to_string() })
            .w_full()
            .h_8()
            .px_3()
            .custom(sidebar_variant(active, cx).hover(rgb(colors.hover).into()))
            .when(active, |b| b.font_weight(FontWeight::SEMIBOLD))
            .child(
                row()
                    .w_full()
                    .min_w_0()
                    .justify_between()
                    .gap_2()
                    .child(
                        gpui_kit::component::Icon::new(icon)
                            .size(px(20.))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_sm()
                            .child(selectable(title.to_string())),
                    )
                    .child(label(
                        if count > 0 {
                            count.to_string()
                        } else {
                            String::new()
                        },
                        cx,
                    ).id(format!("nav-count-{title}")).aria_label(count.to_string()).map(observed)),
            )
            .on_click(
                cx.listener(move |this, _, window, cx| this.request_scope(scope.clone(), window, cx)),
            )
            .into_any_element()
    }

    fn automation_child_nav(
        &self,
        id: &'static str,
        title: &str,
        scope: Scope,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = palette(cx);
        let active = self.sidebar_scope() == scope;
        let icon = match scope {
            Scope::AutomationRuns => assets::IconName::ChartNoAxesCombined,
            _ => assets::IconName::Clock,
        };
        button(id, "")
            .ghost()
            .accessibility_label(title.to_string())
            .tooltip(if scope == Scope::AutomationRuns {
                "All automation periods, newest execution first".to_string()
            } else {
                "Automation definitions and their schedules".to_string()
            })
            .w_full()
            .h_8()
            .px_3()
            .custom(sidebar_variant(active, cx).hover(rgb(colors.hover).into()))
            .when(active, |b| b.font_weight(FontWeight::SEMIBOLD))
            .child(
                row()
                    .w_full()
                    .min_w_0()
                    .justify_between()
                    .gap_2()
                    .child(
                        gpui_kit::component::Icon::new(icon)
                            .size(px(20.))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_sm()
                            .child(selectable(title.to_string())),
                    )
                    .child(label(
                        if count > 0 {
                            count.to_string()
                        } else {
                            String::new()
                        },
                        cx,
                    )),
            )
            .on_click(
                cx.listener(move |app, _, window, cx| app.request_scope(scope.clone(), window, cx)),
            )
            .into_any_element()
    }

    pub(crate) fn navigation_path(&self) -> (Vec<(String, String, CrumbTarget)>, String) {
        // The automation list is its own top-level section. It must not borrow
        // Research's "All research" crumb (and Back must not lead there).
        if self.scope() == Scope::Automations
            && !self.automation_ui.creating
            && self.automation_ui.selected.is_none()
            && self.selected().is_none()
            && self.task_result_context().is_none()
        {
            return (vec![], "Automations".into());
        }
        // A new automation is created from the Automations list; it is not a
        // child of Research's All either.
        if self.scope() == Scope::Automations
            && self.automation_ui.creating
            && self.automation_ui.editing.is_none()
        {
            return (
                vec![(
                    "location-automations".into(),
                    "Automations".into(),
                    CrumbTarget::View(Scope::Automations),
                )],
                "New automation".into(),
            );
        }
        // Editing an existing automation belongs to its detail page, not to the
        // Automations list. Back and the breadcrumb return to that page.
        if self.scope() == Scope::Automations
            && self.automation_ui.creating
            && let Some(id) = self.automation_ui.editing
            && let Some(definition) = self.store.automation(id)
        {
            return (
                vec![
                    (
                        "location-automations".into(),
                        "Automations".into(),
                        CrumbTarget::View(Scope::Automations),
                    ),
                    (
                        "location-automation".into(),
                        definition.name.clone(),
                        CrumbTarget::Automation(id),
                    ),
                ],
                "Edit automation".into(),
            );
        }
        // Settings is a workspace-level destination reached from the workspace
        // menu, not a child of Research. Back uses an explicit return frame when
        // one exists; the structural trail has no All-research parent.
        if self.settings_page() {
            return (vec![], "Settings".into());
        }
        // Automation results are a child of Automations, not of Research. A step
        // opened from a period keeps its own task path below.
        if self.scope() == Scope::AutomationRuns
            && self.selected().is_none()
            && self.task_result_context().is_none()
        {
            return (
                vec![(
                    "location-automations".into(),
                    "Automations".into(),
                    CrumbTarget::View(Scope::Automations),
                )],
                "Automation results".into(),
            );
        }
        // An automation detail sits under Automations, so the app-level Back
        // returns to the list instead of jumping straight to All.
        if self.scope() == Scope::Automations
            && !self.automation_ui.creating
            && self.selected().is_none()
            && self.task_result_context().is_none()
            && let Some(id) = self.automation_ui.selected
            && let Some(definition) = self.store.automation(id)
        {
            return (
                vec![(
                    "location-automations".into(),
                    "Automations".into(),
                    CrumbTarget::View(Scope::Automations),
                )],
                definition.name.clone(),
            );
        }
        let mut links = vec![];
        let result = self.selected().is_none() && self.scope() == Scope::Runs;
        let run = result
            .then(|| self.displayed_run_id())
            .flatten()
            .and_then(|id| self.store.runs().iter().find(|r| r.id == id));
        let task = self
            .selected()
            .or(self.task_result_context())
            .or_else(|| run.map(|r| r.task_id))
            .and_then(|id| self.store.task(id));
        let mut current = self.title();
        if (self.scope() != Scope::All || task.is_some())
            && !task.is_some_and(|task| task.automation_id.is_some())
        {
            links.push((
                "location-all".into(),
                "All research".into(),
                CrumbTarget::View(Scope::All),
            ));
        }
        if let Some(task) = task {
            if let Some(automation) = task.automation_id {
                links.push((
                    "location-automations".into(),
                    "Automations".into(),
                    CrumbTarget::View(Scope::Automations),
                ));
                if let Some(definition) = self.store.automation(automation) {
                    links.push((
                        "location-automation".into(),
                        definition.name.clone(),
                        CrumbTarget::AutomationDetail(definition.id),
                    ));
                }
                let goal = task.research.as_ref().map(|research| research.goal_id);
                if let Some(occurrence) = self.store.all_occurrences().iter().find(|occurrence| {
                    goal.is_some_and(|goal| {
                        occurrence.research_goal_id == Some(goal)
                            || occurrence.research_goal_ids.contains(&goal)
                    })
                }) {
                    links.push((
                        format!("location-automation-period-{}", occurrence.id),
                        format!("Period {}", occurrence.id),
                        CrumbTarget::AutomationResult {
                            automation,
                            occurrence: occurrence.id,
                        },
                    ));
                }
                return (links, self.task_location_label(task));
            }
            if let Some(project) = self.store.projects().iter().find(|p| p.id == task.project_id) {
                links.push((
                    "location-project".into(),
                    project.name.clone(),
                    CrumbTarget::View(Scope::Project(project.id)),
                ));
            }
            if let Some(topic) = self.store.task_topic(task.id) {
                links.push((
                    "location-topic".into(),
                    topic.display_name(),
                    CrumbTarget::View(Scope::Topic(topic.id)),
                ));
            }
            let mut ancestors = vec![];
            let mut seen = vec![task.id];
            let mut parent = task.parent_id;
            while let Some(t) = parent.and_then(|id| self.store.task(id)) {
                if seen.contains(&t.id) {
                    break;
                }
                seen.push(t.id);
                ancestors.push(t);
                parent = t.parent_id;
            }
            for t in ancestors.iter().rev() {
                let key = if Some(t.id) == task.parent_id {
                    "location-parent".into()
                } else {
                    format!("location-ancestor-{}", t.id)
                };
                links.push((key, self.task_location_label(t), CrumbTarget::Task(t.id)));
            }
            current = self.task_location_label(task);
            if result {
                links.push(("location-task".into(), current, CrumbTarget::Task(task.id)));
                current = if run
                    .is_some_and(|r| r.research_kind == Some(crate::research::ResearchKind::Goal))
                {
                    "Planning result"
                } else {
                    "Result"
                }
                .into();
            }
        } else if let Some(project) = run
            .and_then(|r| r.project_plan)
            .and_then(|id| self.store.projects().iter().find(|p| p.id == id))
        {
            links.push((
                "location-project".into(),
                project.name.clone(),
                CrumbTarget::View(Scope::Project(project.id)),
            ));
            current = "Project planning".into();
        } else if let Scope::Topic(id) = self.scope() {
            if let Some(project) = self
                .store
                .topics()
                .iter()
                .find(|t| t.id == id)
                .and_then(|t| self.store.projects().iter().find(|p| p.id == t.project_id))
            {
                links.push((
                    "location-project".into(),
                    project.name.clone(),
                    CrumbTarget::View(Scope::Project(project.id)),
                ));
            }
        }
        (links, current)
    }

    fn navigation_header(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let (links, current) = self.navigation_path();
        let destination = links.last().map(|(_, title, _)| title.clone());
        let back_available = destination.is_some() || self.has_return_frames();
        let mut trail = row()
            .id("location-trail")
            .flex_1()
            .min_w_0()
            .max_w_full()
            .overflow_hidden()
            .gap_1();
        for (id, title, target) in links {
            trail = trail
                .child(
                    row_button(id, title.clone())
                        .h_7()
                        .px_1()
                        .min_w_0()
                        .flex_shrink_1()
                        .max_w(px(150.))
                        .text_color(rgb(colors.secondary))
                        .tooltip(format!("Open {title}"))
                        .on_click(cx.listener(move |a, _, w, cx| {
                            use crate::draft_exit::ExitTarget;
                            use crate::navigation_context::NavigationTarget as NT;
                            let target = match &target {
                                CrumbTarget::View(scope) => ExitTarget::Structural(NT::Research {
                                    scope: scope.clone(),
                                }),
                                CrumbTarget::Task(id) => {
                                    ExitTarget::Structural(NT::ResearchTask(*id))
                                }
                                CrumbTarget::Automation(id) => {
                                    ExitTarget::Structural(NT::AutomationSettings(*id))
                                }
                                CrumbTarget::AutomationDetail(id) => {
                                    ExitTarget::Structural(NT::AutomationDetail(*id))
                                }
                                CrumbTarget::AutomationResult {
                                    automation,
                                    occurrence,
                                } => ExitTarget::Structural(NT::AutomationPeriod {
                                    automation: *automation,
                                    occurrence: *occurrence,
                                }),
                            };
                            a.request_exit(target, w, cx);
                        })),
                )
                .child(label("/", cx));
        }
        trail = trail.child(
            single_line_text("location-current", current)
                .min_w(px(64.))
                .flex_shrink_1()
                .font_weight(FontWeight::MEDIUM)
                .map(observed),
        );
        let sidebar_width = self
            .sidebar_split
            .read(cx)
            .sizes()
            .first()
            .copied()
            .unwrap_or(px(280.));
        let header_width = (window.viewport_size().width - sidebar_width - px(1.)).max(px(320.));
        let mut header = row()
            .id("navigation-header")
            .w(header_width)
            .min_w_0()
            .h(px(44.))
            .flex_shrink_0()
            .px_4()
            .gap_3()
            .border_b_1()
            .border_color(rgb(colors.line))
            .when(back_available, |d| {
                d.child(
                    quiet_button("navigation-back", "Back")
                        .icon(assets::IconName::ArrowLeft)
                        .h_7()
                        .px_1()
                        .flex_shrink_0()
                        .disabled(!back_available)
                        .tooltip(
                            destination
                                .clone()
                                .map(|name| format!("Back to {name} · ⌘["))
                                .unwrap_or_else(|| "Back to previous page · ⌘[".into()),
                        )
                        .on_click(cx.listener(|a, _, w, cx| a.request_back(w, cx))),
                )
            })
            .child(trail.map(observed));
        let options: Vec<_> = self
            .tabs
            .iter()
            .rev()
            .filter_map(|id| {
                self.store
                    .task(*id)
                    .filter(|t| !self.store.task_is_archived(t.id))
                    .map(|_| (*id, self.store.task_navigation_label(*id)))
            })
            .collect();
        if !options.is_empty() {
            let entity = cx.entity().downgrade();
            let selected = self.selected();
            header = header.child(
                quiet_button("open-tasks-menu", "Recently opened")
                    .dropdown_caret(true)
                    .flex_shrink_0()
                    .text_color(rgb(colors.muted))
                    .tooltip("Switch between recently opened task pages. This list does not indicate whether tasks are running or completed.")
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                        for (id, title) in &options {
                            let entity = entity.clone();
                            let id = *id;
                            let label = title.clone();
                            menu = menu.item(
                                labeled_menu_item(format!("recent-task-{id}"), label.clone())
                                    .checked(selected == Some(id))
                                    .on_click(move |_, w, cx| {
                                        let _ = entity.update(cx, |a, cx| {
                                            let surface = a
                                                .navigation
                                                .recent_surfaces
                                                .get(&id)
                                                .copied()
                                                .unwrap_or(
                                                    crate::navigation_context::RecentSurface::Task,
                                                );
                                            a.request_recent(id, surface, w, cx);
                                        });
                                    }),
                            );
                        }
                        if selected.is_some() {
                            let entity = entity.clone();
                            menu = menu.separator().item(
                                PopupMenuItem::new("Close current task").on_click(
                                    move |_, w, cx| {
                                        let _ =
                                            entity.update(cx, |a, cx| a.close_current_task(w, cx));
                                    },
                                ),
                            );
                        }
                        menu
                    }),
            );
        }
        if cfg!(target_os = "macos") {
            gpui_kit::component::TitleBar::new()
                .w_full()
                .h(px(44.))
                .pl_0()
                .border_b_0()
                .bg(rgb(colors.canvas))
                .child(
                    div()
                        .flex_1()
                        .w(px(0.))
                        .min_w_0()
                        .overflow_hidden()
                        .child(header.map(observed)),
                )
                .into_any_element()
        } else {
            header.map(observed).into_any_element()
        }
    }
    fn toolbar(&self, count: usize, cx: &mut Context<Self>) -> AnyElement {
        let project = if let Scope::Project(id) = self.scope() {
            Some(id)
        } else {
            None
        };
        let summary = if let Some(id) = project {
            let pending = self
                .store
                .tasks()
                .iter()
                .filter(|t| {
                    t.project_id == id
                        && t.parent_id.is_none()
                        && !t.archived
                        && self.store.research_pending_approval(t.id)
                })
                .count();
            if pending > 0 {
                format!("{count} goals · grouped by topic · {pending} awaiting approval")
            } else {
                format!("{count} goals · grouped by topic")
            }
        } else {
            format!(
                "{count} goals · {}",
                if self.layout == Layout::List {
                    "grouped by project"
                } else {
                    "grouped by topic"
                }
            )
        };
        row()
            .h(px(60.))
            .px_6()
            .gap_4()
            .flex_shrink_0()
            .child(
                col()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        row().min_w_0().gap_2().child(
                            single_line_text("collection-title", self.title())
                                .h_6()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .map(observed),
                        ),
                    )
                    .child(
                        label(summary.clone(), cx)
                            .id("collection-summary")
                            .aria_label(summary)
                            .map(observed),
                    ),
            )
            .when_some(project, |d, id| {
                d.child(self.project_toolbar_actions(id, cx))
            })
            .when(project.is_none(), |d| {
                d.child(
                    button("new-task", "New research")
                        .icon(assets::IconName::Plus)
                        .when(project.is_some(), |b| b.ghost())
                        .when(project.is_none(), |b| b.primary())
                        .on_click(cx.listener(|this, _, w, cx| this.request_new_research(w, cx))),
                )
            })
            .into_any_element()
    }
    fn all_status_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.all_status;
        let entity = cx.entity().downgrade();
        quiet_button("all-status-filter", selected.label())
            .dropdown_caret(true)
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(palette(cx).accent))
            .bg(rgb(palette(cx).accent_surface))
            .tooltip("Filter tasks by status")
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                for filter in [AllStatus::All, AllStatus::Open]
                    .into_iter()
                    .chain(Stage::BOARD_ORDER.map(AllStatus::Stage))
                    .chain(crate::workflow::ExecutionGroup::ALL.map(AllStatus::Execution))
                {
                    let entity = entity.clone();
                    menu = menu.item(
                        PopupMenuItem::new(filter.label())
                            .checked(filter == selected)
                            .on_click(move |_, _, cx| {
                                let _ = entity.update(cx, |this, cx| {
                                    this.all_status = filter;
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }
    /// The single content boundary rendered inside the retained content surface.
    /// Every branch is rebuilt when a real parent notification reaches the
    /// surface; animation frames reuse the retained subtree.
    pub(crate) fn main_content(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(id) = self.selected() {
            self.detail(id, cx)
        } else {
            match self.scope() {
                Scope::Topic(id) => self.topic_view(id, cx),
                Scope::Runs => self.runs(cx),
                Scope::Recovery => self.recovery_view(cx),
                Scope::Research => self.research_view(cx),
                Scope::Automations => self.automations(cx),
                Scope::AutomationRuns => self.automation_results(cx),
                Scope::Settings => self.model_settings(cx),
                _ => self.collection(cx),
            }
        }
    }
    fn collection(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let tasks = self.visible_tasks();
        let mut main = col().size_full().child(self.toolbar(tasks.len(), cx)).child(
            row()
                .px_6()
                .pb_4()
                .gap_3()
                .when(self.scope() == Scope::All, |d| {
                    d.child(self.all_status_control(cx))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&self.search).id("task-search")),
                )
                .child(self.layout_switch(cx))
                .child(
                    tool_icon("display", assets::IconName::Settings2, "Display options")
                        .selected(self.show_display)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_display = !this.show_display;
                            cx.notify();
                        })),
                ),
        );
        if self.scope() == Scope::Archive && self.layout == Layout::List {
            main = main.child(self.topic_directory(None, cx));
        }
        if let Scope::Project(id) = self.scope() {
            let pending = self
                .store
                .research_requests()
                .iter()
                .filter(|r| {
                    r.project_id == id
                        && r.goal_ids.is_empty()
                        && !self.store.request_is_archived(r)
                })
                .count();
            if pending > 0 {
                main = main.child(
                    row()
                        .px_6()
                        .pb_2()
                        .child(label(
                            format!("{pending} saved requests awaiting a plan"),
                            cx,
                        ))
                        .child(button("saved-requests", "Open requests").on_click(
                            cx.listener(|a, _, w, cx| a.navigate(Scope::Research, w, cx)),
                        )),
                );
            }
            main = main.child(self.todo_batch_bar(Some(id), cx));
        }
        if self.query().ends_with("status:") || self.query().ends_with("stage:") {
            main = main.child(
                row()
                    .px_6()
                    .pb_3()
                    .children(Stage::ALL.into_iter().map(|stage| {
                        button(format!("suggest-{}", stage.key()), stage.label()).on_click(
                            cx.listener(move |this, _, w, cx| {
                                let q = format!("{}{}", this.query(), stage.key());
                                this.search.update(cx, |s, cx| s.set_value(q, w, cx));
                            }),
                        )
                    })),
            );
        }
        if self.show_display {
            main = main.child(
                row()
                    .px_6()
                    .pb_4()
                    .gap_5()
                    .when(self.scope() != Scope::All, |d| {
                        d.child(
                            Checkbox::new("show-done")
                                .label("Show completed")
                                .checked(self.show_done)
                                .on_change(cx.listener(|this, v, _, cx| {
                                    this.show_done = *v;
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        Checkbox::new("compact")
                            .label("Compact rows")
                            .checked(self.compact)
                            .on_change(cx.listener(|this, v, _, cx| {
                                this.compact = *v;
                                cx.notify();
                            })),
                    ),
            );
        }
        if tasks.is_empty() {
            return main
                .child(
                    col()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .child(div().text_xl().child(selectable("No tasks here")))
                        .child(muted("Try another filter, or create your first task.", cx))
                        .child(
                            button("clear-filters", "Reset filters").on_click(cx.listener(
                                |this, _, w, cx| {
                                    this.search.update(cx, |s, cx| s.set_value("", w, cx));
                                    this.show_done = true;
                                    if this.scope() == Scope::All {
                                        this.all_status = AllStatus::All;
                                    }
                                    cx.notify();
                                },
                            )),
                        ),
                )
                .into_any_element();
        }
        if self.layout == Layout::Board {
            return main.child(self.board(&tasks, cx)).into_any_element();
        }
        let ordered = self.order_visible_tasks(&tasks);
        let row_height = if self.compact { 32. } else { 38. };
        let mut rows = Vec::new();
        let grouped = !matches!(self.scope(), Scope::Project(_));
        let mut last_project = None;
        let mut last_topic = None;
        for task in ordered {
            if !grouped {
                let topic = self.store.task_topic(task.id);
                let topic_id = topic.map(|t| t.id);
                if last_topic != Some(topic_id) {
                    let count = tasks
                        .iter()
                        .filter(|t| {
                            self.store.task_topic(t.id).map(|t| t.id) == topic_id
                                && t.parent_id.is_none()
                        })
                        .count();
                    rows.push(CollectionRow::Topic(
                        topic_id,
                        topic
                            .map(|t| t.display_name())
                            .unwrap_or_else(|| "Unassigned topic".into()),
                        count,
                    ));
                    last_topic = Some(topic_id);
                }
            }
            if grouped && last_project != Some(task.project_id) {
                let name = self
                    .store
                    .projects()
                    .iter()
                    .find(|p| p.id == task.project_id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "Unassigned project".into());
                let count = tasks
                    .iter()
                    .filter(|t| t.project_id == task.project_id)
                    .count();
                rows.push(CollectionRow::Project(task.project_id, name, count));
                last_project = Some(task.project_id);
            }
            rows.push(CollectionRow::Task(Box::new(task)));
        }
        let archive = self.scope() == Scope::Archive;
        main.child(
            div().id("collection-table-scroll").flex_1().min_h_0().min_w_0()
                .overflow_x_scroll().restrict_scroll_to_axis()
                .child(col().w_full().min_w(px(if archive { 0. } else { 1100. })).h_full().child(
            row()
                .px_6()
                .pb_2()
                .gap_3()
                .text_xs()
                .text_color(rgb(colors.muted))
                .child(div().w(px(156.)).flex_shrink_0().child(selectable("PRIORITY / ID")))
                .child(div().flex_1().min_w_0().child(selectable("TASK")))
                .child(div().w(px(150.)).flex_shrink_0().child(selectable("STATUS")))
                .when(!archive, |d| d
                    .child(div().w(px(84.)).flex_shrink_0().child(selectable("LABELS")))
                    .child(div().w_16().flex_shrink_0().child(selectable("DUE")))
                    .child(div().w(px(156.)).flex_shrink_0().child(selectable("CREATED")))),
        )
        .child(
            uniform_list(
                "task-list",
                rows.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {                    range
                        .map(|i| match &rows[i] {
                            CollectionRow::Task(task) => this.task_row(task, row_height, cx),
                            CollectionRow::Topic(id, name, count) => {
                                let id = *id;
                                let key = id.map(|id| id.to_string()).unwrap_or_else(|| "unassigned".into());
                                row_button(format!("collection-topic-{key}"), "")
                                    .h(px(row_height)).w_full().px_6().rounded_none()
                                    .bg(rgb(palette(cx).side))
                                    .tooltip(name.clone())
                                    .accessibility_label(format!("{name} · {count} goals · Open topic"))
                                    .child(row().w_full().min_w_0().gap_3()
                                        .child(gpui_kit::component::Icon::new(assets::IconName::Folder).size_4().flex_shrink_0())
                                        .child(single_line_text(format!("collection-topic-title-{key}"), name.clone()).flex_1().min_w_0().font_weight(FontWeight::SEMIBOLD))
                                        .child(label(format!("{count} goals"), cx).flex_shrink_0())
                                        .child(gpui_kit::component::Icon::new(assets::IconName::ChevronRight).size_4().flex_shrink_0()))
                                    .on_click(cx.listener(move |a, _, w, cx| {
                                        if let Some(id) = id {
                                            a.request_scope(Scope::Topic(id), w, cx);
                                        }
                                    })).into_any_element()
                            }
                            CollectionRow::Project(id, name, count) => {
                                let id = *id;
                                row_button(format!("collection-project-{id}"), "")
                                    .h(px(row_height))
                                    .w_full()
                                    .px_6()
                                    .rounded_none()
                                    .bg(rgb(palette(cx).side))
                                    .accessibility_label(format!("Open project {name}"))
                                    .child(
                                        row()
                                            .w_full()
                                            .min_w_0()
                                            .gap_2()
                                            .child(
                                                gpui_kit::component::Icon::new(
                                                    assets::IconName::Folder,
                                                )
                                                .size_4(),
                                            )
                                            .child(
                                                single_line_text(
                                                    format!("collection-project-title-{id}"),
                                                    name.clone(),
                                                )
                                                .font_weight(FontWeight::SEMIBOLD),
                                            )
                                            .child(label(format!("{count} goals"), cx)),
                                    )
                                    .on_click(cx.listener(move |this, _, w, cx| {
                                        this.open_collection_project(id, w, cx)
                                    }))
                                    .into_any_element()
                            }
                        })
                        .collect()
                }),
            )
            .track_scroll(&self.navigation.task_list_scroll)
            .h_full()
            .flex_1(),
        )
        ).map(observed))
        .into_any_element()
    }
    pub fn ordered_visible_tasks(&self) -> Vec<Task> {
        self.order_visible_tasks(&self.visible_tasks())
    }
    fn order_visible_tasks(&self, tasks: &[Task]) -> Vec<Task> {
        let ids: Vec<_> = tasks.iter().map(|t| t.id).collect();
        let mut ordered = vec![];
        for t in tasks
            .iter()
            .filter(|t| t.parent_id.is_none() || !ids.contains(&t.parent_id.unwrap()))
        {
            ordered.push(t.clone());
            if !self.collapsed.contains(&t.id) {
                append_children(tasks, t.id, &self.collapsed, &mut ordered);
            }
        }
        if !matches!(self.scope(), Scope::Project(_)) {
            ordered.sort_by_key(|t| crate::display_order::newest_id(t.project_id));
        }
        ordered
    }
    fn task_row(&self, t: &Task, height: f32, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let id = t.id;
        let archived = self.store.task_is_archived(id);
        let child_count = self
            .store
            .tasks()
            .iter()
            .filter(|c| c.parent_id == Some(id) && !c.archived)
            .count();
        let done = self
            .store
            .tasks()
            .iter()
            .filter(|c| {
                c.parent_id == Some(id)
                    && !c.archived
                    && self.display_task_status(c.id).phase == Stage::Done
            })
            .count();
        row()
            .id(format!("task-row-{id}"))
            .w_full()
            .h(px(height))
            .px_6()
            .gap_3()
            .border_b_1()
            .border_color(rgb(colors.subtle_line))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(colors.hover)))
            .on_click(cx.listener(move |this, _, w, cx| this.request_task(id, false, w, cx)))
            .child(
                div().w(px(156.)).flex_shrink_0().child(
                    row()
                        .child(
                            div()
                                .w_5()
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .text_xs()
                                .text_color(rgb(if t.priority == Priority::Urgent {
                                    colors.danger
                                } else {
                                    colors.muted
                                }))
                                .child(
                                    gpui_kit::component::Icon::new(priority_icon(t.priority))
                                        .size_4(),
                                ),
                        )
                        .child(
                            row()
                                .gap_1()
                                .child(label(self.store.identifier(t), cx))
                                .child(
                                    gpui_kit::component::Icon::new(assets::IconName::ChevronRight)
                                        .size_4(),
                                ),
                        ),
                ),
            )
            .child(
                row()
                    .flex_1()
                    .min_w_0()
                    .when(t.parent_id.is_some(), |d| d.pl_5())
                    .child(
                        row()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .child(self.execution_indicator(id, cx))
                            .child(
                                single_line_text(format!("task-title-{id}"), t.title.clone())
                                    .when(matches!(t.stage, Stage::Done | Stage::Cancelled), |d| {
                                        d.text_color(rgb(colors.muted))
                                    })
                                    .when(child_count > 0, |d| d.font_weight(FontWeight::MEDIUM)),
                            ),
                    )
                    .when(child_count > 0, |d| {
                        let step_name = format!(
                            "Open Steps for {} · {done} of {child_count} complete",
                            t.title
                        );
                        d.child(
                            row_button(format!("goal-steps-{id}"), format!("{done}/{child_count}"))
                                .icon(assets::IconName::ChevronRight)
                                .tooltip(step_name.clone())
                                .accessibility_label(step_name)
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    cx.stop_propagation();
                                    // Typed destination survives dirty-exit replay.
                                    this.request_goal_steps(id, w, cx);
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .w(px(150.))
                    .flex_shrink_0()
                    .text_xs()
                    .id(format!("task-status-{id}"))
                    .aria_label(self.display_task_status(id).label())
                    .child(self.display_task_status(id).label())
                    .map(observed),
            )
            .when(archived, |d| {
                d.child(
                    quiet_button(format!("restore-archived-{id}"), "Restore")
                        .icon(assets::IconName::RotateCw)
                        .h(px(28.))
                        .px_2()
                        .flex_shrink_0()
                        .tooltip("Restore this archived research and its retained results")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.restore_archived(id, cx);
                        })),
                )
            })
            .when(!archived, |d| {
                d.child(
                    div().w(px(84.)).flex_shrink_0().overflow_hidden().child(
                        row().children(
                            t.labels
                                .iter()
                                .take(1)
                                .map(|l| label(l.clone(), cx).truncate()),
                        ),
                    ),
                )
            })
            .when(!archived, |d| {
                d.child(
                    div()
                        .w_16()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(rgb(
                            if !t.due_date.is_empty()
                                && t.due_date.as_str()
                                    < chrono::Local::now().date_naive().to_string().as_str()
                            {
                                colors.danger
                            } else {
                                colors.muted
                            },
                        ))
                        .child(if t.due_date.is_empty() {
                            "".into()
                        } else {
                            t.due_date[5..].to_string()
                        }),
                )
            })
            .when(!archived, |d| {
                d.child(
                    row_button(format!("task-created-{id}"), "")
                        .w(px(156.))
                        .flex_shrink_0()
                        .p_0()
                        .text_xs()
                        .text_color(rgb(colors.muted))
                        .tooltip(if t.created_at.is_empty() {
                            "Creation time was not recorded for this older task.".to_string()
                        } else {
                            format!("Created: {}", t.created_at)
                        })
                        .child(div().w_full().whitespace_nowrap().text_xs().child(
                            if t.created_at.is_empty() {
                                "Not recorded".to_string()
                            } else {
                                t.created_at
                                    .split_whitespace()
                                    .take(2)
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            },
                        )),
                )
            })
            .map(observed)
            .map(|element| self.task_context(element, id, cx))
            .into_any_element()
    }
    pub(crate) fn board(&self, tasks: &[Task], cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let archive = self.scope() == Scope::Archive;
        col().flex_1().min_h_0()
            .when(!archive, |d| d.child(div().id("board-guidance").px_6().pb_2().child(label(
                "Drop Todo tasks into Execution to submit. Waiting → Queued → Running; problems appear under Needs attention.", cx))))
            .when(archive, |d| d.child(div().id("archive-board-guidance").px_6().pb_2().child(label(
                "Archived history · retained results. Restore a goal to resume work.", cx))))
            .child(row()
            .id("board-scroll")
            .track_scroll(&self.navigation.board_scroll)
            .items_start()
            .gap_4()
            .px_6()
            .pb_5()
            .flex_1()
            .min_h_0()
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .children(Stage::BOARD_ORDER.into_iter()
                .filter(|stage| !archive || tasks.iter().any(|t| t.parent_id.is_none()
                    && self.display_task_status(t.id).phase == *stage))
                .map(|stage| {
                let entity = cx.entity().downgrade();
                let cards: Vec<_> = tasks
                    .iter()
                    .filter(|t| t.parent_id.is_none() && self.display_task_status(t.id).phase == stage)
                    .cloned()
                    .collect();
                col()
                    .id(format!("column-{}", stage.key()))
                    .w(px(if stage == Stage::InProgress { 350. } else { 292. }))
                    .flex_shrink_0()
                    .h_full()
                    .gap_3()
                    .rounded_md()
                    .when(!archive, |d| d
                        .drag_over::<DragTask>(move |d, drag, _, cx| {
                            let allowed = entity.upgrade().is_some_and(|entity| {
                                let app = entity.read(cx);
                                app.store.validate_manual_transition(drag.id, stage, app.todo_batch.as_ref()).is_ok()
                            });
                            d.bg(rgb(if allowed { colors.hover } else { colors.danger }))
                        })
                        .on_drop(cx.listener(move |this, drag: &DragTask, _, cx| {
                            this.change_stage(drag.id, stage, cx)
                        })))
                    .child(
                        row()
                            .justify_between()
                            .h_9()
                            .child(
                                row()
                                    .child(
                                        div()
                                            .text_color(rgb(stage_color(stage, cx)))
                                            .child(crate::workflow::phase_label(stage)),
                                    )
                                    .child(label(cards.len().to_string(), cx))
                                    .when(stage == Stage::InProgress && !archive, |d| d.child(self.execution_status_filter(&cards, cx))),
                            )
                            .when(!archive, |d| d.child(
                                tool_icon(
                                    format!("add-{}", stage.key()),
                                    assets::IconName::Plus,
                                    "New research starts in Backlog",
                                )
                                .on_click(cx.listener(
                                    move |this, _, w, cx| this.request_new_research(w, cx),
                                )),
                            )),
                    )
                    .child(
                        col()
                            .id(format!("cards-{}", stage.key()))
                            .track_scroll(&self.navigation.board_column(stage))
                            .overflow_y_scroll()
                            .restrict_scroll_to_axis()
                            .flex_1()
                            .min_h_0()
                            .gap_3()
                            .children(if stage == Stage::InProgress && !archive {
                                self.execution_board_cards(&cards, cx)
                            } else { self.topic_grouped_cards(&cards, stage.key(), cx) })
                            .when(cards.is_empty() && !archive, |d| {
                                d.child(
                                    div()
                                        .id(format!("empty-drop-{}", stage.key()))
                                        .h_24()
                                        .border_1()
                                        .border_color(rgb(colors.line))
                                        .rounded_md()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(label("Drop a goal here", cx)),
                                )
                            }),
                    )
                    .map(observed)
            })).map(observed))
            .into_any_element()
    }
    pub(crate) fn goal_card(&self, t: &Task, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        // Keep each status recognizable even when a card is hovered.
        let dark = gpui_kit::component::Theme::global(cx).is_dark();
        let archived = self.store.task_is_archived(t.id);
        let status = self.display_task_status(t.id);
        let (surface, hover, border) = board_card_colors(status.phase, cx);
        let mut metadata = t.labels.iter().take(2).cloned().collect::<Vec<_>>();
        if !t.bucket.is_empty() {
            metadata.push(t.bucket.clone());
        }
        if let Some(cycle) = &t.cycle {
            metadata.push(format!("⟳ {cycle}"));
        }
        let id = t.id;
        let drag = DragTask {
            id,
            title: t.title.clone(),
        };
        col()
            .id(format!("card-{id}"))
            .p_3()
            .gap_2()
            .rounded(px(7.))
            .border_1()
            .border_color(rgb(border))
            .bg(rgb(surface))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(hover)))
            .on_click(cx.listener(move |this, _, w, cx| this.request_task(id, false, w, cx)))
            .when(!archived, |d| {
                d.on_drag(drag, |value, _, _, cx| cx.new(|_| value.clone()))
            })
            .child(
                row()
                    .items_start()
                    .gap_2()
                    .child(
                        row()
                            .flex_1()
                            .min_w_0()
                            .flex_wrap()
                            .gap_1()
                            .child(
                                div()
                                    .rounded(px(4.))
                                    .px_1()
                                    .py(px(2.))
                                    .bg(rgba(if dark { 0xffffff14 } else { 0xffffff99 }))
                                    .text_xs()
                                    .text_color(rgb(colors.text))
                                    .child(self.store.identifier(t)),
                            )
                            .when(
                                !matches!(self.scope(), Scope::Project(_) | Scope::Topic(_)),
                                |d| {
                                    d.child(
                                        div()
                                            .min_w_0()
                                            .rounded(px(4.))
                                            .px_1()
                                            .py(px(2.))
                                            .bg(rgba(if dark { 0xffffff14 } else { 0xffffff99 }))
                                            .text_xs()
                                            .text_color(rgb(colors.text))
                                            .child(
                                                self.store
                                                    .projects()
                                                    .iter()
                                                    .find(|p| p.id == t.project_id)
                                                    .map(|p| p.name.clone())
                                                    .unwrap_or_else(|| "Unassigned project".into()),
                                            ),
                                    )
                                },
                            ),
                    )
                    .child(self.execution_indicator(id, cx)),
            )
            .when(t.priority != Priority::Medium, |d| {
                d.child(label(format!("Priority: {}", t.priority.label()), cx).text_xs())
            })
            .child(
                div()
                    .line_height(relative(1.4))
                    .child(selectable(t.title.clone())),
            )
            .child(
                chip(status.label(), status.execution.color(cx), cx)
                    .id(format!("card-status-{id}"))
                    .aria_label(status.label())
                    .map(observed),
            )
            .child(label(status.reason.clone(), cx))
            .when_some(
                self.store
                    .research_requests()
                    .iter()
                    .find(|r| r.goal_ids.contains(&id))
                    .and_then(|r| r.brief.as_ref()),
                |d, brief| {
                    let summary: String = brief
                        .deliverable
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .chars()
                        .take(90)
                        .collect();
                    d.child(
                        label(
                            format!(
                                "Output · {summary}{}",
                                if brief.deliverable.chars().count() > 90 {
                                    "…"
                                } else {
                                    ""
                                }
                            ),
                            cx,
                        )
                        .line_clamp(3),
                    )
                },
            )
            .when(
                t.research
                    .as_ref()
                    .is_some_and(|r| r.kind == crate::research::ResearchKind::Goal),
                |d| {
                    d.child(
                        row_button(format!("goal-progress-{id}"), "")
                            .icon(assets::IconName::ArrowRight)
                            .h_auto()
                            .px_0()
                            .py_1()
                            .w_full()
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .text_xs()
                                    .whitespace_normal()
                                    .text_left()
                                    .child(format!(
                                        "Open {} steps",
                                        self.store.research_children(id).len()
                                    )),
                            )
                            .on_click(cx.listener(move |a, _, w, cx| {
                                cx.stop_propagation();
                                a.record_goal_origin(id);
                                a.open_goal_steps(id, w, cx);
                            })),
                    )
                },
            )
            .when(!metadata.is_empty(), |d| {
                d.child(
                    div()
                        .text_xs()
                        .line_height(relative(1.4))
                        .text_color(rgb(colors.secondary))
                        .child(selectable(metadata.join(" · "))),
                )
            })
            .child(
                row_button(format!("card-created-{id}"), "")
                    .self_start()
                    .h_5()
                    .px_0()
                    .py_0()
                    .text_xs()
                    .text_color(rgb(colors.secondary))
                    .tooltip(if t.created_at.is_empty() {
                        "Creation time was not recorded for this older task.".to_string()
                    } else {
                        format!("Created: {}", t.created_at)
                    })
                    .child(div().text_xs().child(format!(
                        "Created · {}",
                        if t.created_at.is_empty() {
                            "Not recorded".to_string()
                        } else {
                            t.created_at
                                .split_whitespace()
                                .take(2)
                                .collect::<Vec<_>>()
                                .join(" ")
                        }
                    ))),
            )
            .when(archived, |d| {
                d.child(
                    quiet_button(format!("restore-archived-{id}"), "Restore")
                        .icon(assets::IconName::RotateCw)
                        .tooltip("Restore this archived research and its retained results")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.restore_archived(id, cx);
                        })),
                )
            })
            .when(!archived && self.store.research_pending_approval(id), |d| {
                d.child(label("Awaiting approval", cx)).child(
                    quiet_button(
                        format!("approve-topic-{id}"),
                        if t.parent_id.is_some() {
                            "Approve parent goal"
                        } else {
                            "Approve research"
                        },
                    )
                    .icon(assets::IconName::ArrowRight)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.approve_research_topic(id, cx);
                    })),
                )
            })
            .when(
                !archived
                    && t.research.as_ref().is_some_and(|r| r.requires_approval)
                    && t.stage == Stage::Planned,
                |d| {
                    d.child(label(
                        if self.store.has_research_plan(id) {
                            "Approved · open to run research"
                        } else {
                            "Approved · open to generate research steps"
                        },
                        cx,
                    ))
                },
            )
            .map(observed)
            .map(|element| self.task_context(element, id, cx))
            .into_any_element()
    }
    fn task_context(&self, element: AnyElement, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity().downgrade();
        let archived = self.store.task_is_archived(id);
        let automation_owned = self
            .store
            .task(id)
            .is_some_and(|task| task.automation_id.is_some());
        let allowed_stages = Stage::ALL
            .into_iter()
            .filter(|stage| {
                self.store
                    .validate_manual_transition(id, *stage, self.todo_batch.as_ref())
                    .is_ok()
            })
            .collect::<Vec<_>>();
        div()
            .id(format!("task-context-{id}"))
            .child(element)
            .context_menu(move |mut menu, _, _| {
                let target = entity.clone();
                menu = menu.item(PopupMenuItem::new("Open task").on_click(move |_, w, cx| {
                    let _ = target.update(cx, |this, cx| this.request_task(id, false, w, cx));
                }));
                if !archived {
                    let target = entity.clone();
                    menu = menu.item(PopupMenuItem::new("Edit task").on_click(move |_, w, cx| {
                        let _ = target.update(cx, |this, cx| this.begin_draft(id, w, cx));
                    }));
                    menu = menu.separator();
                    for stage in allowed_stages.iter().copied() {
                        let target = entity.clone();
                        menu = menu.item(
                            PopupMenuItem::new(format!(
                                "{}{}",
                                if stage == Stage::InProgress {
                                    "Submit to "
                                } else {
                                    "Move to "
                                },
                                crate::workflow::phase_label(stage)
                            ))
                            .on_click(move |_, _, cx| {
                                let _ =
                                    target.update(cx, |this, cx| this.change_stage(id, stage, cx));
                            }),
                        );
                    }
                    let target = entity.clone();
                    if !automation_owned {
                        menu = menu
                            .separator()
                            .item(
                                PopupMenuItem::new("Archive task").on_click(move |_, w, cx| {
                                    let _ =
                                        target.update(cx, |this, cx| this.archive_task(id, w, cx));
                                }),
                            );
                    }
                }
                menu
            })
            .into_any_element()
    }
    fn detail(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let Some(t) = self.store.task(id) else {
            return col().child(selectable("Task not found")).into_any_element();
        };
        let t = t.clone();
        let is_goal = t
            .research
            .as_ref()
            .is_some_and(|r| r.kind == crate::research::ResearchKind::Goal);
        let version_history_count = self.previous_version_entries(id).len();
        let show_version_history = version_history_count > 0
            && (version_history_count > 1
                || !self.store.research_result_available_or_pending(id));
        let result_target = if self.store.task_is_archived(id) {
            crate::archive_presentation::result_target(&self.store, id, is_goal)
        } else if is_goal {
            self.store
                .research_children(id)
                .into_iter()
                .rev()
                .find(|child| self.store.research_result_available_or_pending(*child))
        } else {
            self.store
                .research_result_available_or_pending(id)
                .then_some(id)
        };
        let result_is_report = result_target
            .and_then(|target| self.store.task(target))
            .and_then(|task| task.research.as_ref())
            .is_some_and(|r| r.kind == crate::research::ResearchKind::Advise);
        let kind_label = t
            .research
            .as_ref()
            .map(|r| match r.kind {
                crate::research::ResearchKind::Goal => "Research goal",
                crate::research::ResearchKind::Collect => "Evidence",
                crate::research::ResearchKind::Analyze => "Analysis",
                crate::research::ResearchKind::Advise => "Final report",
            })
            .unwrap_or("Task");
        let navigation = self.store.task_navigation(id);
        let previous_enabled = navigation.previous.resolved(&self.store).is_some();
        let next_enabled = navigation.next.resolved(&self.store).is_some();
        let previous = navigation.previous.clone();
        let next = navigation.next.clone();
        let overview = navigation.overview.clone();
        let task_menu_entity = cx.entity().downgrade();
        let editing_disabled = self.store.task_is_archived(id) || self.task_running(id);
        let archived = self.store.task_is_archived(id);
        // Automation-owned steps belong to their automation; they must not be
        // archived individually from the Research task surface.
        let automation_owned = t.automation_id.is_some();
        let mut head = row()
            .min_h(px(44.))
            .py_2()
            .px_6()
            .flex_wrap()
            .border_b_1()
            .border_color(rgb(colors.line))
            .justify_between()
            .child(
                row()
                    .gap_3()
                    .flex_wrap()
                    .when(is_goal, |d| d.child(self.goal_tabs(id, cx)))
                    .when(is_goal, |d| {
                        d.child(
                            div()
                                .w(px(1.))
                                .h_5()
                                .flex_shrink_0()
                                .bg(rgb(colors.border_strong)),
                        )
                    })
                    .child(
                        div()
                            .id("task-kind-badge")
                            .flex_shrink_0()
                            .px_2()
                            .py_0p5()
                            .rounded(px(4.))
                            .bg(rgb(colors.tag_surface))
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(colors.tag_text))
                            .child(selectable(kind_label))
                            .map(observed),
                    ),
            );
        head = head.child(
            row()
                .flex_wrap()
                .when(is_goal && !archived, |d| d.child(self.goal_run_button(id, "goal-resume", cx).h(px(28.)).px_2().text_sm()))
                .when(is_goal && !archived, |d| d.child(button("goal-export", "Export ZIP").h(px(28.)).px_2().text_sm()
                    .disabled(!self.store.goal_tasks_complete(id) || self.goal_busy(id))
                    .tooltip("Export completed evidence, analysis and reports as a ZIP. All steps must have current results.")
                    .on_click(cx.listener(move |a, _, _, cx| a.export_research_goal(id, cx)))))
                .when(show_version_history, |d| d.child(
                    button("previous-versions-open", "Version history")
                        .h(px(28.)).px_2().text_sm()
                        .tooltip("View every retained version without resuming execution")
                        .on_click(cx.listener(move |a, _, w, cx| a.open_previous_versions(id, w, cx))),
                ))
                .when(archived, |d| d.child(
                    button(format!("restore-archived-{id}"), "Restore")
                        .icon(assets::IconName::RotateCw)
                        .h(px(28.)).px_2().text_sm()
                        .tooltip("Restore this archived research and its retained results")
                        .on_click(cx.listener(move |a, _, _, cx| a.restore_archived(id, cx))),
                ))
                .when_some(result_target, |d, target| {
                    d.child(
                        primary_button(
                            if is_goal {
                                "research-task-result"
                            } else {
                                "task-view-result"
                            },
                            if result_is_report {
                                "View final report"
                            } else {
                                "View latest result"
                            },
                        )
                        .h(px(28.)).px_2().text_sm()
                        .on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.record_origin();
                                this.view_task_result(target, cx)
                            }),
                        ),
                    )
                })
                .when(result_target.is_some() || is_goal, |d| d.child(
                    div().w(px(1.)).h_4().mx_2().flex_shrink_0().bg(rgb(colors.border_strong)),
                ))
                .when_some(overview, |d, overview| {
                    d.child(
                        quiet_button("period-overview", "Overview")
                            .h(px(28.)).px_2().text_sm()
                            .tooltip(overview.tooltip.clone())
                            .on_click(cx.listener(move |a, _, w, cx| {
                                a.open_task_neighbor(&overview, false, w, cx);
                            })),
                    )
                })
                .child(tool_icon("previous-task", assets::IconName::ArrowUp, "Previous task")
                    .h(px(28.)).w(px(28.))
                    .disabled(!previous_enabled)
                    .tooltip(previous.tooltip.clone())
                    .on_click(cx.listener(move |a, _, w, cx| {
                        a.open_task_neighbor(&previous, false, w, cx);
                    })))
                .child(tool_icon("next-task", assets::IconName::ArrowDown, "Next task")
                    .h(px(28.)).w(px(28.))
                    .disabled(!next_enabled)
                    .tooltip(next.tooltip.clone())
                    .on_click(cx.listener(move |a, _, w, cx| {
                        a.open_task_neighbor(&next, false, w, cx);
                    })))
                .child(div().w(px(1.)).h_4().mx_2().flex_shrink_0().bg(rgb(colors.border_strong)))
                .child(
                    tool_icon(
                        "task-actions-menu",
                        assets::IconName::Ellipsis,
                        "More task actions",
                    )
                    .h(px(28.)).w(px(28.))
                    .dropdown_menu_with_anchor(
                        Anchor::TopRight,
                        move |mut menu, _, _| {
                            let edit = task_menu_entity.clone();
                            let archive = task_menu_entity.clone();
                            menu = menu
                                .item(
                                    PopupMenuItem::new("Edit task")
                                        .disabled(editing_disabled)
                                        .on_click(move |_, w, cx| {
                                            let _ = edit.update(cx, |a, cx| {
                                                a.begin_draft(id, w, cx)
                                            });
                                        }),
                                );
                            if !automation_owned {
                                menu = menu.separator().item(
                                    PopupMenuItem::new(if archived {
                                        "Restore task"
                                    } else {
                                        "Archive task"
                                    })
                                    .on_click(move |_, w, cx| {
                                        let _ = archive.update(cx, |a, cx| {
                                            if a.store.task_is_archived(id) {
                                                a.restore_archived(id, cx);
                                            } else {
                                                a.archive_task(id, w, cx);
                                            }
                                        });
                                    }),
                                );
                            }
                            menu
                        },
                    ),
                ),
        );
        if is_goal && self.goal_steps_tab() == Some(id) {
            return col()
                .size_full()
                .child(head)
                .when_some(
                    self.task_start_feedback
                        .as_ref()
                        .filter(|(task, _)| *task == id),
                    |d, (_, message)| {
                        d.child(div().px_6().py_2().child(crate::views::notice(
                            crate::views::NoticeTone::Danger,
                            message.clone(),
                            cx,
                        )))
                    },
                )
                .child(self.goal_steps_view(id, cx))
                .into_any_element();
        }
        let mut children: Vec<_> = self
            .store
            .tasks()
            .iter()
            .filter(|c| c.parent_id == Some(id) && !c.archived)
            .cloned()
            .collect();
        children.sort_by(crate::display_order::siblings_in_display_order);
        let show_children = is_goal || t.research.is_none() || !children.is_empty();
        let mut body = col()
            .flex_1()
            .min_w_0()
            .gap_3()
            .px_6()
            .py_4()
            .when_some(t.parent_id, |d, parent| {
                d.child(label(
                    format!(
                        "Subtask of {}",
                        self.store
                            .task(parent)
                            .map(|p| p.title.clone())
                            .unwrap_or_default()
                    ),
                    cx,
                ))
            })
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(relative(1.3))
                    .child(selectable(t.title.clone())),
            )
            .child(
                div()
                    .text_color(rgb(colors.secondary))
                    .line_height(relative(1.7))
                    .child(selectable(if t.description.is_empty() {
                        "Add a description to give this task more context.".into()
                    } else {
                        t.description.clone()
                    })),
            )
            .when(is_goal, |d| d.child(self.request_brief_view(id, cx)))
            .child(self.materials_panel(id, cx))
            .when(t.research.is_some(), |d| {
                d.child(self.research_task_panel(id, cx))
            })
            .when(show_children, |d| {
                d.child(
                    row()
                        .id("task-children-heading")
                        .mt_4()
                        .justify_between()
                        .child(muted(
                            format!(
                                "{}  {}/{}",
                                if is_goal {
                                    "Research steps"
                                } else {
                                    "Subtasks"
                                },
                                children
                                    .iter()
                                    .filter(|t| self.display_task_status(t.id).phase == Stage::Done)
                                    .count(),
                                children.len()
                            ),
                            cx,
                        ))
                        .map(observed),
                )
            });
        let mut subtask_list = col().gap_1();
        for child in children {
            let cid = child.id;
            let child_archived = self.store.task_is_archived(cid);
            let (needs_refresh, refresh_label) = if child_archived {
                (
                    crate::archive_presentation::retained_result(&self.store, cid).is_some()
                        && crate::archive_presentation::validated_history_result(&self.store, cid)
                            .is_none(),
                    "Retained attempt · not validated",
                )
            } else {
                (
                    child.research.is_some()
                        && self
                            .store
                            .runs()
                            .iter()
                            .any(|r| r.task_id == cid && r.status == "Succeeded")
                        && self.store.research_result(cid).is_none(),
                    "Needs refresh",
                )
            };
            let title = format!("{} · {}", self.task_location_label(&child), child.title);
            let hint = self
                .store
                .step_block_reason(cid)
                .map(|reason| format!("{title}\n{reason}"))
                .unwrap_or_else(|| title.clone())
                .chars()
                .collect::<Vec<_>>()
                .chunks(36)
                .map(|chunk| chunk.iter().collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            subtask_list = subtask_list.child(
                row_button(format!("subtask-{cid}"), "")
                    .w_full()
                    .h_8()
                    .px_2()
                    .tooltip(hint)
                    .accessibility_label(title.clone())
                    .on_click(cx.listener(move |this, _, w, cx| this.open_task(cid, w, cx)))
                    .child(
                        row()
                            .w_full()
                            .min_w_0()
                            .child(task_state_icon(
                                &self.store,
                                &child,
                                self.todo_batch.as_ref(),
                                cx,
                            ))
                            .child(single_line_text(format!("subtask-title-{cid}"), title))
                            .when(needs_refresh, |d| {
                                d.child(
                                    chip(refresh_label, colors.accent, cx)
                                        .id(format!("subtask-refresh-{cid}"))
                                        .aria_label(refresh_label)
                                        .map(observed),
                                )
                            })
                            .child(
                                div().flex_shrink_0().child(
                                    row().gap_1().child(label("Open", cx)).child(
                                        gpui_kit::component::Icon::new(
                                            assets::IconName::ChevronRight,
                                        )
                                        .size_4(),
                                    ),
                                ),
                            ),
                    ),
            );
        }
        body = body.when(show_children, |d| d.child(subtask_list));
        let mut activity_panel = col().gap_2();
        if self.show_task_activity {
            for activity in t.activity.iter().rev().take(8) {
                activity_panel = activity_panel.child(
                    row()
                        .items_start()
                        .child(div().text_color(rgb(colors.muted)).child("·"))
                        .child(label(activity_display(activity, &t.created_at), cx)),
                );
            }
            for (i, comment) in t.comments.iter().enumerate() {
                activity_panel = activity_panel.child(
                    col()
                        .id(format!("comment-{i}"))
                        .p_3()
                        .gap_2()
                        .rounded_md()
                        .bg(rgb(colors.panel))
                        .child(
                            row()
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(selectable(comment.author.clone())),
                                )
                                .child(label(comment.created_at.clone(), cx)),
                        )
                        .child(selectable(comment.body.clone())),
                );
            }
            activity_panel = activity_panel
                .child(
                    div()
                        .id("comment-input")
                        .child(Textarea::new(&self.comment))
                        .map(observed),
                )
                .child(row().justify_end().child(
                    primary_button("post-comment", "Comment").on_click(cx.listener(
                        move |this, _, w, cx| {
                            let text = this.comment.read(cx).value().to_string();
                            if text.trim().is_empty() {
                                return;
                            }
                            this.request_task_comment(id, &text, w, cx);
                        },
                    )),
                ));
        }
        body = body.child(
            col()
                .border_t_1()
                .border_color(rgb(colors.line))
                .pt_2()
                .gap_2()
                .child(
                    row_button("task-activity-toggle", "Activity")
                        .self_start()
                        .px_0()
                        .text_color(rgb(colors.muted))
                        .icon(if self.show_task_activity {
                            assets::IconName::ChevronDown
                        } else {
                            assets::IconName::ChevronRight
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_task_activity = !this.show_task_activity;
                            cx.notify();
                        })),
                )
                .when(self.show_task_activity, |d| d.child(activity_panel)),
        );
        let mut properties = col()
            .w(px(240.))
            .flex_shrink_0()
            .p_3()
            .m_4()
            .ml_0()
            .gap_2()
            .child(self.step_execution_panel(id, cx));
        let archived = self.store.task_is_archived(id);
        let has_result = t
            .research
            .as_ref()
            .is_some_and(|r| r.kind != crate::research::ResearchKind::Goal)
            && if archived {
                crate::archive_presentation::validated_history_result(&self.store, id).is_some()
                    || crate::archive_presentation::pending_history_result(&self.store, id).is_some()
            } else {
                self.store.research_result_available_or_pending(id)
            };
        let retained_invalid = archived
            && !has_result
            && crate::archive_presentation::pending_history_result(&self.store, id).is_none()
            && crate::archive_presentation::retained_result(&self.store, id).is_some()
            && t.research
                .as_ref()
                .is_some_and(|r| r.kind != crate::research::ResearchKind::Goal);
        let previously_completed = t.stage == Stage::Done
            || self
                .store
                .runs()
                .iter()
                .any(|run| run.task_id == id && run.status == "Succeeded");
        let needs_refresh = !archived
            && previously_completed
            && !has_result
            && t.research
                .as_ref()
                .is_some_and(|r| r.kind != crate::research::ResearchKind::Goal);
        if needs_refresh && !self.task_running(id) {
            properties = properties.child(
                col().id("completed-update-needed").gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(colors.accent)).child(selectable("Completed previously · update needed")))
                    .child(div().text_xs().text_color(rgb(colors.muted))
                        .child(selectable("No reusable result is available for the current inputs. Finish any prerequisite updates before running again.")))
                    .map(observed),
            );
        }
        if retained_invalid {
            properties = properties.child(
                col().id("archive-retained-attempt-notice").gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(colors.muted)).child(selectable("Archived history · retained attempt not validated")))
                    .child(div().text_xs().text_color(rgb(colors.muted))
                        .child(selectable("This saved attempt remains readable but is not part of the validated completion. Restore the goal to review or run it again.")))
                    .map(observed),
            );
        }
        if self.task_running(id) {
            properties = properties.child(button("task-progress", "View progress").on_click(
                cx.listener(move |this, _, w, cx| {
                    if this
                        .store
                        .task(id)
                        .is_some_and(|task| task.automation_id.is_some())
                        && let Some(goal) = this.store.owning_research_goal(id)
                        && this.open_automation_progress_for_goal(goal, w, cx)
                    {
                        return;
                    }
                    if let Some(goal) = this.store.owning_research_goal(id) {
                        let layout = this.steps_layout();
                        this.open_goal_steps(goal, w, cx);
                        this.set_steps_layout(layout);
                    } else {
                        this.view_task_result(id, cx);
                    }
                }),
            ));
        }
        if let Some((task, message)) = &self.task_start_feedback {
            if *task == id {
                properties =
                    properties.child(
                        col()
                            .gap_1()
                            .p_2()
                            .border_l_2()
                            .border_color(rgb(colors.accent))
                            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(
                                selectable(if previously_completed {
                                    "Task could not run again"
                                } else {
                                    "Task could not start"
                                }),
                            ))
                            .child({
                                let related = self.store.tasks().iter().find_map(|task| {
                                    let name =
                                        format!("{} · {}", self.store.identifier(task), task.title);
                                    message.split_once(&name).map(|(before, after)| {
                                        (
                                            before.trim().to_string(),
                                            name.clone(),
                                            after.trim_start_matches('.').trim().to_string(),
                                        )
                                    })
                                });
                                if let Some((before, name, after)) = related {
                                    col()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(rgb(colors.muted))
                                                .child(selectable(before)),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgb(colors.accent))
                                                .child(selectable(name)),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(rgb(colors.muted))
                                                .child(selectable(after)),
                                        )
                                        .into_any_element()
                                } else {
                                    div()
                                        .text_sm()
                                        .child(selectable(message.clone()))
                                        .into_any_element()
                                }
                            }),
                    );
            }
        }
        let missing_folder = self
            .store
            .projects()
            .iter()
            .find(|p| p.id == t.project_id)
            .is_none_or(|p| p.directory.is_empty());
        let missing_model = t
            .research
            .as_ref()
            .is_some_and(|r| !self.store.valid_research_model(r.kind));
        if missing_folder && t.research.is_some() {
            properties = properties
                .child(label(
                    "Choose where this project's work will be saved before starting.",
                    cx,
                ))
                .child(
                    button("task-setup-folder", "Choose working folder…").on_click(
                        cx.listener(move |this, _, w, cx| this.open_directory(t.project_id, w, cx)),
                    ),
                );
        }
        if missing_model {
            properties = properties
                .child(label("Choose a model in Settings before starting.", cx))
                .child(
                    button("task-setup-model", "Open settings")
                        .on_click(cx.listener(|this, _, w, cx| this.open_model_settings(w, cx))),
                );
        }
        properties = properties.child(div().h_2()).child(
            div()
                .text_base()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(colors.text))
                .id("task-properties-heading")
                .child(selectable("Task"))
                .map(observed),
        );
        let status = self.display_task_status(id);
        let execution = status.execution;
        properties = properties.child(
            row()
                .justify_between()
                .w_full()
                .min_w_0()
                .child(label("Status", cx).id("task-status-label").flex_shrink_0().map(observed))
                // Keep the read-only value aligned with the other properties.
                // Longer status labels can wrap within the value column.
                .child(
                    div()
                        .id("task-execution-status")
                        .role(Role::Label)
                        .aria_label(status.label())
                        .max_w(px(164.))
                        .min_w_0()
                        .py_0p5()
                        .px_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_color(rgb(execution.color(cx)))
                        .tooltip({
                            let reason = status.reason.clone();
                            move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(reason.clone())
                                    .build(window, cx)
                            }
                        })
                        .child(execution.icon_view(format!("task-status-icon-{id}"), cx))
                        .child(
                            div()
                                .min_w_0()
                                .text_right()
                                .whitespace_normal()
                                .line_height(relative(1.3))
                                .child(selectable(status.label())),
                        )
                        .map(observed),
                ),
        );
        properties = properties.child(label(status.reason.clone(), cx));
        if t.parent_id.is_none() {
            properties = properties.child(
                row()
                    .justify_between()
                    .min_w_0()
                    .child(label("Topic", cx))
                    .child(self.topic_picker(Some(id), t.project_id, cx)),
            );
        }
        for (prop, name, value) in [
            (
                Property::Stage,
                "Stage",
                crate::workflow::phase_label(status.phase).into(),
            ),
            (Property::Priority, "Priority", t.priority.label().into()),
            (
                Property::Project,
                "Topic project",
                self.store
                    .projects()
                    .iter()
                    .find(|p| p.id == t.project_id)
                    .map(|p| p.name.clone())
                    .unwrap_or_default(),
            ),
            (
                Property::Cycle,
                "Cycle",
                t.cycle.clone().unwrap_or("No cycle".into()),
            ),
            (
                Property::Bucket,
                "Bucket",
                if t.bucket.is_empty() {
                    "No bucket".into()
                } else {
                    t.bucket.clone()
                },
            ),
            (
                Property::Parent,
                "Parent",
                t.parent_id
                    .and_then(|pid| self.store.task(pid))
                    .map(|p| self.store.identifier(p))
                    .unwrap_or("No parent".into()),
            ),
        ] {
            if t.parent_id.is_some() && t.research.is_some() && prop == Property::Stage {
                continue;
            }
            if !self.show_task_details
                && matches!(prop, Property::Cycle | Property::Bucket | Property::Parent)
            {
                continue;
            }
            properties = properties.child(
                row()
                    .flex_wrap()
                    .justify_between()
                    .child(label(if prop == Property::Stage {
                        if archived { "Stage" } else { "Move to" }
                    } else { name }, cx))
                    .child(
                        quiet_button(format!("property-{name}"), value.clone())
                            .px_2()
                            .h(px(28.))
                            .max_w(px(164.))
                            .min_w_0()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(match prop {
                                Property::Stage => stage_color(status.phase, cx),
                                Property::Priority if t.priority == Priority::Urgent => {
                                    colors.danger
                                }
                                Property::Priority if t.priority == Priority::High => {
                                    colors.warning
                                }
                                _ => colors.accent,
                            }))
                            .tooltip(if prop == Property::Stage && archived {
                                "Stage is read-only while archived. Restore this research to change it.".to_string()
                            } else if prop == Property::Stage {
                                format!("Workflow stage: {value}. In progress means research has started, not necessarily that a task is running. See Execution status for current activity.")
                            } else { format!("Change {}: {value}", name.to_lowercase()) })
                            .when(self.property == Some(prop), |b| {
                                b.bg(rgb(colors.accent_surface))
                            })
                            .when(prop == Property::Stage && archived, |b| {
                                b.text_color(rgb(colors.muted))
                            })
                            .dropdown_caret(!(prop == Property::Stage && archived))
                            .open(self.property == Some(prop))
                            .disabled(if prop == Property::Stage {
                                archived || self.task_running(id)
                            } else {
                                t.archived || self.task_running(id)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.property = if this.property == Some(prop) {
                                    None
                                } else {
                                    Some(prop)
                                };
                                cx.notify();
                            })),
                    ),
            );
            if self.property == Some(prop) {
                properties = properties.child(self.property_options(id, prop, cx));
            }
        }
        properties = properties.child(
            quiet_button(
                "task-details-toggle",
                if self.show_task_details {
                    "Hide details"
                } else {
                    "More details…"
                },
            )
            .self_start()
            .px_2()
            .icon(if self.show_task_details {
                assets::IconName::ChevronUp
            } else {
                assets::IconName::ChevronDown
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.show_task_details = !this.show_task_details;
                this.property = None;
                cx.notify();
            })),
        );
        if self.show_task_details {
            properties = properties
                .child(div().h_2().border_b_1().border_color(rgb(colors.line)))
                .child(
                    row()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(selectable("Labels")),
                        )
                        .child(
                            tool_icon("edit-labels", assets::IconName::Pencil, "Edit labels")
                                .disabled(t.archived || self.task_running(id))
                                .on_click(
                                    cx.listener(move |this, _, w, cx| this.begin_draft(id, w, cx)),
                                ),
                        ),
                )
                .child(
                    row()
                        .flex_wrap()
                        .gap_1()
                        .children(t.labels.iter().map(|name| {
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_sm()
                                .bg(rgb(colors.tag_surface))
                                .text_xs()
                                .text_color(rgb(colors.tag_text))
                                .child(selectable(name.clone()))
                        }))
                        .when(t.labels.is_empty(), |d| d.child(label("No labels", cx))),
                )
                .child(
                    div()
                        .pt_2()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(selectable("Execution settings")),
                );
            let execution_field = |name: &str, value: String| {
                row()
                    .items_start()
                    .min_w_0()
                    .child(
                        div()
                            .w(px(88.))
                            .flex_shrink_0()
                            .child(label(name.to_string(), cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(rgb(colors.text))
                            .whitespace_normal()
                            .overflow_hidden()
                            .child(selectable(value)),
                    )
            };
            let model_fields = |model: &str| {
                let effective = self.store.effective_model(model);
                col()
                    .gap_2()
                    .child(execution_field(
                        "Model source",
                        if model.is_empty() {
                            "Global default"
                        } else {
                            "Stage override"
                        }
                        .into(),
                    ))
                    .child(execution_field(
                        "Model",
                        if effective.is_empty() {
                            "Not configured".into()
                        } else {
                            effective.to_string()
                        },
                    ))
            };
            if let Some(r) = &t.research {
                properties = properties
                    .child(model_fields(
                        &self.store.research_config().models[r.kind.index()],
                    ))
                    .child(execution_field(
                        "Browser space",
                        self.store
                            .research_space(id)
                            .unwrap_or_else(|| r.space_name.clone()),
                    ))
                    .child(button("configure-research", "Research settings…").on_click(
                        cx.listener(|this, _, w, cx| {
                            this.open_model_settings(w, cx);
                        }),
                    ));
            }
            properties = properties.child(button("task-folder", "Working folder…").on_click(
                cx.listener(move |this, _, w, cx| this.open_directory(t.project_id, w, cx)),
            ));
        }
        col()
            .size_full()
            .child(head)
            .child(
                row()
                    .id("detail-scroll")
                    .track_scroll(&self.detail_scroll)
                    .items_start()
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h_0()
                    .child(body)
                    .child(properties.id("task-properties").map(observed))
                    .map(observed),
            )
            .into_any_element()
    }
    fn property_options(&self, id: u64, property: Property, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let mut options = col()
            .p_1()
            .gap_0()
            .bg(rgb(colors.panel))
            .border_1()
            .border_color(rgb(colors.line))
            .rounded_md();
        match property {
            Property::Stage => {
                for stage in Stage::ALL {
                    let allowed =
                        self.store
                            .validate_manual_transition(id, stage, self.todo_batch.as_ref());
                    options = options.child(
                        property_choice(
                            format!("set-{}", stage.key()),
                            if stage == Stage::InProgress {
                                "Submit execution"
                            } else {
                                crate::workflow::phase_label(stage)
                            },
                            Some(crate::execution_status::ExecutionStatus::Stage(stage).icon()),
                            self.store.task(id).is_some_and(|t| t.stage == stage),
                        )
                        .w_full()
                        .justify_start()
                        .disabled(allowed.is_err())
                        .tooltip(allowed.err().unwrap_or_else(|| "Move to this phase".into()))
                        .when(self.store.task(id).is_some_and(|t| t.stage == stage), |b| {
                            b.bg(rgb(colors.accent_surface))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.change_stage(id, stage, cx);
                            this.property = None;
                        })),
                    );
                }
            }
            Property::Priority => {
                for priority in Priority::ALL {
                    options = options.child(
                        property_choice(
                            format!("priority-{}", priority.key()),
                            priority.label(),
                            None,
                            self.store.task(id).is_some_and(|t| t.priority == priority),
                        )
                        .w_full()
                        .justify_start()
                        .when(
                            self.store.task(id).is_some_and(|t| t.priority == priority),
                            |b| b.bg(rgb(colors.accent_surface)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.apply_task_metadata(
                                id,
                                crate::scoped_edit::TaskEditField::Priority,
                                crate::scoped_edit::TaskEditValue::Priority(priority),
                                cx,
                            );
                            this.property = None;
                            cx.notify();
                        })),
                    );
                }
            }
            Property::Project => {
                options = options.child(button("move-entire-topic", "Move topic…").on_click(
                    cx.listener(move |this, _, w, cx| {
                        if let Some(topic) = this.store.task_topic(id).map(|t| t.id) {
                            this.request_topic_move(topic, w, cx);
                        }
                    }),
                ));
            }
            Property::Cycle => {
                let cycles: std::collections::BTreeSet<_> = self
                    .store
                    .tasks()
                    .iter()
                    .filter_map(|t| t.cycle.clone())
                    .collect();
                for value in std::iter::once("No cycle".to_string()).chain(cycles) {
                    options = options.child(
                        property_choice(
                            format!("set-cycle-{value}"),
                            value.clone(),
                            None,
                            self.store
                                .task(id)
                                .is_some_and(|t| t.cycle.as_deref().unwrap_or("No cycle") == value),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.apply_task_metadata(
                                id,
                                crate::scoped_edit::TaskEditField::Cycle,
                                crate::scoped_edit::TaskEditValue::OptionalText(
                                    if value == "No cycle" {
                                        None
                                    } else {
                                        Some(value.clone())
                                    },
                                ),
                                cx,
                            );
                            this.property = None;
                            cx.notify();
                        })),
                    );
                }
            }
            Property::Bucket => {
                for value in ["No bucket", "Now", "Next", "Later"] {
                    options = options.child(
                        property_choice(
                            format!("set-bucket-{value}"),
                            value,
                            None,
                            self.store.task(id).is_some_and(|t| {
                                if t.bucket.is_empty() {
                                    value == "No bucket"
                                } else {
                                    t.bucket == value
                                }
                            }),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.apply_task_metadata(
                                id,
                                crate::scoped_edit::TaskEditField::Bucket,
                                crate::scoped_edit::TaskEditValue::Text(if value == "No bucket" {
                                    String::new()
                                } else {
                                    value.to_string()
                                }),
                                cx,
                            );
                            this.property = None;
                            cx.notify();
                        })),
                    );
                }
            }
            Property::Parent => {
                options = options.child(
                    property_choice(
                        "no-parent",
                        "No parent",
                        None,
                        self.store.task(id).is_some_and(|t| t.parent_id.is_none()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_domain_command_undoable(
                            crate::scoped_domain::DomainCommand::TaskParent { task: id, parent: None },
                            "Parent updated",
                            cx,
                        );
                        this.property = None;
                    })),
                );
                for task in self.store.tasks().iter().filter(|t| {
                    t.id != id
                        && !t.archived
                        && t.parent_id.is_none()
                        && self
                            .store
                            .task(id)
                            .is_some_and(|current| current.project_id == t.project_id)
                }) {
                    let parent = task.id;
                    options = options.child(
                        property_choice(
                            format!("set-parent-{parent}"),
                            task.title.clone(),
                            None,
                            self.store
                                .task(id)
                                .is_some_and(|t| t.parent_id == Some(parent)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.apply_domain_command_undoable(
                                crate::scoped_domain::DomainCommand::TaskParent {
                                    task: id,
                                    parent: Some(parent),
                                },
                                "Parent updated",
                                cx,
                            );
                            this.property = None;
                        })),
                    );
                }
            }
        }
        options.into_any_element()
    }
    fn directory_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let name = self
            .store
            .projects()
            .iter()
            .find(|p| Some(p.id) == self.directory_project)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x000000a0))
            .flex()
            .items_center()
            .justify_center()
            .child(
                col()
                    .w(px(620.))
                    .p_6()
                    .gap_4()
                    .bg(rgb(colors.panel))
                    .rounded_lg()
                    .child(
                        div()
                            .text_lg()
                            .child(selectable(format!("Working folder · {name}"))),
                    )
                    .child(muted(
                        "OpenCode will read and modify files in this folder when you run a task.",
                        cx,
                    ))
                    .child(Input::new(&self.directory_input).id("directory-path"))
                    .when(!self.error.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(rgb(colors.danger))
                                .child(self.error.clone()),
                        )
                    })
                    .child(
                        row()
                            .justify_end()
                            .child(button("cancel-directory", "Cancel").on_click(cx.listener(
                                |this, _, w, cx| {
                                    this.directory_project = None;
                                    this.error.clear();
                                    this.save_feedback = None;
                                    w.focus(&this.focus, cx);
                                    cx.notify();
                                },
                            )))
                            .child(
                                primary_button("save-directory", "Save folder").on_click(
                                    cx.listener(|this, _, _, cx| this.save_directory(cx)),
                                ),
                            ),
                    ),
            )
            .focus_trap("directory-focus", &self.modal_focus)
            .into_any_element()
    }
    fn editor_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let Some(d) = &self.draft else {
            return div().into_any_element();
        };
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x000000a0))
            .flex()
            .items_center()
            .justify_center()
            .child(
                col()
                    .w(px(680.))
                    .max_h(relative(0.92))
                    .id("draft-scroll")
                    .overflow_y_scroll()
                    .p_6()
                    .gap_4()
                    .rounded(px(12.))
                    .bg(rgb(colors.panel))
                    .border_1()
                    .border_color(rgb(colors.border_strong))
                    .child(
                        row()
                            .justify_between()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Edit task"),
                            )
                            .child(
                                tool_icon(
                                    "cancel-draft",
                                    assets::IconName::Close,
                                    "Close without saving",
                                )
                                .tooltip("Close without saving")
                                .on_click(cx.listener(
                                    |this, _, w, cx| {
                                        this.request_cancel_task_edit(w, cx);
                                    },
                                )),
                            ),
                    )
                    .when_some(
                        crate::research_input_views::edit_impact_note(self, d.id, cx),
                        |v, note| v.child(note),
                    )
                    .child(label("TITLE", cx))
                    .child(Input::new(&d.title).id("draft-title"))
                    .child(label("DESCRIPTION", cx))
                    .child(
                        div()
                            .id("draft-description")
                            .child(Textarea::new(&d.description)),
                    )
                    .child(label("WORKFLOW STAGE", cx))
                    .child(
                        row()
                            .gap_3()
                            .flex_wrap()
                            .children(Stage::ALL.into_iter().map(|stage| {
                                select_button(
                                    format!("draft-stage-{}", stage.key()),
                                    crate::workflow::phase_label(stage),
                                    d.stage == stage,
                                    cx,
                                )
                                .disabled(
                                    self.store.task(d.id).is_none_or(|task| task.stage != stage)
                                        && (stage == Stage::InProgress
                                            || self
                                                .store
                                                .validate_manual_transition(
                                                    d.id,
                                                    stage,
                                                    self.todo_batch.as_ref(),
                                                )
                                                .is_err()),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if let Some(d) = &mut this.draft {
                                            d.stage = stage;
                                        }
                                        cx.notify();
                                    },
                                ))
                            })),
                    )
                    .child(
                        row()
                            .gap_3()
                            .flex_wrap()
                            .children(self.store.visible_projects().map(|p| {
                                let id = p.id;
                                select_button(
                                    format!("draft-project-{id}"),
                                    p.name.clone(),
                                    d.project == id,
                                    cx,
                                )
                                .disabled(
                                    self.store
                                        .task(d.id)
                                        .is_some_and(|task| task.parent_id.is_some()),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if let Some(d) = &mut this.draft {
                                            d.project = id;
                                        }
                                        cx.notify();
                                    },
                                ))
                            })),
                    )
                    .child(
                        row().items_start().gap_4().child(
                            col()
                                .flex_1()
                                .gap_2()
                                .child(label("LABELS, COMMA SEPARATED", cx))
                                .child(Input::new(&d.labels).id("draft-labels")),
                        ),
                    )
                    .when(!self.error.is_empty(), |v| {
                        v.child(
                            div()
                                .text_color(rgb(colors.danger))
                                .child(self.error.clone()),
                        )
                    })
                    .child(
                        row()
                            .justify_end()
                            .gap_3()
                            .child(button("discard-draft", "Cancel").on_click(cx.listener(
                                |this, _, w, cx| {
                                    this.draft = None;
                                    let _ = this.close_task_editor_overlay();
                                    this.error.clear();
                                    this.save_feedback = None;
                                    w.focus(&this.focus, cx);
                                    cx.notify();
                                },
                            )))
                            .child(primary_button("save-task", "Save changes").on_click(
                                cx.listener(|this, _, w, cx| {
                                    let _ = this.request_save_draft(w, cx);
                                }),
                            )),
                    )
                    .map(observed),
            )
            .focus_trap("draft-focus", &self.modal_focus)
            .into_any_element()
    }
}
impl TaskApp {
    fn dirty_exit_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(pending) = &self.navigation.pending_exit else {
            return div().into_any_element();
        };
        let colors = palette(cx);
        let saving = self.navigation.pending_saved_exit.is_some();
        let mut actions = row().flex_wrap().justify_end().gap_2();
        actions = actions
            .child(
                button("dirty-exit-stay", crate::draft_exit::stay_action_label())
                    .disabled(saving)
                    .on_click(cx.listener(|a, _, w, cx| {
                        a.decide_exit(crate::draft_exit::ExitDecision::Stay, w, cx)
                    })),
            )
            .child(
                button(
                    "dirty-exit-discard",
                    crate::draft_exit::discard_action_label(),
                )
                .disabled(saving)
                .on_click(cx.listener(|a, _, w, cx| {
                    a.decide_exit(crate::draft_exit::ExitDecision::Discard, w, cx)
                })),
            )
            .child(
                primary_button(
                    "dirty-exit-save",
                    crate::draft_exit::save_action_label(pending.surface),
                )
                .disabled(saving)
                .on_click(cx.listener(|a, _, w, cx| {
                    a.decide_exit(crate::draft_exit::ExitDecision::Save, w, cx)
                })),
            );
        let mut body = col()
            .w(px(500.))
            .max_w_full()
            .max_h(relative(0.9))
            .p_6()
            .gap_4()
            .rounded_lg()
            .bg(rgb(colors.panel));
        body = body
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(selectable(crate::draft_exit::decision_title(
                        pending.surface,
                    ))),
            )
            .child(muted(
                crate::draft_exit::decision_explanation(pending.surface),
                cx,
            ));
        if pending.surface == crate::draft_exit::DecisionSurface::PlanningRequest {
            body = body.child(muted(
                "Saving stores the request only. Generating a plan and running research are separate actions.",
                cx,
            ));
        }
        if let Some(error) = &pending.error {
            body = body.child(crate::views::notice(
                crate::views::NoticeTone::Danger,
                error.clone(),
                cx,
            ));
        }
        if let Some(notice) = &pending.notice {
            body = body.child(crate::views::notice(
                crate::views::NoticeTone::Info,
                notice.clone(),
                cx,
            ));
        }
        if self.research_save_impact.is_some() {
            body = body.child(crate::views::notice(
                crate::views::NoticeTone::Info,
                "The save impact review is open. Review it first; leaving waits for your confirmation.",
                cx,
            ));
        }
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x00000080))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(body.child(actions))
            .focus_trap("dirty-exit-focus", &self.modal_focus)
            .into_any_element()
    }
}

#[cfg(feature = "qa")]
struct LiveRenderGuard(std::time::Instant, usize);
#[cfg(feature = "qa")]
impl Drop for LiveRenderGuard {
    fn drop(&mut self) {
        let elapsed = self.0.elapsed().as_micros();
        if self.1 > 10_000 {
            eprintln!("LIVE_DIAG render_us={elapsed} runs={}", self.1);
        }
    }
}

impl Render for TaskApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "qa")]
        let live_render_started = std::time::Instant::now();
        #[cfg(feature = "qa")]
        let _live_render_guard = LiveRenderGuard(live_render_started, self.store.runs().len());
        #[cfg(feature = "qa")]
        crate::brand::note_app_render();
        // A render is the central wake point for durable work scheduled by
        // context-free legacy callers (mark-read, follow-up saves, dispatch).
        // Only wake while progress is still possible; a parked/failed worker
        // waits for an explicit retry instead of looping on every frame.
        let live_wake = self.live_needs_pump()
            && self.live_failure.is_none()
            && self.storage_error.is_empty();
        if self.history_needs_pump()
            || live_wake
            || self.dispatch_pump_pending()
            || self.pending_material_job.is_some()
        {
            self.start_pump(cx);
        }
        // Window-dependent post-ACK work (modal dismissal/focus restore) runs
        // here because the pump has no Window.
        self.apply_deferred_ui(window, cx);
        self.apply_pending_result_navigation(window, cx);
        if self.navigation.pending_saved_exit.is_some() {
            let weak = cx.weak_entity();
            window.defer(cx, move |window, cx| {
                let _ = weak.update(cx, |app, cx| {
                    app.poll_pending_saved_exit(window, cx);
                });
            });
        }
        if !self.window_close_registered {
            self.window_close_registered = true;
            let weak = cx.weak_entity();
            window.on_window_should_close(cx, move |_window, cx| {
                weak.update(cx, |app, cx| app.can_close_window(cx))
                    .unwrap_or(true)
            });
        }
        if self.window_close_requested && !self.quit_requested && self.live_clean_for_quit() {
            self.window_close_requested = false;
            self.window_close_ready = true;
        }
        if self.window_close_ready {
            self.window_close_ready = false;
            window.remove_window();
        }
        // Queue changes notify the app and invalidate statuses through the observer.
        // Merely having a batch must not invalidate every wheel/animation frame.
        let status_epoch = self.store.dirty_epoch();
        if self.render_status_epoch.replace(status_epoch) != status_epoch {
            self.render_statuses.borrow_mut().clear();
            self.render_executions.borrow_mut().clear();
        }
        if let Some(id) = self.sidebar_reveal.take() {
            // Deferred effects run after this frame has laid out expanded ancestors.
            let entity = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = entity.update(cx, |a, cx| {
                    if a.selected().or(a.task_result_context()) != Some(id) {
                        return;
                    }
                    let Some(bounds) = a.sidebar_selected_row.bounds_for_item(0) else {
                        return;
                    };
                    let viewport = a.sidebar_scroll.bounds();
                    let delta = if bounds.top() < viewport.top() {
                        viewport.top() - bounds.top()
                    } else if bounds.bottom() > viewport.bottom() {
                        viewport.bottom() - bounds.bottom()
                    } else {
                        px(0.)
                    };
                    if delta != px(0.) {
                        let offset = a.sidebar_scroll.offset();
                        a.sidebar_scroll
                            .set_offset(point(offset.x, offset.y + delta));
                        cx.notify();
                    }
                });
            });
        }
        let colors = palette(cx);
        if self.workspace_required {
            if let Some(path) = self.workspace_choice.take() {
                self.directory_input.update(cx, |input, cx| {
                    input.set_value(path.to_string_lossy().into_owned(), window, cx)
                });
            }
            if let Some(result) = self
                .workspace_setup
                .as_ref()
                .and_then(|rx| match rx.try_recv() {
                    Ok(result) => Some(result),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(
                        "Workspace setup stopped unexpectedly. Please retry.".into(),
                    )),
                })
            {
                self.workspace_setup = None;
                match result {
                    Ok(root) => {
                        self.workspace_root = Some(root.clone());
                        self.begin_workspace_open(
                            crate::persistence::Storage::new(
                                root.join("workspace/workspace.sqlite3"),
                            ),
                            cx,
                        );
                    }
                    Err(error) => self.storage_error = error,
                }
            }
        }
        self.poll_workspace_open(window, cx);
        if self.workspace_required || self.startup.required {
            if cfg!(target_os = "macos") {
                return col()
                    .size_full()
                    .bg(rgb(colors.canvas))
                    .child(
                        gpui_kit::component::TitleBar::new()
                            .h(px(44.))
                            .border_b_0()
                            .bg(rgb(colors.canvas)),
                    )
                    .child(div().flex_1().min_h_0().child(self.startup_view(cx)))
                    .into_any_element();
            }
            return self.startup_view(cx);
        }
        let content = self.retained_content(cx);

        let mut root = col()
            .id("app")
            .tab_group()
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .bg(rgb(colors.canvas))
            .text_color(rgb(colors.text))
            .font_family(".AppleSystemUIFont")
            .text_sm()
            // Plain-text forms use Tab for navigation, not textarea indentation.
            // Dispatch the base actions to preserve modal focus containment.
            .capture_action(|_: &IndentInline, w, cx| {
                let action = cx
                    .build_action("root::Tab", None)
                    .expect("base Tab action registered");
                w.dispatch_action(action, cx);
                cx.stop_propagation();
            })
            .capture_action(|_: &OutdentInline, w, cx| {
                let action = cx
                    .build_action("root::TabPrev", None)
                    .expect("base TabPrev action registered");
                w.dispatch_action(action, cx);
                cx.stop_propagation();
            })
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, w, cx| {
                if event.keystroke.modifiers.platform && event.keystroke.key == "k" {
                    this.focus_search(w, cx);
                } else if this.global_search_ui.open {
                    // The overlay is modal: its own capture handler owns
                    // Up/Down/Enter/Escape, and background shortcuts must not
                    // navigate underneath it. Plain text still reaches the input.
                    if event.keystroke.key == "escape" {
                        this.close_global_search(w, cx);
                        cx.stop_propagation();
                    } else if event.keystroke.modifiers.platform {
                        cx.stop_propagation();
                    }
                } else if event.keystroke.modifiers.platform
                    && event.keystroke.key == "["
                    && this
                        .dirty_draft(cx)
                        .and_then(crate::draft_exit::decision_surface)
                        .is_some()
                {
                    // A dirty surface owns the decision even though the plain
                    // Back shortcut is otherwise suppressed while editing.
                    this.request_back(w, cx);
                    cx.stop_propagation();
                } else if event.keystroke.modifiers.platform
                    && event.keystroke.key == "["
                    && this.draft.is_none()
                    && this.new_project.is_none()
                    && this.topic_editor.is_none()
                    && this.project_rename.is_none()
                    && this.topic_move.is_none()
                    && this.planning_project.is_none()
                    && this.planning_destination.is_none()
                    && this.planning_destination_error.is_none()
                    && this.directory_project.is_none()
                    && !this.settings_overlay()
                    && this.period_overlay().is_none()
                {
                    this.request_back(w, cx);
                    cx.stop_propagation();
                } else if event.keystroke.modifiers.platform && event.keystroke.key == "n" {
                    this.request_new_research(w, cx);
                } else if event.keystroke.key == "escape" {
                    if this.request_escape(w, cx) {
                        cx.stop_propagation();
                        return;
                    }
                    let planning_opener = this.planning_opener_focus.take();
                    this.save_feedback = None;
                    this.draft = None;
                    let _ = this.close_task_editor_overlay();
                    this.property = None;
                    this.new_project = None;
                    this.topic_editor = None;
                    this.project_rename = None;
                    this.topic_move = None;
                    this.planning_project = None;
                    this.planning_topic = None;
                    this.planning_replan_source = None;
                    if this.planning_destination.is_some() {
                        this.bump_planning_form_epoch();
                    }
                    this.planning_destination = None;
                    this.planning_destination_error = None;
                    this.directory_project = None;
                    this.clear_typed_overlays();
                    this.error.clear();
                    if let Some(opener) = planning_opener {
                        w.focus(&opener, cx);
                    } else {
                        w.focus(&this.focus, cx);
                    }
                    cx.notify();
                }
            }))
            .child(
                div().flex_1().min_h_0().child(
                    h_resizable("workspace-split")
                        .with_state(&self.sidebar_split)
                        .child(
                            resizable_panel()
                                .size(px(260.))
                                .size_range(px(220.)..px(480.))
                                .fixed()
                                .child(self.retained_sidebar(cx)),
                        )
                        .child(
                            resizable_panel().size_range(px(480.)..Pixels::MAX).child(
                                col()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .child(self.navigation_header(window, cx))
                                    .when(self.save_feedback.is_some(), |d| {
                                        d.child(self.save_feedback_view(cx))
                                    })
                                    .child(div().flex_1().min_h_0().child(content)),
                            ),
                        ),
                ),
            )
            .child(self.retained_footer(cx));
        if self.editing() {
            root = root.child(self.editor_overlay(cx));
        }
        if self.research_save_impact.is_some() {
            root = root.child(crate::research_input_views::save_impact_dialog(self, cx));
        }
        if self.previous_versions_task.is_some() {
            root = root.child(crate::research_input_views::previous_versions_dialog(
                self, cx,
            ));
        }
        if self.planning_destination.is_some() || self.planning_destination_error.is_some() {
            root = root.child(self.project_plan_overlay(cx));
        }
        if self.directory_project.is_some() {
            root = root.child(self.directory_overlay(cx));
        }
        if self.topic_move.is_some() {
            root = root.child(self.topic_move_view(cx));
        }
        if self.project_rename.is_some() {
            root = root.child(self.project_rename_view(cx));
        }
        if self.topic_editor.is_some() {
            root = root.child(self.topic_editor_view(cx));
        }
        if let Some(input) = &self.new_project {
            let input = input.clone();
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .bg(rgba(0x000000a0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        col()
                            .w(px(420.))
                            .p_6()
                            .gap_4()
                            .bg(rgb(colors.panel))
                            .rounded_lg()
                            .child(div().text_lg().child(selectable("New project")))
                            .child(Input::new(&input).id("project-name"))
                            .child(muted("Create a project to organize topics. You can add research requests later.", cx))
                            .when(!self.error.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_color(rgb(colors.danger))
                                        .child(self.error.clone()),
                                )
                            })
                            .child(
                                row()
                                    .justify_end()
                                    .child(button("cancel-project", "Cancel").on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.new_project = None;
                                            cx.notify();
                                        }),
                                    ))
                                    .child(
                                        primary_button("create-project", "Create project")
                                            .on_click(cx.listener(move |this, _, w, cx| {
                                                let name = input.read(cx).value().to_string();
                                                if name.trim().is_empty() {
                                                    this.error =
                                                        "A project name is required".into();
                                                    cx.notify();
                                                    return;
                                                }
                                                let id = this
                                                    .store
                                                    .projects()
                                                    .iter()
                                                    .map(|p| p.id)
                                                    .max()
                                                    .unwrap_or(0)
                                                    + 1;
                                                let directory = match this.managed_project_directory(id) {
                                                    Ok(path) => path,
                                                    Err(error) => { this.error = error; cx.notify(); return; }
                                                };
                                                let name = name.trim().to_string();
                                                if let Some(outcome) = this.apply_domain_command_undoable(
                                                    crate::scoped_domain::DomainCommand::ProjectCreate {
                                                        name,
                                                        directory,
                                                    },
                                                    "Project created · add a topic or start a research request",
                                                    cx,
                                                ) {
                                                    let id = outcome.created_project.unwrap_or(id);
                                                    this.new_project = None;
                                                    this.navigate(Scope::Project(id), w, cx);
                                                }
                                            })),
                                    ),
                            ),
                    )
                    .focus_trap("project-focus", &self.modal_focus),
            );
        }
        if self.settings_overlay() {
            root = root.child(self.automation_settings_dialog(cx));
        }
        if self.period_overlay().is_some() {
            root = root.child(self.retained_automation_result(cx));
        } else {
            self.automation_result_surface = None;
        }
        if self.planning_attempt_overlay().is_some() {
            root = root.child(self.planning_attempt_dialog(cx));
        }
        // Global search is the topmost surface. Opening it from a dialog must not
        // dismiss that dialog; Escape closes only the search overlay.
        if self.global_search_ui.open {
            root = root.child(self.global_search_overlay(cx));
        }
        if self.navigation.pending_exit.is_some() && self.research_save_impact.is_none() {
            root = root.child(self.dirty_exit_dialog(cx));
        }
        root.into_any_element()
    }
}
pub(crate) fn stage_color_for_definition(
    status: crate::automation::DefinitionStatus,
    cx: &App,
) -> u32 {
    let colors = palette(cx);
    match status {
        crate::automation::DefinitionStatus::Enabled => colors.success,
        crate::automation::DefinitionStatus::Paused => colors.warning,
        crate::automation::DefinitionStatus::Draft => colors.secondary,
        crate::automation::DefinitionStatus::NeedsAttention => colors.danger,
        crate::automation::DefinitionStatus::Archived => colors.muted,
    }
}

fn priority_icon(p: Priority) -> assets::IconName {
    match p {
        Priority::None => assets::IconName::Minus,
        Priority::Low => assets::IconName::ArrowDown,
        Priority::Medium => assets::IconName::Equal,
        Priority::High => assets::IconName::ArrowUp,
        Priority::Urgent => assets::IconName::CircleAlert,
    }
}
fn append_children(tasks: &[Task], id: u64, collapsed: &[u64], out: &mut Vec<Task>) {
    for t in tasks.iter().filter(|t| t.parent_id == Some(id)) {
        out.push(t.clone());
        if !collapsed.contains(&t.id) {
            append_children(tasks, t.id, collapsed, out);
        }
    }
}

pub(crate) fn run_color(status: &str, cx: &App) -> u32 {
    let colors = palette(cx);
    match status {
        "Running" | "Cancelling" | "Recovering" => colors.warning,
        "Succeeded" => colors.success,
        "Failed" | "Interrupted" | "Blocked" | "Stale" => colors.danger,
        _ => colors.muted,
    }
}
