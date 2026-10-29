//! Shared semantic colors for the app shell and the GPUI component theme.
use gpui_kit::{
    App, Window,
    component::{Theme, ThemeMode},
    px, rgb,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    Light,
    Dark,
    System,
}
impl Appearance {
    pub fn next(self) -> Self {
        match self {
            Self::System => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
        }
    }
}
#[cfg(any(test, feature = "qa"))]
impl Appearance {
    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark | Self::System => Self::Light,
        }
    }
}
pub fn apply(appearance: Appearance, window: Option<&mut Window>, cx: &mut App) {
    // Match native titlebar chrome to the app, and release the override in System mode.
    cx.set_window_appearance(match appearance {
        Appearance::Light => Some(gpui_kit::WindowAppearance::Light),
        Appearance::Dark => Some(gpui_kit::WindowAppearance::Dark),
        Appearance::System => None,
    });
    #[cfg(target_os = "macos")]
    let system = cx.window_appearance();
    #[cfg(not(target_os = "macos"))]
    let system = window
        .as_ref()
        .map(|w| w.appearance())
        .unwrap_or_else(|| cx.window_appearance());
    Theme::change(
        match appearance {
            Appearance::Light => ThemeMode::Light,
            Appearance::Dark => ThemeMode::Dark,
            Appearance::System => system.into(),
        },
        window,
        cx,
    );
    // Update both solid colors and the component render tokens together.
    let p = palette(cx);
    let dark = Theme::global(cx).is_dark();
    Theme::update(cx, |theme| {
        theme.radius = px(6.);
        theme.button = rgb(if dark { 0x2c2e32 } else { 0xf2f3ef }).into();
        theme.button_hover = rgb(if dark { 0x393d43 } else { 0xe1e4dc }).into();
        theme.button_active = rgb(if dark { 0x484e56 } else { 0xd0d6c9 }).into();
        theme.button_foreground = rgb(p.text).into();
        theme.input = rgb(if dark { 0x777f8a } else { 0x81897b }).into();
        theme.button_primary = rgb(p.accent).into();
        theme.button_primary_hover = rgb(if dark { 0xdbc08e } else { 0x654619 }).into();
        theme.button_primary_active = rgb(if dark { 0xebd2a7 } else { 0x4d3412 }).into();
        theme.button_primary_foreground = rgb(if dark { 0x211d16 } else { 0xfffcf5 }).into();
        theme.primary = rgb(p.accent).into();
        theme.primary_foreground = theme.button_primary_foreground;
        theme.ring = rgb(p.accent).into();
        theme.link = rgb(p.accent).into();
        theme.link_hover = theme.button_primary_hover;
        theme.link_active = theme.button_primary_active;
        theme.danger = rgb(p.danger).into();
    });
}
#[derive(Clone, Copy)]
pub struct Palette {
    pub tag_surface: u32,
    pub tag_text: u32,
    pub tag_success: u32,
    pub canvas: u32,
    pub side: u32,
    pub panel: u32,
    pub line: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub accent_surface: u32,
    pub accent_hover: u32,
    pub hover: u32,
    pub subtle_line: u32,
    pub border_strong: u32,
    pub secondary: u32,
    pub warning: u32,
    pub success: u32,
    pub danger: u32,
    pub review: u32,
    pub cancelled: u32,
}
pub fn palette(cx: &App) -> Palette {
    if Theme::global(cx).is_dark() {
        Palette {
            tag_surface: 0x34383e,
            tag_text: 0xf0f1f3,
            tag_success: 0xb0e6c8,
            canvas: 0x191a1c,
            side: 0x141517,
            panel: 0x202124,
            line: 0x303135,
            text: 0xe5e5e8,
            muted: 0x96989f,
            accent: 0xc8a875,
            accent_surface: 0x39332a,
            accent_hover: 0x30291e,
            hover: 0x242529,
            subtle_line: 0x242529,
            border_strong: 0x414247,
            secondary: 0xb9bbc3,
            warning: 0xe2b36a,
            success: 0x80b8a0,
            danger: 0xe39790,
            review: 0xbca1df,
            cancelled: 0x9b7d82,
        }
    } else {
        Palette {
            tag_surface: 0xe5eae5,
            tag_text: 0x303830,
            tag_success: 0x205739,
            canvas: 0xfafaf9,
            side: 0xeeeeec,
            panel: 0xffffff,
            line: 0xd5d6d3,
            text: 0x252724,
            muted: 0x60665f,
            accent: 0x795822,
            accent_surface: 0xeee2cd,
            accent_hover: 0xf0e9dd,
            hover: 0xe5e6e2,
            subtle_line: 0xe3e4e0,
            border_strong: 0xa1a59e,
            secondary: 0x484f48,
            warning: 0x875b12,
            success: 0x2c7153,
            danger: 0xa53d35,
            review: 0x765398,
            cancelled: 0x79565c,
        }
    }
}
