use unicode_width::UnicodeWidthChar;
use vte::{Params, Perform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);
pub const BLACK: Color = Color(0, 0, 0);
pub const NEON_CYAN: Color = Color(0, 255, 220);

#[derive(Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: u8,
}

pub const ATTR_BOLD: u8 = 1 << 0;
pub const ATTR_UNDERLINE: u8 = 1 << 1;
pub const ATTR_REVERSE: u8 = 1 << 2;
pub const ATTR_WIDE: u8 = 1 << 3;
pub const ATTR_CONT: u8 = 1 << 4;

#[derive(Clone)]
struct Screen {
    cells: Vec<Cell>,
    cx: usize,
    cy: usize,
    top: usize,
    bot: usize,
    saved_cx: usize,
    saved_cy: usize,
    saved_fg: Color,
    saved_bg: Color,
    saved_attrs: u8,
}

pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub fg: Color,
    pub bg: Color,
    pub attrs: u8,
    pub cursor_visible: bool,
    pub dirty: bool,
    pub bytes_since_last: u32,
    alt: bool,
    primary: Screen,
    alternate: Screen,
}

impl Screen {
    fn new(cols: usize, rows: usize) -> Self {
        Self {
            cells: vec![
                Cell {
                    ch: ' ',
                    fg: NEON_CYAN,
                    bg: BLACK,
                    attrs: 0,
                };
                cols * rows
            ],
            cx: 0,
            cy: 0,
            top: 0,
            bot: rows.saturating_sub(1),
            saved_cx: 0,
            saved_cy: 0,
            saved_fg: NEON_CYAN,
            saved_bg: BLACK,
            saved_attrs: 0,
        }
    }
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            fg: NEON_CYAN,
            bg: BLACK,
            attrs: 0,
            cursor_visible: true,
            dirty: true,
            bytes_since_last: 0,
            alt: false,
            primary: Screen::new(cols, rows),
            alternate: Screen::new(cols, rows),
        }
    }
    pub fn cx(&self) -> usize {
        self.screen().cx
    }
    pub fn cy(&self) -> usize {
        self.screen().cy
    }
    pub fn cells(&self) -> &[Cell] {
        &self.screen().cells
    }
    pub fn is_alt(&self) -> bool {
        self.alt
    }
    fn screen(&self) -> &Screen {
        if self.alt {
            &self.alternate
        } else {
            &self.primary
        }
    }
    fn screen_mut(&mut self) -> &mut Screen {
        if self.alt {
            &mut self.alternate
        } else {
            &mut self.primary
        }
    }
    fn blank(&self) -> Cell {
        Cell {
            ch: ' ',
            fg: self.fg,
            bg: self.bg,
            attrs: 0,
        }
    }
    fn newline(&mut self) {
        let cols = self.cols;
        let bot = self.screen().bot;
        {
            let s = self.screen_mut();
            s.cx = 0;
        }
        if self.screen().cy >= bot {
            self.scroll_up(1);
        } else {
            self.screen_mut().cy += 1;
        }
        let _ = cols;
    }
    fn scroll_up(&mut self, n: usize) {
        let cols = self.cols;
        let blank = self.blank();
        let top = self.screen().top;
        let bot = self.screen().bot;
        if top >= bot {
            return;
        }
        let n = n.min(bot - top + 1);
        let s = self.screen_mut();
        for y in top..=bot - n {
            let src = (y + n) * cols;
            let dst = y * cols;
            s.cells.copy_within(src..src + cols, dst);
        }
        for y in bot + 1 - n..=bot {
            let row = y * cols;
            s.cells[row..row + cols].fill(blank);
        }
        self.dirty = true;
    }
    fn scroll_down(&mut self, n: usize) {
        let cols = self.cols;
        let blank = self.blank();
        let top = self.screen().top;
        let bot = self.screen().bot;
        if top >= bot {
            return;
        }
        let n = n.min(bot - top + 1);
        let s = self.screen_mut();
        for y in (top + n..=bot).rev() {
            let src = (y - n) * cols;
            let dst = y * cols;
            s.cells.copy_within(src..src + cols, dst);
        }
        for y in top..top + n {
            let row = y * cols;
            s.cells[row..row + cols].fill(blank);
        }
        self.dirty = true;
    }
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        for alt in [false, true] {
            let old_cells;
            let old_cols;
            let old_rows;
            let old_cx;
            let old_cy;
            {
                let s = if alt {
                    &self.alternate
                } else {
                    &self.primary
                };
                old_cells = s.cells.clone();
                old_cols = self.cols;
                old_rows = self.rows;
                old_cx = s.cx;
                old_cy = s.cy;
            }
            let mut cells = vec![self.blank(); cols * rows];
            for y in 0..rows.min(old_rows) {
                for x in 0..cols.min(old_cols) {
                    cells[y * cols + x] = old_cells[y * old_cols + x];
                }
            }
            let target = if alt {
                &mut self.alternate
            } else {
                &mut self.primary
            };
            target.cells = cells;
            target.cx = old_cx.min(cols - 1);
            target.cy = old_cy.min(rows - 1);
            target.top = 0;
            target.bot = rows - 1;
        }
        self.cols = cols;
        self.rows = rows;
        self.dirty = true;
    }
    fn clear_screen(&mut self, mode: u16) {
        let blank = self.blank();
        let cols = self.cols;
        let cx = self.screen().cx;
        let cy = self.screen().cy;
        let p = cy * cols + cx;
        let s = self.screen_mut();
        match mode {
            0 => {
                for c in &mut s.cells[p..] {
                    *c = blank;
                }
            }
            1 => {
                let end = p.min(s.cells.len().saturating_sub(1));
                for c in &mut s.cells[..=end] {
                    *c = blank;
                }
            }
            2 | 3 => s.cells.fill(blank),
            _ => {}
        }
        self.dirty = true;
    }
    fn clear_line(&mut self, mode: u16) {
        let blank = self.blank();
        let cols = self.cols;
        let cy = self.screen().cy;
        let cx = self.screen().cx;
        let row = cy * cols;
        let s = self.screen_mut();
        match mode {
            0 => s.cells[row + cx..row + cols].fill(blank),
            1 => s.cells[row..=row + cx].fill(blank),
            2 => s.cells[row..row + cols].fill(blank),
            _ => {}
        }
        self.dirty = true;
    }
    fn erase_chars(&mut self, n: usize) {
        let blank = self.blank();
        let cols = self.cols;
        let cy = self.screen().cy;
        let cx = self.screen().cx;
        let end = (cx + n).min(cols);
        let s = self.screen_mut();
        s.cells[cy * cols + cx..cy * cols + end].fill(blank);
        self.dirty = true;
    }
    fn insert_chars(&mut self, n: usize) {
        let blank = self.blank();
        let cols = self.cols;
        let cy = self.screen().cy;
        let cx = self.screen().cx;
        let n = n.min(cols - cx);
        let s = self.screen_mut();
        let row = cy * cols;
        for x in (cx + n..cols).rev() {
            s.cells[row + x] = s.cells[row + x - n];
        }
        for x in cx..cx + n {
            s.cells[row + x] = blank;
        }
        self.dirty = true;
    }
    fn delete_chars(&mut self, n: usize) {
        let blank = self.blank();
        let cols = self.cols;
        let cy = self.screen().cy;
        let cx = self.screen().cx;
        let n = n.min(cols - cx);
        let s = self.screen_mut();
        let row = cy * cols;
        for x in cx..cols - n {
            s.cells[row + x] = s.cells[row + x + n];
        }
        for x in cols - n..cols {
            s.cells[row + x] = blank;
        }
        self.dirty = true;
    }
    fn insert_lines(&mut self, n: usize) {
        let top = self.screen().top;
        let bot = self.screen().bot;
        let cy = self.screen().cy;
        if cy < top || cy > bot {
            return;
        }
        let cols = self.cols;
        let n = n.min(bot - cy + 1);
        let blank = self.blank();
        let s = self.screen_mut();
        for y in (cy + n..=bot).rev() {
            let src = (y - n) * cols;
            let dst = y * cols;
            s.cells.copy_within(src..src + cols, dst);
        }
        for y in cy..cy + n {
            let row = y * cols;
            s.cells[row..row + cols].fill(blank);
        }
        self.dirty = true;
    }
    fn delete_lines(&mut self, n: usize) {
        let top = self.screen().top;
        let bot = self.screen().bot;
        let cy = self.screen().cy;
        if cy < top || cy > bot {
            return;
        }
        let cols = self.cols;
        let n = n.min(bot - cy + 1);
        let blank = self.blank();
        let s = self.screen_mut();
        for y in cy..=bot - n {
            let src = (y + n) * cols;
            let dst = y * cols;
            s.cells.copy_within(src..src + cols, dst);
        }
        for y in bot + 1 - n..=bot {
            let row = y * cols;
            s.cells[row..row + cols].fill(blank);
        }
        self.dirty = true;
    }
    fn switch_alt(&mut self, on: bool) {
        if self.alt == on {
            return;
        }
        self.alt = on;
        if on {
            let blank = self.blank();
            for c in &mut self.alternate.cells {
                *c = blank;
            }
            self.alternate.cx = 0;
            self.alternate.cy = 0;
            self.alternate.top = 0;
            self.alternate.bot = self.rows - 1;
        }
        self.dirty = true;
    }
    fn save_cursor(&mut self) {
        let (fg, bg, attrs) = (self.fg, self.bg, self.attrs);
        let s = self.screen_mut();
        s.saved_cx = s.cx;
        s.saved_cy = s.cy;
        s.saved_fg = fg;
        s.saved_bg = bg;
        s.saved_attrs = attrs;
    }
    fn restore_cursor(&mut self) {
        let (cx, cy, fg, bg, attrs) = {
            let s = self.screen();
            (s.saved_cx, s.saved_cy, s.saved_fg, s.saved_bg, s.saved_attrs)
        };
        let cols = self.cols;
        let rows = self.rows;
        let s = self.screen_mut();
        s.cx = cx.min(cols - 1);
        s.cy = cy.min(rows - 1);
        self.fg = fg;
        self.bg = bg;
        self.attrs = attrs;
    }
    fn sgr(&mut self, p: &[u16]) {
        let mut i = 0;
        if p.is_empty() {
            self.fg = NEON_CYAN;
            self.bg = BLACK;
            self.attrs = 0;
            return;
        }
        while i < p.len() {
            match p[i] {
                0 => {
                    self.fg = NEON_CYAN;
                    self.bg = BLACK;
                    self.attrs = 0;
                }
                1 => self.attrs |= ATTR_BOLD,
                4 => self.attrs |= ATTR_UNDERLINE,
                7 => self.attrs |= ATTR_REVERSE,
                22 => self.attrs &= !ATTR_BOLD,
                24 => self.attrs &= !ATTR_UNDERLINE,
                27 => self.attrs &= !ATTR_REVERSE,
                30..=37 => self.fg = ansi_color((p[i] - 30) as u8, false),
                40..=47 => self.bg = ansi_color((p[i] - 40) as u8, false),
                90..=97 => self.fg = ansi_color((p[i] - 90) as u8, true),
                100..=107 => self.bg = ansi_color((p[i] - 100) as u8, true),
                39 => self.fg = NEON_CYAN,
                49 => self.bg = BLACK,
                38 | 48 => {
                    let fg = p[i] == 38;
                    if i + 2 < p.len() && p[i + 1] == 5 {
                        let c = xterm256(p[i + 2] as u8);
                        if fg {
                            self.fg = c
                        } else {
                            self.bg = c
                        };
                        i += 2;
                    } else if i + 4 < p.len() && p[i + 1] == 2 {
                        let c = Color(
                            p[i + 2].min(255) as u8,
                            p[i + 3].min(255) as u8,
                            p[i + 4].min(255) as u8,
                        );
                        if fg {
                            self.fg = c
                        } else {
                            self.bg = c
                        };
                        i += 4;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
    fn move_cursor(&mut self, x: usize, y: usize) {
        let cols = self.cols;
        let rows = self.rows;
        let s = self.screen_mut();
        s.cx = x.min(cols - 1);
        s.cy = y.min(rows - 1);
    }
}

impl Perform for Grid {
    fn print(&mut self, c: char) {
        if self.cols == 0 || self.rows == 0 {
            return;
        }
        self.bytes_since_last = self.bytes_since_last.saturating_add(1);
        let w = UnicodeWidthChar::width(c).unwrap_or(1);
        if w == 0 {
            return;
        }
        let cols = self.cols;
        let cx = self.screen().cx;
        if cx + w > cols {
            self.newline();
        }
        let cy = self.screen().cy;
        let cx = self.screen().cx;
        let fg = self.fg;
        let bg = self.bg;
        let attrs = self.attrs | if w == 2 { ATTR_WIDE } else { 0 };
        let i = cy * cols + cx;
        {
            let s = self.screen_mut();
            s.cells[i] = Cell {
                ch: c,
                fg,
                bg,
                attrs,
            };
            if w == 2 && i + 1 < s.cells.len() {
                s.cells[i + 1] = Cell {
                    ch: ' ',
                    fg,
                    bg,
                    attrs: attrs | ATTR_CONT,
                };
            }
            s.cx += w;
            if s.cx > cols {
                s.cx = cols;
            }
        }
        self.dirty = true;
    }
    fn execute(&mut self, b: u8) {
        self.bytes_since_last = self.bytes_since_last.saturating_add(1);
        match b {
            b'\n' | 0x0b | 0x0c => self.newline(),
            b'\r' => self.screen_mut().cx = 0,
            b'\t' => {
                let cols = self.cols;
                let s = self.screen_mut();
                let next = ((s.cx / 8) + 1) * 8;
                s.cx = next.min(cols.saturating_sub(1));
            }
            0x08 => {
                let s = self.screen_mut();
                s.cx = s.cx.saturating_sub(1);
            }
            0x07 => {}
            _ => {}
        }
    }
    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _: bool, action: char) {
        let p: Vec<u16> = params.iter().flat_map(|s| s.iter().copied()).collect();
        let n_or = |i: usize, d: u16| p.get(i).copied().filter(|&v| v != 0).unwrap_or(d) as usize;
        let n1 = |i: usize| n_or(i, 1);
        let private = intermediates.first() == Some(&b'?');
        if private {
            match action {
                'h' => {
                    for &v in &p {
                        match v {
                            25 => self.cursor_visible = true,
                            47 | 1047 | 1049 => self.switch_alt(true),
                            _ => {}
                        }
                    }
                }
                'l' => {
                    for &v in &p {
                        match v {
                            25 => self.cursor_visible = false,
                            47 | 1047 | 1049 => self.switch_alt(false),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        match action {
            'H' | 'f' => self.move_cursor(n1(1).saturating_sub(1), n1(0).saturating_sub(1)),
            'A' => {
                let s = self.screen_mut();
                s.cy = s.cy.saturating_sub(n1(0));
            }
            'B' => {
                let rows = self.rows;
                let s = self.screen_mut();
                s.cy = (s.cy + n1(0)).min(rows - 1);
            }
            'C' => {
                let cols = self.cols;
                let s = self.screen_mut();
                s.cx = (s.cx + n1(0)).min(cols - 1);
            }
            'D' => {
                let s = self.screen_mut();
                s.cx = s.cx.saturating_sub(n1(0));
            }
            'E' => {
                let rows = self.rows;
                let s = self.screen_mut();
                s.cy = (s.cy + n1(0)).min(rows - 1);
                s.cx = 0;
            }
            'F' => {
                let s = self.screen_mut();
                s.cy = s.cy.saturating_sub(n1(0));
                s.cx = 0;
            }
            'G' | '`' => {
                let cols = self.cols;
                let s = self.screen_mut();
                s.cx = n1(0).saturating_sub(1).min(cols - 1);
            }
            'd' => {
                let rows = self.rows;
                let s = self.screen_mut();
                s.cy = n1(0).saturating_sub(1).min(rows - 1);
            }
            'J' => self.clear_screen(p.first().copied().unwrap_or(0)),
            'K' => self.clear_line(p.first().copied().unwrap_or(0)),
            'L' => self.insert_lines(n1(0)),
            'M' => self.delete_lines(n1(0)),
            'P' => self.delete_chars(n1(0)),
            '@' => self.insert_chars(n1(0)),
            'X' => self.erase_chars(n1(0)),
            'S' => self.scroll_up(n1(0)),
            'T' => self.scroll_down(n1(0)),
            'm' => self.sgr(&p),
            'r' => {
                let top = n_or(0, 1).saturating_sub(1);
                let bot = n_or(1, self.rows as u16).saturating_sub(1).min(self.rows - 1);
                let s = self.screen_mut();
                s.top = top;
                s.bot = bot.max(top);
                s.cx = 0;
                s.cy = 0;
            }
            's' => self.save_cursor(),
            'u' => self.restore_cursor(),
            _ => {}
        }
    }
    fn hook(&mut self, _: &Params, _: &[u8], _: bool, _: char) {}
    fn put(&mut self, _: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}
    fn esc_dispatch(&mut self, _: &[u8], _: bool, byte: u8) {
        match byte {
            b'7' => self.save_cursor(),
            b'8' => self.restore_cursor(),
            b'D' => {
                let bot = self.screen().bot;
                if self.screen().cy >= bot {
                    self.scroll_up(1);
                } else {
                    self.screen_mut().cy += 1;
                }
            }
            b'M' => {
                let top = self.screen().top;
                if self.screen().cy <= top {
                    self.scroll_down(1);
                } else {
                    self.screen_mut().cy -= 1;
                }
            }
            b'E' => {
                self.screen_mut().cx = 0;
                let bot = self.screen().bot;
                if self.screen().cy >= bot {
                    self.scroll_up(1);
                } else {
                    self.screen_mut().cy += 1;
                }
            }
            _ => {}
        }
    }
}

pub fn ansi_color(code: u8, bright: bool) -> Color {
    const N: [(u8, u8, u8); 8] = [
        (30, 30, 40),
        (220, 60, 100),
        (60, 220, 130),
        (240, 220, 80),
        (80, 140, 240),
        (200, 90, 240),
        (60, 220, 220),
        (200, 200, 220),
    ];
    const B: [(u8, u8, u8); 8] = [
        (80, 80, 80),
        (255, 80, 120),
        (100, 255, 150),
        (255, 240, 100),
        (100, 180, 255),
        (240, 120, 255),
        (100, 255, 240),
        (240, 240, 255),
    ];
    let v = if bright { B } else { N }[(code & 7) as usize];
    Color(v.0, v.1, v.2)
}
pub fn xterm256(i: u8) -> Color {
    match i {
        0..=7 => ansi_color(i, false),
        8..=15 => ansi_color(i - 8, true),
        16..=231 => {
            let n = i - 16;
            let f = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            Color(f(n / 36), f((n / 6) % 6), f(n % 6))
        }
        _ => {
            let v = 8 + 10 * (i - 232);
            Color(v, v, v)
        }
    }
}
