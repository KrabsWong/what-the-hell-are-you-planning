//! Shared action affordances. Keep geometry here and state colors in appearance.rs.
use gpui_kit::{
    component::{
        Selectable,
        button::{Button, ButtonVariants},
    },
    prelude::FluentBuilder,
    *,
};

pub fn button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Button {
    Button::new(id)
        .label(text)
        .h(px(32.))
        .px_3()
        .rounded(px(6.))
}
/// Auxiliary actions stay visible without competing with the page's main action.
pub fn quiet_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Button {
    button(id, text).ghost()
}
pub fn tool_icon(
    id: impl Into<ElementId>,
    icon: assets::IconName,
    name: impl Into<SharedString>,
) -> Button {
    let name = name.into();
    Button::new(id)
        .ghost()
        .h(px(32.))
        .rounded(px(6.))
        .icon(icon)
        .w(px(32.))
        .px_0()
        .tooltip(name.clone())
        .accessibility_label(name)
}
pub fn primary_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Button {
    button(id, text).primary().font_weight(FontWeight::MEDIUM)
}
pub fn link_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Button {
    button(id, text).link().px_0().self_start().max_w_full()
}
pub fn icon_button(
    id: impl Into<ElementId>,
    glyph: impl Into<SharedString>,
    name: impl Into<SharedString>,
) -> Button {
    let name = name.into();
    button(id, glyph)
        .w(px(32.))
        .px_0()
        .tooltip(name.clone())
        .accessibility_label(name)
}
pub fn select_button(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    selected: bool,
) -> Button {
    button(id, text)
        .when(selected, |b| b.primary())
        .selected(selected)
        .toggled(selected)
}

// Measure shaped glyphs, including CJK fallback fonts, before adding an ellipsis.
pub fn single_line_text(id: impl Into<ElementId>, title: impl Into<String>) -> Stateful<Div> {
    let title = title.into();
    div()
        .h_5()
        .flex_1()
        .min_w_0()
        .id(id)
        .role(Role::Label)
        .aria_label(title.clone())
        .overflow_hidden()
        .child(
            canvas(
                {
                    let title: SharedString = title.replace(['\r', '\n'], " ").into();
                    move |bounds, window, _| {
                        let style = window.text_style();
                        let font_size = style.font_size.to_pixels(window.rem_size());
                        let shape = |text: SharedString| {
                            window.text_system().shape_line(
                                text.clone(),
                                font_size,
                                &[style.to_run(text.len())],
                                None,
                            )
                        };
                        let full = shape(title.clone());
                        if full.width <= bounds.size.width {
                            return full;
                        }
                        // GPUI's per-character ellipsis estimate can undercount
                        // mixed CJK text. Fit the actual shaped glyphs instead.
                        let suffix = shape("…".into());
                        if suffix.width > bounds.size.width {
                            return shape("".into());
                        }
                        let mut end = full
                            .index_for_x(bounds.size.width - suffix.width)
                            .unwrap_or(title.len());
                        loop {
                            let candidate = shape(format!("{}…", title[..end].trim_end()).into());
                            if candidate.width <= bounds.size.width || end == 0 {
                                return candidate;
                            }
                            end = full
                                .runs
                                .iter()
                                .flat_map(|run| &run.glyphs)
                                .map(|glyph| glyph.index)
                                .filter(|&index| index < end)
                                .max()
                                .unwrap_or(0);
                        }
                    }
                },
                |bounds, line, window, cx| {
                    if let Err(error) = line.paint(
                        bounds.origin,
                        bounds.size.height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    ) {
                        eprintln!("Could not paint task title: {error}");
                    }
                },
            )
            .w_full()
            .h_full(),
        )
}

/// Long menu labels use measured glyph truncation, including mixed CJK text.
pub fn task_menu_item(title: String) -> gpui_kit::component::menu::PopupMenuItem {
    gpui_kit::component::menu::PopupMenuItem::element(move |w, _| {
        let mut hint = String::new();
        for (i, c) in title.chars().enumerate() {
            if i > 0 && i % 36 == 0 {
                hint.push('\n');
            }
            hint.push(c);
        }
        quiet_button("menu-task-title", "")
            .w((w.viewport_size().width - px(100.)).min(px(380.)))
            .h_5()
            .p_0()
            .min_w_0()
            .tooltip(hint)
            .accessibility_label(title.clone())
            .child(single_line_text("menu-task-label", title.clone()))
    })
}

/// Property choices align their text independently of optional status symbols.
pub fn property_choice(
    id: impl Into<ElementId>,
    text: impl Into<String>,
    icon: Option<&str>,
    selected: bool,
) -> Button {
    let text = text.into();
    quiet_button(id, "")
        .w_full()
        .accessibility_label(text.clone())
        .child(
            div()
                .flex()
                .items_center()
                .w_full()
                .min_w_0()
                .gap_2()
                .when_some(icon, |d, icon| {
                    d.child(
                        div()
                            .w_4()
                            .flex_shrink_0()
                            .text_center()
                            .child(icon.to_string()),
                    )
                })
                .child(single_line_text("choice-label", text))
                .child(
                    div()
                        .w_4()
                        .flex_shrink_0()
                        .text_center()
                        .child(if selected { "✓" } else { "" }),
                ),
        )
}
