//! leaf's paper palette and sizes. The single source of every color: leaf's own views read
//! `colors(cx)`, and the gpui-kit theme (editor, scrollbars, menus) is generated from the same values.

use gpui_kit::component::{ActiveTheme as _, Theme, ThemeRegistry};
use gpui_kit::*;

pub struct Palette {
    pub paper: u32,
    pub side: u32,
    pub desk: u32,
    pub line: u32,
    pub frame: u32,
    pub ink: u32,
    pub sub: u32,
    pub num: u32,
    pub ignored: u32,
    pub accent: u32,
    pub accent_text: u32,
    pub sel_bg: u32,
    pub sel_ink: u32,
    pub cur: u32,
    pub hover: u32,
    pub guide: u32,
    pub fold: u32,
    pub syn_title: u32,
    pub syn_mark: u32,
    pub syn_literal: u32,
    pub syn_keyword: u32,
    pub syn_type: u32,
    pub syn_comment: u32,
    /// The terminal's 16 named colors (black, red, green, yellow, blue, magenta, cyan, white,
    /// then their bright forms), toned to the paper like the syntax colors.
    pub ansi: [u32; 16],
}

pub const LIGHT: Palette = Palette {
    paper: 0xF7F4ED,
    side: 0xEFEBE2,
    desk: 0xE8E3D8,
    line: 0xDDD6C8,
    frame: 0xD3CCBD,
    ink: 0x2D2A25,
    sub: 0x6F695F,
    num: 0xA39C90,
    ignored: 0xA29B8F,
    accent: 0x4A7A2C,
    accent_text: 0x3E6A22,
    sel_bg: 0xDCE6CE,
    sel_ink: 0x26401A,
    cur: 0xF0ECE3,
    hover: 0xE7E2D7,
    guide: 0xDAD3C5,
    fold: 0xCFC7B6,
    syn_title: 0x3B5E25,
    syn_mark: 0xA39C90,
    syn_literal: 0x94572A,
    syn_keyword: 0x7A4E9C,
    syn_type: 0x2F6B8A,
    syn_comment: 0x9A9387,
    ansi: [
        0x2D2A25, 0xA63D2F, 0x4A7A2C, 0x8A6516, 0x2F6B8A, 0x7A4E9C, 0x2E7672, 0x8F887C, 0x6F695F,
        0xC0493A, 0x5E9440, 0xA97B1C, 0x3B7FA6, 0x9461B8, 0x3A9894, 0xB3AC9F,
    ],
};

pub const DARK: Palette = Palette {
    paper: 0x23211D,
    side: 0x1D1B18,
    desk: 0x191714,
    line: 0x34302A,
    frame: 0x3B362F,
    ink: 0xE3DDD2,
    sub: 0xA39B8D,
    num: 0x6E675C,
    ignored: 0x776F63,
    accent: 0x8DBF6A,
    accent_text: 0x9CCB78,
    sel_bg: 0x2F3B25,
    sel_ink: 0xD6EBC4,
    cur: 0x2A2723,
    hover: 0x24221E,
    guide: 0x3A352E,
    fold: 0x4A443B,
    syn_title: 0x9CCB78,
    syn_mark: 0x7D7568,
    syn_literal: 0xD7A46B,
    syn_keyword: 0xC9A0DC,
    syn_type: 0x7FB3CF,
    syn_comment: 0x7D7568,
    ansi: [
        0x3B362F, 0xE07A68, 0x8DBF6A, 0xD7A46B, 0x7FB3CF, 0xC9A0DC, 0x7CC4BE, 0xE3DDD2, 0x7D7568,
        0xF09080, 0x9CCB78, 0xE6BC85, 0x98C5DE, 0xD6B4E6, 0x94D4CE, 0xF5F1EA,
    ],
};

/// SF Mono. GPUI finds it only by the system's monospaced font name; "SF Mono" falls back to Helvetica.
pub const MONO_FONT: &str = ".AppleSystemUIFontMonospaced";

/// The editor font the settings name, with "SF Mono" mapped to the name GPUI can find.
pub fn editor_font(cx: &App) -> String {
    match crate::settings(cx).editor_font.as_str() {
        "SF Mono" => MONO_FONT.to_string(),
        other => other.to_string(),
    }
}

/// The editor's text size at 100% zoom, from the settings.
pub fn editor_text(cx: &App) -> f32 {
    crate::settings(cx).editor_font_size
}

/// The palette for the current appearance.
pub fn colors(cx: &App) -> &'static Palette {
    if cx.theme().is_dark() { &DARK } else { &LIGHT }
}

/// A palette value as a GPUI color.
pub fn c(value: u32) -> Hsla {
    rgb(value).into()
}

/// Sizes at 100% zoom (the design doc's sizes table). Multiply by [`zoom`] before use.
pub mod size {
    pub const TITLE_BAR: f32 = 38.;
    pub const TREE_WIDTH: f32 = 300.;
    pub const TREE_ROW: f32 = 22.;
    pub const TREE_INDENT: f32 = 14.;
    pub const KEY_HINTS: f32 = 30.;
    pub const TAB_BAR: f32 = 34.;
    pub const STATUS_BAR: f32 = 26.;
    pub const SHEET: f32 = 720.;
    pub const UI_TEXT: f32 = 13.;
    pub const EDITOR_TEXT: f32 = 13.;
    pub const TERMINAL_HEIGHT: f32 = 260.;
    /// The terminal's line height, as a multiple of its text size.
    pub const TERMINAL_LINE: f32 = 1.3;
}

/// UI zoom steps, in percent.
pub const ZOOM_STEPS: [u32; 8] = [80, 90, 100, 110, 125, 150, 175, 200];

/// The current UI zoom in percent.
#[derive(Clone, Copy)]
pub struct Zoom(pub u32);

impl Global for Zoom {}

pub fn zoom(cx: &App) -> f32 {
    cx.try_global::<Zoom>().map_or(100, |z| z.0) as f32 / 100.
}

/// A design size scaled by the current zoom.
pub fn z(value: f32, cx: &App) -> Pixels {
    px(value * zoom(cx))
}

/// Sets the zoom, scales the gpui-kit font sizes with it and redraws every window.
pub fn set_zoom(percent: u32, cx: &mut App) {
    cx.set_global(Zoom(percent));
    let factor = percent as f32 / 100.;
    let editor_size = editor_text(cx);
    Theme::update(cx, |theme| {
        theme.font_size = px(size::UI_TEXT * factor);
        theme.mono_font_size = px(editor_size * factor);
    });
}

fn hex(value: u32) -> String {
    format!("#{value:06X}")
}

fn theme_config(name: &str, mode: &str, p: &Palette, cx: &App) -> serde_json::Value {
    let style = |color: u32| serde_json::json!({ "color": hex(color) });
    serde_json::json!({
        "name": name,
        "mode": mode,
        "font.size": size::UI_TEXT,
        "mono_font.family": editor_font(cx),
        "mono_font.size": editor_text(cx),
        "radius": 4,
        "radius.lg": 6,
        "shadow": false,
        "colors": {
            "background": hex(p.paper),
            "foreground": hex(p.ink),
            "border": hex(p.line),
            "input.border": hex(p.line),
            "ring": hex(p.accent),
            "caret": hex(p.accent),
            "selection.background": hex(p.sel_bg),
            "muted.background": hex(p.side),
            "muted.foreground": hex(p.sub),
            "accent.background": hex(p.hover),
            "accent.foreground": hex(p.ink),
            "primary.background": hex(p.ink),
            "primary.foreground": hex(p.paper),
            "secondary.background": hex(p.side),
            "secondary.foreground": hex(p.ink),
            "button.background": hex(p.side),
            "button.hover.background": hex(p.hover),
            "button.active.background": hex(p.guide),
            "button.foreground": hex(p.ink),
            "popover.background": hex(p.paper),
            "popover.foreground": hex(p.ink),
            "list.background": hex(p.side),
            "list.hover.background": hex(p.hover),
            "list.active.background": hex(p.sel_bg),
            "list.active.border": hex(p.sel_bg),
            "sidebar.background": hex(p.side),
            "sidebar.foreground": hex(p.ink),
            "sidebar.border": hex(p.line),
            "tab.background": hex(p.side),
            "tab.foreground": hex(p.sub),
            "tab.active.background": hex(p.paper),
            "tab.active.foreground": hex(p.ink),
            "tab_bar.background": hex(p.side),
            "title_bar.background": hex(p.side),
            "title_bar.border": hex(p.line),
            "status_bar.background": hex(p.side),
            "status_bar.border": hex(p.line),
            "window.border": hex(p.frame),
            "link": hex(p.syn_type),
            "scrollbar.background": "#00000000",
            "scrollbar.thumb.background": hex(p.guide),
            "scrollbar.thumb.hover.background": hex(p.num),
        },
        "highlight": {
            "editor.background": hex(p.paper),
            "editor.foreground": hex(p.ink),
            "editor.active_line.background": hex(p.cur),
            "editor.line_number": hex(p.num),
            "editor.active_line_number": hex(p.ink),
            "editor.invisible": hex(p.guide),
            "syntax": {
                "title": { "color": hex(p.syn_title), "font_weight": 500 },
                "punctuation": style(p.syn_mark),
                "punctuation.list_marker": style(p.syn_mark),
                "punctuation.special": style(p.syn_mark),
                "punctuation.delimiter": style(p.syn_mark),
                "punctuation.bracket": style(p.sub),
                "text.literal": style(p.syn_literal),
                "string": style(p.syn_literal),
                "string.escape": style(p.syn_literal),
                "keyword": style(p.syn_keyword),
                "type": style(p.syn_type),
                "constructor": style(p.syn_type),
                "function": style(p.syn_type),
                "constant": style(p.syn_literal),
                "number": style(p.syn_literal),
                "boolean": style(p.syn_keyword),
                "attribute": style(p.syn_type),
                "tag": style(p.syn_keyword),
                "property": style(p.ink),
                "link_text": style(p.syn_type),
                "link_uri": style(p.sub),
                "comment": { "color": hex(p.syn_comment), "font_style": "italic" },
                "comment.doc": { "color": hex(p.syn_comment), "font_style": "italic" },
                "emphasis": { "color": hex(p.sub), "font_style": "italic" },
                "emphasis.strong": { "color": hex(p.ink), "font_weight": 600 },
            },
        },
    })
}

/// Registers the leaf light and dark themes and follows the system appearance.
pub fn init(cx: &mut App) {
    let set = serde_json::json!({
        "name": "leaf",
        "themes": [theme_config("Leaf Light", "light", &LIGHT, cx), theme_config("Leaf Dark", "dark", &DARK, cx)],
    });
    let registry = ThemeRegistry::global_mut(cx);
    if let Err(err) = registry.load_themes_from_str(&set.to_string()) {
        eprintln!("leaf: theme did not load: {err}");
        return;
    }
    let light = registry.themes().get("Leaf Light").cloned();
    let dark = registry.themes().get("Leaf Dark").cloned();
    Theme::update(cx, |theme| {
        if let Some(light) = light {
            theme.light_theme = light;
        }
        if let Some(dark) = dark {
            theme.dark_theme = dark;
        }
    });
    Theme::sync_system_appearance(None, cx);
    set_zoom(100, cx);
}
