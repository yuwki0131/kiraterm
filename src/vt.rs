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
}
pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Cell>,
    pub cx: usize,
    pub cy: usize,
    pub fg: Color,
    pub bg: Color,
    pub dirty: bool,
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            cells: vec![
                Cell {
                    ch: ' ',
                    fg: NEON_CYAN,
                    bg: BLACK
                };
                cols * rows
            ],
            cx: 0,
            cy: 0,
            fg: NEON_CYAN,
            bg: BLACK,
            dirty: true,
        }
    }
    fn blank(&self) -> Cell {
        Cell {
            ch: ' ',
            fg: self.fg,
            bg: self.bg,
        }
    }
    fn newline(&mut self) {
        self.cx = 0;
        if self.cy + 1 >= self.rows {
            self.scroll_up();
        } else {
            self.cy += 1;
        }
    }
    fn scroll_up(&mut self) {
        if self.rows == 0 {
            return;
        }
        self.cells.copy_within(self.cols.., 0);
        let blank = self.blank();
        let from = (self.rows - 1) * self.cols;
        self.cells[from..].fill(blank);
        self.dirty = true;
    }
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let mut cells = vec![self.blank(); cols * rows];
        for y in 0..rows.min(self.rows) {
            for x in 0..cols.min(self.cols) {
                cells[y * cols + x] = self.cells[y * self.cols + x];
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.cells = cells;
        self.cx = self.cx.min(cols - 1);
        self.cy = self.cy.min(rows - 1);
        self.dirty = true;
    }
    fn clear_screen(&mut self, mode: u16) {
        let blank = self.blank();
        let p = self.cy * self.cols + self.cx;
        match mode {
            0 => self.cells[p..].fill(blank),
            1 => self.cells[..=p].fill(blank),
            2 | 3 => self.cells.fill(blank),
            _ => {}
        }
        self.dirty = true;
    }
    fn clear_line(&mut self, mode: u16) {
        let blank = self.blank();
        let row = self.cy * self.cols;
        match mode {
            0 => self.cells[row + self.cx..row + self.cols].fill(blank),
            1 => self.cells[row..=row + self.cx].fill(blank),
            2 => self.cells[row..row + self.cols].fill(blank),
            _ => {}
        }
        self.dirty = true;
    }
    fn sgr(&mut self, p: &[u16]) {
        let mut i = 0;
        if p.is_empty() {
            self.fg = NEON_CYAN;
            self.bg = BLACK;
            return;
        }
        while i < p.len() {
            match p[i] {
                0 => {
                    self.fg = NEON_CYAN;
                    self.bg = BLACK;
                }
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
}

impl Perform for Grid {
    fn print(&mut self, c: char) {
        if self.cols == 0 || self.rows == 0 {
            return;
        }
        let i = self.cy * self.cols + self.cx;
        self.cells[i] = Cell {
            ch: c,
            fg: self.fg,
            bg: self.bg,
        };
        self.cx += 1;
        if self.cx >= self.cols {
            self.newline();
        }
        self.dirty = true;
    }
    fn execute(&mut self, b: u8) {
        match b {
            b'\n' => self.newline(),
            b'\r' => self.cx = 0,
            b'\t' => {
                let next = ((self.cx / 8) + 1) * 8;
                self.cx = next.min(self.cols.saturating_sub(1));
            }
            0x08 => self.cx = self.cx.saturating_sub(1),
            _ => {}
        }
    }
    fn csi_dispatch(&mut self, params: &Params, _: &[u8], _: bool, action: char) {
        let p: Vec<u16> = params.iter().flat_map(|s| s.iter().copied()).collect();
        let n = |i: usize| p.get(i).copied().unwrap_or(1).max(1) as usize;
        match action {
            'H' | 'f' => {
                self.cy = n(0).saturating_sub(1).min(self.rows - 1);
                self.cx = n(1).saturating_sub(1).min(self.cols - 1);
            }
            'A' => self.cy = self.cy.saturating_sub(n(0)),
            'B' => self.cy = (self.cy + n(0)).min(self.rows - 1),
            'C' => self.cx = (self.cx + n(0)).min(self.cols - 1),
            'D' => self.cx = self.cx.saturating_sub(n(0)),
            'J' => self.clear_screen(p.first().copied().unwrap_or(0)),
            'K' => self.clear_line(p.first().copied().unwrap_or(0)),
            'm' => self.sgr(&p),
            _ => {}
        }
    }
    fn hook(&mut self, _: &Params, _: &[u8], _: bool, _: char) {}
    fn put(&mut self, _: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}
    fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
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
