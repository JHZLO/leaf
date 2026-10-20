//! subwy's paper palette and sizes. The single source of every color: subwy's own views read
//! `colors(cx)`, and the gpui-kit theme (editor, scrollbars, menus) is generated from the same values.

use gpui_kit::component::{
    ActiveTheme as _, Theme, ThemeMode, ThemeRegistry, highlighter::LanguageRegistry,
};
use gpui_kit::*;
use subwy_project::settings::Appearance;

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
    /// The tree's selection band: softer than `sel_bg`, so the route's colors read on it.
    pub tree_sel: u32,
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
    /// The tree's route: the line leaving each depth's folder, the open folder's first, in soft
    /// pastel tones of metro-map lines (the user asked for pastel inside the app).
    pub route: [u32; 6],
    /// The logo's three lines, which keep the bright colors of the app icon.
    pub brand: [u32; 3],
}

pub const LIGHT: Palette = Palette {
    paper: 0xFBFAF8,
    side: 0xF3F2EF,
    desk: 0xEDECE9,
    line: 0xE7E5E1,
    frame: 0xD9D7D2,
    ink: 0x1F2023,
    sub: 0x6B6C70,
    num: 0xB5B4B0,
    ignored: 0xA2A3A7,
    accent: 0x5DBB8A,
    accent_text: 0x2F8F62,
    sel_bg: 0xBFE6D0,
    sel_ink: 0x1F4D3A,
    tree_sel: 0xECEAE5,
    cur: 0xF4F2EE,
    hover: 0xF1EFEB,
    guide: 0xE0DED9,
    fold: 0xD5D3CE,
    syn_title: 0x3A9466,
    syn_mark: 0xA2A3A7,
    syn_literal: 0xB4530F,
    syn_keyword: 0x8B3FD9,
    syn_type: 0x1D6FD8,
    syn_comment: 0x9C9DA1,
    ansi: [
        0x2D2A25, 0xA63D2F, 0x4A7A2C, 0x8A6516, 0x2F6B8A, 0x7A4E9C, 0x2E7672, 0x8F887C, 0x6F695F,
        0xC0493A, 0x5E9440, 0xA97B1C, 0x3B7FA6, 0x9461B8, 0x3A9894, 0xB3AC9F,
    ],
    route: [0x6CC193, 0xF2A66E, 0xEE8AAE, 0x7EA8EE, 0xAE95EE, 0x69C4BF],
    brand: [0x12A150, 0xFF7A00, 0xF0306E],
};

pub const DARK: Palette = Palette {
    paper: 0x151619,
    side: 0x111214,
    desk: 0x0D0E10,
    line: 0x24262A,
    frame: 0x24262B,
    ink: 0xE7E7E5,
    sub: 0x8D8F95,
    num: 0x4A4C52,
    ignored: 0x5E6066,
    accent: 0x8EDDB1,
    accent_text: 0x8EDDB1,
    sel_bg: 0x34584A,
    sel_ink: 0xD3F0E2,
    tree_sel: 0x22242A,
    cur: 0x1A1C20,
    hover: 0x1C1E22,
    guide: 0x2A2D33,
    fold: 0x3A3D44,
    syn_title: 0x8EDDB1,
    syn_mark: 0x5E6066,
    syn_literal: 0xF5B379,
    syn_keyword: 0xC4A1FF,
    syn_type: 0x7CB7FF,
    syn_comment: 0x6E7076,
    ansi: [
        0x3B362F, 0xE07A68, 0x8DBF6A, 0xD7A46B, 0x7FB3CF, 0xC9A0DC, 0x7CC4BE, 0xE3DDD2, 0x7D7568,
        0xF09080, 0x9CCB78, 0xE6BC85, 0x98C5DE, 0xD6B4E6, 0x94D4CE, 0xF5F1EA,
    ],
    route: [0x8EDDB1, 0xF7BE8F, 0xF5A3C0, 0x9DBEF5, 0xC3B1F5, 0x8FDAD5],
    brand: [0x2BD47A, 0xFF9A3D, 0xFF5A92],
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

/// The terminal's text size at 100% zoom: its own setting, or the editor's.
pub fn terminal_text(cx: &App) -> f32 {
    crate::settings(cx)
        .terminal_font_size
        .unwrap_or_else(|| editor_text(cx))
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
    pub const TREE_HEAD: f32 = 34.;
    pub const TREE_ROW: f32 = 22.;
    pub const TREE_INDENT: f32 = 14.;
    pub const KEY_HINTS: f32 = 30.;
    pub const TAB_BAR: f32 = 34.;
    pub const STATUS_BAR: f32 = 26.;
    pub const UI_TEXT: f32 = 13.;
    pub const EDITOR_TEXT: f32 = 13.;
    /// The space between the islands (tree, editor, terminal), which is also where they resize.
    pub const ISLAND_GAP: f32 = 6.;
    pub const ISLAND_RADIUS: f32 = 8.;
    pub const TERMINAL_HEIGHT: f32 = 260.;
    pub const TERMINAL_MIN: f32 = 80.;
    pub const TERMINAL_TABS: f32 = 30.;
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

/// How opaque gpui-kit lets the editor selection be: it caps the theme's selection color at 30%.
const SELECTION_ALPHA: f32 = 0.3;

/// The selection color to hand gpui-kit so that, drawn at its 30% over the paper, it comes out as
/// `sel_bg`, the selection subwy's own views paint. Given `sel_bg` itself, a drag was barely visible.
fn editor_selection(p: &Palette) -> u32 {
    let channel = |shift: u32| {
        let wanted = ((p.sel_bg >> shift) & 0xFF) as f32;
        let paper = ((p.paper >> shift) & 0xFF) as f32;
        let value = (wanted - (1. - SELECTION_ALPHA) * paper) / SELECTION_ALPHA;
        (value.round().clamp(0., 255.) as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
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
            "selection.background": hex(editor_selection(p)),
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
            // Off, the switch's track must still read as one on the paper.
            "switch.background": hex(p.ignored),
            "switch.thumb.background": hex(p.paper),
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
                // Markdown's structure marks (list and task markers, table pipes, quotes), as IntelliJ.
                "punctuation.list_marker": style(p.syn_keyword),
                "punctuation.special": style(p.syn_mark),
                "punctuation.delimiter": style(p.syn_mark),
                "punctuation.bracket": style(p.sub),
                "text.literal": style(p.syn_literal),
                // Markdown's `inline code`, which gpui-kit names apart from code blocks.
                "text.code.span": style(p.syn_literal),
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

/// Registers the subwy light and dark themes and follows the system appearance.
pub fn init(cx: &mut App) {
    register(cx);
    apply_appearance(None, cx);
    set_zoom(100, cx);
    highlight_markdown();
}

/// The settings changed: the appearance, the editor font and its size are applied anew.
pub fn settings_changed(cx: &mut App) {
    apply_appearance(None, cx);
}

/// Light or dark paper as the settings say, or as macOS is set (then again whenever macOS changes).
pub fn apply_appearance(window: Option<&mut Window>, cx: &mut App) {
    match crate::settings(cx).appearance {
        Appearance::System => Theme::sync_system_appearance(window, cx),
        Appearance::Light => Theme::change(ThemeMode::Light, window, cx),
        Appearance::Dark => Theme::change(ThemeMode::Dark, window, cx),
    }
    // A theme brings the font and sizes it was registered with at start (registering again does
    // not replace it): the settings' font and the current zoom go back on top.
    let font: SharedString = editor_font(cx).into();
    Theme::update(cx, |theme| theme.mono_font_family = font);
    set_zoom(cx.try_global::<Zoom>().map_or(100, |zoom| zoom.0), cx);
}

/// Registers the subwy light and dark themes, which carry the editor font from the settings.
fn register(cx: &mut App) {
    let set = serde_json::json!({
        "name": "subwy",
        "themes": [theme_config("Subwy Light", "light", &LIGHT, cx), theme_config("Subwy Dark", "dark", &DARK, cx)],
    });
    let registry = ThemeRegistry::global_mut(cx);
    if let Err(err) = registry.load_themes_from_str(&set.to_string()) {
        eprintln!("subwy: theme did not load: {err}");
        return;
    }
    let light = registry.themes().get("Subwy Light").cloned();
    let dark = registry.themes().get("Subwy Dark").cloned();
    Theme::update(cx, |theme| {
        if let Some(light) = light {
            theme.light_theme = light;
        }
        if let Some(dark) = dark {
            theme.dark_theme = dark;
        }
    });
}

/// Swaps in subwy's Markdown highlights, which also mark tables, task boxes and link definitions.
fn highlight_markdown() {
    let registry = LanguageRegistry::singleton();
    if let Some(mut config) = registry.language("markdown") {
        config.highlights = include_str!("markdown_highlights.scm").into();
        registry.register("markdown", &config);
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings gpui's `test` attribute, which shadows the standard one.
    use super::{DARK, LIGHT, SELECTION_ALPHA, editor_selection};

    #[test]
    fn the_editor_selection_blends_to_the_selection_color() {
        for p in [&LIGHT, &DARK] {
            let given = editor_selection(p);
            for shift in [16, 8, 0] {
                let channel = |value: u32| ((value >> shift) & 0xFF) as f32;
                let blended =
                    SELECTION_ALPHA * channel(given) + (1. - SELECTION_ALPHA) * channel(p.paper);
                assert!(
                    (blended - channel(p.sel_bg)).abs() <= 1.,
                    "{given:06X} at {shift}"
                );
            }
        }
    }
}
