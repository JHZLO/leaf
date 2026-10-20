//! The terminal panel's shell, without the window: starting the shell on a pseudo terminal,
//! turning keys and pastes into the bytes a shell expects, and copying the screen into a frame
//! the panel paints. alacritty_terminal does the terminal emulation.
//!
//! Key sequences follow xterm as Alacritty sends them; pastes are filtered the way Alacritty
//! filters them (Alacritty is Apache-2.0).

use std::{
    borrow::Cow,
    cell::Cell,
    collections::HashMap,
    io,
    path::Path,
    sync::Arc,
    thread::{self, JoinHandle},
};

use alacritty_terminal::{
    event::{Notify, OnResize, WindowSize},
    event_loop::{EventLoop, EventLoopSender, Msg, Notifier, State},
    grid::{Dimensions, Scroll},
    index::{Column, Point},
    selection::Selection,
    sync::FairMutex,
    term::{self, Config, Term, cell::Flags, color::Colors},
    tty::{self, Options, Pty, Shell as Program},
    vte::ansi::{Color, NamedColor},
};

/// The parts of alacritty_terminal the panel names.
pub use alacritty_terminal::{
    event::{Event, EventListener},
    index::Side,
    selection::SelectionType,
    term::TermMode,
    vte::ansi::{CursorShape, Rgb},
};

/// Lines the scrollback keeps (alacritty's default).
const HISTORY: usize = 10_000;

/// The grid in cells, and one cell in pixels (shells may ask for the pixel size).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl GridSize {
    fn window(self) -> WindowSize {
        WindowSize {
            num_lines: self.rows,
            num_cols: self.cols,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
        }
    }
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }

    fn screen_lines(&self) -> usize {
        self.rows as usize
    }

    fn columns(&self) -> usize {
        self.cols as usize
    }
}

/// A shell running on its own pseudo terminal. A thread reads its output into the terminal;
/// the listener hears about it (redraws, replies the shell waits for, its exit).
pub struct Shell<L: EventListener + Send + 'static> {
    term: Arc<FairMutex<Term<L>>>,
    /// The size last given, for shells that ask for it.
    size: Cell<GridSize>,
    sender: EventLoopSender,
    io: Option<JoinHandle<(EventLoop<Pty, L>, State)>>,
}

impl<L: EventListener + Clone + Send + 'static> Shell<L> {
    /// The user's login shell in `cwd`, or `program` with its arguments (the tests run `sh -c`).
    /// Call it on the main thread: the shell inherits the signal mask of the thread that starts it.
    pub fn spawn(
        cwd: &Path,
        program: Option<(&str, &[&str])>,
        size: GridSize,
        listener: L,
    ) -> io::Result<Self> {
        let options = Options {
            // None is the login shell: macOS starts $SHELL through `login`, like Terminal.app.
            shell: program.map(|(program, args)| {
                Program::new(
                    program.to_string(),
                    args.iter().map(ToString::to_string).collect(),
                )
            }),
            working_directory: Some(cwd.to_path_buf()),
            drain_on_exit: true,
            env: shell_env(std::env::var_os("LANG").is_some()),
        };
        let config = Config {
            scrolling_history: HISTORY,
            ..Config::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, listener.clone())));
        let pty = tty::new(&options, size.window(), 0)?;
        let event_loop = EventLoop::new(term.clone(), listener, pty, true, false)?;
        let sender = event_loop.channel();
        let io = Some(event_loop.spawn());
        Ok(Self {
            term,
            size: Cell::new(size),
            sender,
            io,
        })
    }
}

impl<L: EventListener + Send + 'static> Shell<L> {
    /// The screen as the panel paints it.
    pub fn frame(&self, palette: &Palette) -> Frame {
        frame(&self.term.lock(), palette)
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    /// A color as the shell sees it, for its color queries (OSC 4, 10, 11).
    pub fn color(&self, index: usize, palette: &Palette) -> Rgb {
        palette.index(self.term.lock().colors(), index)
    }

    /// Scrolls the history by `lines`, up when positive.
    pub fn scroll(&self, lines: i32) {
        self.term.lock().scroll_display(Scroll::Delta(lines));
    }

    /// Starts a selection at a screen cell (`line` from the top of what is shown).
    pub fn select(&self, kind: SelectionType, line: usize, col: usize, side: Side) {
        let mut term = self.term.lock();
        let point = grid_point(&term, line, col);
        term.selection = Some(Selection::new(kind, point, side));
    }

    /// Moves the end of the selection being dragged.
    pub fn extend_selection(&self, line: usize, col: usize, side: Side) {
        let mut term = self.term.lock();
        let point = grid_point(&term, line, col);
        if let Some(selection) = term.selection.as_mut() {
            selection.update(point, side);
        }
    }

    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    /// Tells the terminal whether the panel has focus (a block cursor turns hollow without it).
    /// Returns whether the program asked to hear about focus changes (mode 1004).
    pub fn set_focused(&self, focused: bool) -> bool {
        let mut term = self.term.lock();
        term.is_focused = focused;
        term.mode().contains(TermMode::FOCUS_IN_OUT)
    }

    /// Sends bytes to the shell: keys, pastes, and the replies it asked for.
    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        Notifier(self.sender.clone()).notify(bytes);
    }

    /// What the user types: it also clears the selection and scrolls back to the prompt.
    pub fn input(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        {
            let mut term = self.term.lock();
            term.selection = None;
            term.scroll_display(Scroll::Bottom);
        }
        self.write(bytes);
    }

    /// Answers what a program asked of the terminal itself (cursor reports, its colors, the
    /// window size): programs like fish and vim wait for these. Hands back the events that are
    /// the panel's (clipboard, exit, redraw).
    pub fn answer(&self, event: Event, palette: &Palette) -> Option<Event> {
        match event {
            Event::PtyWrite(text) => self.write(text.into_bytes()),
            Event::ColorRequest(index, format) => {
                let color = self.color(index, palette);
                self.write(format(color).into_bytes());
            }
            Event::TextAreaSizeRequest(format) => {
                self.write(format(self.size.get().window()).into_bytes());
            }
            other => return Some(other),
        }
        None
    }

    /// Gives the grid a new size, telling the shell only when the cells changed.
    pub fn resize(&self, size: GridSize) {
        self.size.set(size);
        let mut term = self.term.lock();
        if term.columns() == size.cols as usize && term.screen_lines() == size.rows as usize {
            return;
        }
        Notifier(self.sender.clone()).on_resize(size.window());
        term.resize(size);
    }
}

impl<L: EventListener + Send + 'static> Drop for Shell<L> {
    /// Stops the reader. Dropping the pseudo terminal hangs up the shell and waits for it to
    /// exit, so that happens on a thread of its own, never the caller's.
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
        if let Some(io) = self.io.take() {
            thread::spawn(move || drop(io.join()));
        }
    }
}

/// A cell on screen as a point in the grid, which counts history lines as negative.
fn grid_point<L>(term: &Term<L>, line: usize, col: usize) -> Point {
    let line = line.min(term.screen_lines().saturating_sub(1));
    let col = col.min(term.columns().saturating_sub(1));
    term::viewport_to_point(term.grid().display_offset(), Point::new(line, Column(col)))
}

/// What subwy adds to the environment the shell inherits. An app started from the Dock has no
/// LANG, and without one shells and programs fall back to ASCII.
fn shell_env(has_lang: bool) -> HashMap<String, String> {
    let mut env = HashMap::from([
        ("TERM".to_string(), "xterm-256color".to_string()),
        ("COLORTERM".to_string(), "truecolor".to_string()),
        ("TERM_PROGRAM".to_string(), "subwy".to_string()),
    ]);
    if !has_lang {
        env.insert("LANG".to_string(), "en_US.UTF-8".to_string());
    }
    env
}

/// Modifier keys held with a key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

impl Mods {
    /// xterm's modifier parameter: 1 + shift (1) + alt (2) + ctrl (4).
    fn param(self) -> u8 {
        1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.ctrl as u8
    }

    fn any(self) -> bool {
        self.shift || self.alt || self.ctrl
    }
}

/// The bytes for a key the shell reads as a key rather than as text: Enter, arrows, function
/// keys, Ctrl combinations. `key` is GPUI's key name ("up", "pageup", "f5", "c"). None means
/// the key is text, which arrives through the input method (so Hangul composes).
///
/// Option stays macOS's way of typing characters (Option-e is an accent), so it only changes
/// the keys that are not text. Alt-Left and Alt-Right still move by word, as in Terminal.app.
pub fn encode_key(key: &str, mods: Mods, mode: TermMode) -> Option<Vec<u8>> {
    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let with_mods = |code: &str, end: char| -> Vec<u8> {
        if mods.any() {
            let code = if code.is_empty() { "1" } else { code };
            format!("\x1b[{code};{}{end}", mods.param()).into_bytes()
        } else {
            format!("\x1b[{code}{end}").into_bytes()
        }
    };
    // Cursor keys and F1 to F4 use the SS3 form without modifiers, cursor keys only in the
    // application cursor mode that full-screen programs turn on.
    let cursor = |end: char, ss3: bool| -> Vec<u8> {
        if !mods.any() && ss3 {
            format!("\x1bO{end}").into_bytes()
        } else {
            with_mods("", end)
        }
    };
    let alt_prefix = |bytes: &[u8]| -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len() + 1);
        if mods.alt {
            out.push(0x1b);
        }
        out.extend_from_slice(bytes);
        out
    };
    let bytes = match key {
        "enter" => alt_prefix(b"\r"),
        "tab" if mods.shift => b"\x1b[Z".to_vec(),
        "tab" => alt_prefix(b"\t"),
        "escape" => alt_prefix(b"\x1b"),
        "backspace" if mods.ctrl => alt_prefix(b"\x08"),
        "backspace" => alt_prefix(b"\x7f"),
        "space" if mods.ctrl => vec![0],
        "left" if mods.alt && !mods.ctrl && !mods.shift => b"\x1bb".to_vec(),
        "right" if mods.alt && !mods.ctrl && !mods.shift => b"\x1bf".to_vec(),
        "up" => cursor('A', app_cursor),
        "down" => cursor('B', app_cursor),
        "right" => cursor('C', app_cursor),
        "left" => cursor('D', app_cursor),
        "home" => cursor('H', app_cursor),
        "end" => cursor('F', app_cursor),
        "insert" => with_mods("2", '~'),
        "delete" => with_mods("3", '~'),
        "pageup" => with_mods("5", '~'),
        "pagedown" => with_mods("6", '~'),
        "f1" => cursor('P', true),
        "f2" => cursor('Q', true),
        "f3" => cursor('R', true),
        "f4" => cursor('S', true),
        "f5" => with_mods("15", '~'),
        "f6" => with_mods("17", '~'),
        "f7" => with_mods("18", '~'),
        "f8" => with_mods("19", '~'),
        "f9" => with_mods("20", '~'),
        "f10" => with_mods("21", '~'),
        "f11" => with_mods("23", '~'),
        "f12" => with_mods("24", '~'),
        key if mods.ctrl => {
            let mut chars = key.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return None;
            };
            let byte = match c.to_ascii_lowercase() {
                c @ 'a'..='z' => c as u8 - b'a' + 1,
                '@' | '2' => 0x00,
                '[' | '3' => 0x1b,
                '\\' | '4' => 0x1c,
                ']' | '5' => 0x1d,
                '^' | '6' => 0x1e,
                '_' | '-' | '/' | '7' => 0x1f,
                '?' | '8' => 0x7f,
                _ => return None,
            };
            alt_prefix(&[byte])
        }
        _ => return None,
    };
    Some(bytes)
}

/// A paste as the shell should receive it. In bracketed paste mode the text is wrapped so the
/// shell does not run it line by line, with escape characters removed so it cannot end the
/// bracket early; otherwise line breaks become the carriage returns a key press sends.
/// Paths as a shell reads them back, a word each and a space after: what Terminal.app types when
/// files are dropped on it. A path is quoted when a character in it would mean something to the
/// shell (a space, a quote, `~`, letters outside ASCII).
pub fn shell_words(paths: &[std::path::PathBuf]) -> String {
    let mut out = String::new();
    for path in paths {
        let text = path.to_string_lossy();
        let plain = !text.is_empty()
            && text
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || "/._-+,:@%=".contains(ch));
        if plain {
            out.push_str(&text);
        } else {
            out.push('\'');
            out.push_str(&text.replace('\'', "'\\''"));
            out.push('\'');
        }
        out.push(' ');
    }
    out
}

pub fn paste_bytes(text: &str, mode: TermMode) -> Vec<u8> {
    if mode.contains(TermMode::BRACKETED_PASTE) {
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(text.replace(['\x1b', '\x03'], "").as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

/// The colors programs choose from: 16 named ones from subwy's palette, then xterm's 6x6x6 cube
/// and gray ramp, plus the default text, background and cursor.
pub struct Palette {
    table: [Rgb; 256],
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
}

pub fn rgb(value: u32) -> Rgb {
    Rgb {
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
    }
}

impl Palette {
    pub fn new(ansi: [u32; 16], foreground: u32, background: u32, cursor: u32) -> Self {
        let mut table = [Rgb::default(); 256];
        for (slot, value) in table.iter_mut().zip(ansi) {
            *slot = rgb(value);
        }
        let level = |v: u8| if v == 0 { 0 } else { v * 40 + 55 };
        for i in 0..216u8 {
            table[16 + i as usize] = Rgb {
                r: level(i / 36),
                g: level(i / 6 % 6),
                b: level(i % 6),
            };
        }
        for i in 0..24u8 {
            let v = i * 10 + 8;
            table[232 + i as usize] = Rgb { r: v, g: v, b: v };
        }
        Self {
            table,
            foreground: rgb(foreground),
            background: rgb(background),
            cursor: rgb(cursor),
        }
    }

    /// A color by its index, as a program may have redefined it (OSC 4, 10, 11).
    pub fn index(&self, colors: &Colors, index: usize) -> Rgb {
        if let Some(set) = colors[index] {
            return set;
        }
        match index {
            0..=255 => self.table[index],
            i if i == NamedColor::Background as usize => self.background,
            i if i == NamedColor::Cursor as usize => self.cursor,
            i if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize).contains(&i) => {
                dim(self.table[i - NamedColor::DimBlack as usize])
            }
            i if i == NamedColor::DimForeground as usize => dim(self.foreground),
            _ => self.foreground,
        }
    }

    fn color(&self, colors: &Colors, color: Color, dimmed: bool) -> Rgb {
        let value = match color {
            Color::Spec(rgb) => rgb,
            Color::Named(named) => self.index(colors, named as usize),
            Color::Indexed(index) => self.index(colors, index as usize),
        };
        if dimmed { dim(value) } else { value }
    }
}

fn dim(c: Rgb) -> Rgb {
    let f = |v: u8| (v as f32 * 0.66) as u8;
    Rgb {
        r: f(c.r),
        g: f(c.g),
        b: f(c.b),
    }
}

/// Cells next to each other on one line that look the same, painted as one piece of text.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub col: usize,
    /// Columns covered: one per character, two for a wide one (Hangul, CJK, emoji).
    pub cells: usize,
    /// A wide character, alone in its span: its glyph covers two cells.
    pub wide: bool,
    pub text: String,
    pub fg: Rgb,
    /// None is the terminal's own background, which the panel already painted.
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub selected: bool,
}

impl Span {
    fn same_look(&self, other: &Span) -> bool {
        self.fg == other.fg
            && self.bg == other.bg
            && self.bold == other.bold
            && self.italic == other.italic
            && self.underline == other.underline
            && self.strike == other.strike
            && self.selected == other.selected
    }

    /// Blanks on the plain background paint nothing.
    fn plain(&self) -> bool {
        self.bg.is_none() && !self.underline && !self.strike && !self.selected
    }

    /// Drops the blanks at the end that paint nothing (each blank is one cell).
    fn trim_end(&mut self) {
        if !self.plain() {
            return;
        }
        let kept = self.text.trim_end_matches(' ').len();
        self.cells -= self.text.len() - kept;
        self.text.truncate(kept);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
    pub shape: CursorShape,
    /// The character under it, which a block cursor draws in the background color.
    pub text: String,
    pub wide: bool,
}

/// What the panel paints, copied out of the terminal so the lock is held only that long.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    /// One list per screen line, top first.
    pub lines: Vec<Vec<Span>>,
    pub cursor: Option<Cursor>,
    pub mode: TermMode,
    /// Lines scrolled back into the history; 0 is the live screen.
    pub scrolled: usize,
}

/// Copies the visible screen into spans: the selection marked, wide characters on their own,
/// blank stretches left out.
pub fn frame<L: EventListener>(term: &Term<L>, palette: &Palette) -> Frame {
    let content = term.renderable_content();
    let scrolled = content.display_offset;
    let mut lines: Vec<Vec<Span>> = vec![Vec::new(); term.screen_lines()];
    let mut cursor = None;
    let cursor_point = content.cursor.point;
    for indexed in content.display_iter {
        let cell = indexed.cell;
        let flags = cell.flags;
        if flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
            continue;
        }
        let Some(at) = term::point_to_viewport(scrolled, indexed.point) else {
            continue;
        };
        let Some(line) = lines.get_mut(at.line) else {
            continue;
        };
        let dimmed = flags.contains(Flags::DIM);
        let mut fg = palette.color(content.colors, cell.fg, dimmed);
        let mut bg = palette.color(content.colors, cell.bg, false);
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        let mut text = String::new();
        if !flags.contains(Flags::HIDDEN) {
            text.push(cell.c);
            if let Some(extra) = cell.zerowidth() {
                text.extend(extra);
            }
        }
        let wide = flags.contains(Flags::WIDE_CHAR);
        if indexed.point == cursor_point {
            cursor = Some(Cursor {
                line: at.line,
                col: at.column.0,
                shape: content.cursor.shape,
                text: text.clone(),
                wide,
            });
        }
        let span = Span {
            col: at.column.0,
            cells: if wide { 2 } else { 1 },
            wide,
            text,
            fg,
            bg: (bg != palette.background).then_some(bg),
            bold: flags.contains(Flags::BOLD),
            italic: flags.contains(Flags::ITALIC),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            strike: flags.contains(Flags::STRIKEOUT),
            selected: content
                .selection
                .is_some_and(|s| s.contains_cell(&indexed, cursor_point, content.cursor.shape)),
        };
        // A wide character always stands alone: its glyph spans two cells.
        match line.last_mut() {
            Some(last)
                if !wide
                    && !last.wide
                    && last.col + last.cells == span.col
                    && last.same_look(&span)
                    && !span.text.is_empty() =>
            {
                last.text.push_str(&span.text);
                last.cells += 1;
            }
            _ => line.push(span),
        }
    }
    for line in &mut lines {
        for span in line.iter_mut() {
            span.trim_end();
        }
        line.retain(|span| !(span.plain() && span.text.trim_start_matches(' ').is_empty()));
    }
    // A hidden cursor, or one scrolled out of view, is not drawn.
    let cursor = cursor.filter(|_| content.cursor.shape != CursorShape::Hidden);
    Frame {
        lines,
        cursor,
        mode: *term.mode(),
        scrolled,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    use alacritty_terminal::{event::VoidListener, index::Line, vte::ansi::Processor};

    use super::*;

    const ANSI: [u32; 16] = [
        0x000000, 0xAA0000, 0x00AA00, 0xAA5500, 0x0000AA, 0xAA00AA, 0x00AAAA, 0xAAAAAA, 0x555555,
        0xFF5555, 0x55FF55, 0xFFFF55, 0x5555FF, 0xFF55FF, 0x55FFFF, 0xFFFFFF,
    ];

    fn palette() -> Palette {
        Palette::new(ANSI, 0x222222, 0xF7F4ED, 0x4A7A2C)
    }

    fn term(cols: u16, rows: u16, bytes: &[u8]) -> Term<VoidListener> {
        let size = GridSize {
            cols,
            rows,
            cell_width: 8,
            cell_height: 16,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new().advance(&mut term, bytes);
        term
    }

    fn texts(frame: &Frame) -> Vec<Vec<&str>> {
        frame
            .lines
            .iter()
            .map(|line| line.iter().map(|s| s.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn keys_that_are_not_text_become_xterm_sequences() {
        let none = Mods::default();
        let shift = Mods {
            shift: true,
            ..none
        };
        let alt = Mods { alt: true, ..none };
        let ctrl = Mods { ctrl: true, ..none };
        let plain = TermMode::empty();
        let app = TermMode::APP_CURSOR;
        let key = |k, m, mode| encode_key(k, m, mode).unwrap();
        assert_eq!(key("enter", none, plain), b"\r");
        assert_eq!(key("backspace", none, plain), b"\x7f");
        assert_eq!(key("backspace", alt, plain), b"\x1b\x7f");
        assert_eq!(key("tab", shift, plain), b"\x1b[Z");
        assert_eq!(key("escape", none, plain), b"\x1b");
        assert_eq!(key("up", none, plain), b"\x1b[A");
        assert_eq!(key("up", none, app), b"\x1bOA");
        assert_eq!(key("up", ctrl, app), b"\x1b[1;5A");
        assert_eq!(key("end", shift, plain), b"\x1b[1;2F");
        assert_eq!(key("pageup", none, plain), b"\x1b[5~");
        assert_eq!(key("delete", ctrl, plain), b"\x1b[3;5~");
        assert_eq!(key("f1", none, plain), b"\x1bOP");
        assert_eq!(key("f5", none, plain), b"\x1b[15~");
        assert_eq!(key("c", ctrl, plain), [0x03]);
        assert_eq!(key("d", ctrl, plain), [0x04]);
        assert_eq!(key("[", ctrl, plain), [0x1b]);
        assert_eq!(key("space", ctrl, plain), [0x00]);
        // Option moves by word, as in Terminal.app.
        assert_eq!(key("left", alt, plain), b"\x1bb");
        assert_eq!(key("right", alt, plain), b"\x1bf");
    }

    #[test]
    fn text_keys_are_left_to_the_input_method() {
        let none = Mods::default();
        assert_eq!(encode_key("a", none, TermMode::empty()), None);
        // Option-e types an accent on macOS, so Option alone never makes a letter a key.
        let alt = Mods { alt: true, ..none };
        assert_eq!(encode_key("e", alt, TermMode::empty()), None);
    }

    #[test]
    fn pastes_are_bracketed_when_the_shell_asks() {
        assert_eq!(paste_bytes("a\nb\r\nc", TermMode::empty()), b"a\rb\rc");
        assert_eq!(
            paste_bytes("rm\x1b[201~ -rf", TermMode::BRACKETED_PASTE),
            b"\x1b[200~rm[201~ -rf\x1b[201~"
        );
    }

    #[test]
    fn the_palette_holds_the_xterm_cube_and_ramp() {
        let p = palette();
        let colors = Colors::default();
        assert_eq!(p.index(&colors, 1), rgb(0xAA0000));
        assert_eq!(p.index(&colors, 196), rgb(0xFF0000));
        assert_eq!(p.index(&colors, 232), rgb(0x080808));
        assert_eq!(p.index(&colors, 255), rgb(0xEEEEEE));
        assert_eq!(
            p.index(&colors, NamedColor::Background as usize),
            rgb(0xF7F4ED)
        );
    }

    #[test]
    fn a_line_of_one_look_is_one_span() {
        let term = term(20, 2, b"hello world");
        let frame = frame(&term, &palette());
        assert_eq!(texts(&frame), [vec!["hello world"], vec![]]);
        let span = &frame.lines[0][0];
        assert_eq!((span.col, span.cells), (0, 11));
        assert_eq!(span.fg, rgb(0x222222));
        assert_eq!(span.bg, None);
    }

    #[test]
    fn colors_and_styles_split_spans() {
        let term = term(20, 1, b"a\x1b[31mred\x1b[0m \x1b[1;44mB\x1b[0m");
        let frame = frame(&term, &palette());
        assert_eq!(texts(&frame), [vec!["a", "red", "B"]]);
        let line = &frame.lines[0];
        assert_eq!(line[1].fg, rgb(0xAA0000));
        assert_eq!(line[1].col, 1);
        assert!(line[2].bold);
        assert_eq!(line[2].bg, Some(rgb(0x0000AA)));
        assert_eq!(line[2].col, 5);
    }

    #[test]
    fn wide_characters_stand_alone_and_cover_two_cells() {
        let term = term(20, 1, "가나 ok".as_bytes());
        let frame = frame(&term, &palette());
        let line = &frame.lines[0];
        assert_eq!(texts(&frame), [vec!["가", "나", " ok"]]);
        assert_eq!((line[0].col, line[0].cells), (0, 2));
        assert_eq!((line[1].col, line[1].cells), (2, 2));
        assert_eq!((line[2].col, line[2].cells), (4, 3));
    }

    #[test]
    fn the_cursor_carries_the_character_under_it() {
        let shown = term(10, 2, b"ab\x1b[1D");
        let cursor = frame(&shown, &palette()).cursor.unwrap();
        assert_eq!((cursor.line, cursor.col), (0, 1));
        assert_eq!(cursor.text, "b");
        // Programs may hide it (DECTCEM).
        let hidden = term(10, 2, b"ab\x1b[?25l");
        assert!(frame(&hidden, &palette()).cursor.is_none());
    }

    #[test]
    fn a_selection_marks_its_cells() {
        let mut term = term(20, 1, b"hello world");
        let mut selection = Selection::new(
            SelectionType::Simple,
            Point::new(Line(0), Column(6)),
            Side::Left,
        );
        selection.update(Point::new(Line(0), Column(10)), Side::Right);
        term.selection = Some(selection);
        let frame = frame(&term, &palette());
        assert_eq!(texts(&frame), [vec!["hello", "world"]]);
        assert!(frame.lines[0][1].selected);
        assert_eq!(term.selection_to_string().as_deref(), Some("world"));
    }

    #[test]
    fn scrolled_back_shows_history() {
        let mut bytes = Vec::new();
        for i in 0..10 {
            bytes.extend_from_slice(format!("line {i}\r\n").as_bytes());
        }
        let mut term = term(10, 3, &bytes);
        term.scroll_display(Scroll::Delta(2));
        let frame = frame(&term, &palette());
        assert_eq!(frame.scrolled, 2);
        assert_eq!(texts(&frame)[0], ["line 6"]);
    }

    #[test]
    fn the_shell_env_names_subwy_and_fills_in_a_missing_lang() {
        let env = shell_env(false);
        assert_eq!(env["TERM"], "xterm-256color");
        assert_eq!(env["TERM_PROGRAM"], "subwy");
        assert_eq!(env["LANG"], "en_US.UTF-8");
        assert!(!shell_env(true).contains_key("LANG"));
    }

    #[derive(Clone)]
    struct Events(mpsc::Sender<Event>);

    impl EventListener for Events {
        fn send_event(&self, event: Event) {
            let _ = self.0.send(event);
        }
    }

    #[test]
    fn a_real_shell_runs_in_the_folder_and_reports_its_exit() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let size = GridSize {
            cols: 40,
            rows: 5,
            cell_width: 8,
            cell_height: 16,
        };
        let shell = Shell::spawn(
            dir.path(),
            Some(("/bin/sh", &["-c", "printf 'in %s' \"${PWD##*/}\"; exit 3"])),
            size,
            Events(tx),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut exit = None;
        while exit.is_none() && Instant::now() < deadline {
            if let Ok(Event::ChildExit(status)) = rx.recv_timeout(Duration::from_millis(100)) {
                exit = Some(status.code());
            }
        }
        assert_eq!(exit, Some(Some(3)));
        let name = dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let frame = shell.frame(&palette());
        assert_eq!(texts(&frame)[0], [format!("in {name}").as_str()]);
    }

    #[test]
    fn dropped_paths_are_typed_as_shell_words() {
        let paths: Vec<std::path::PathBuf> = [
            "/work/docs/a.md",
            "/work/My Notes/b.md",
            "/work/it's.md",
            "/work/한글.md",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        assert_eq!(
            shell_words(&paths),
            "/work/docs/a.md '/work/My Notes/b.md' '/work/it'\\''s.md' '/work/한글.md' "
        );
    }
}
