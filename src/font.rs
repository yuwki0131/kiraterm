use anyhow::{anyhow, Result};
use fontdb::{Database, Family, Query, Stretch, Style, Weight};
use fontdue::Font;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub struct GlyphInfo {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub xmin: i32,
    pub ymin: i32,
    pub advance: f32,
}

pub fn find_font() -> Result<Vec<u8>> {
    let mut db = Database::new();
    db.load_system_fonts();
    let names = [
        "JetBrainsMono Nerd Font",
        "JetBrains Mono",
        "Fira Code",
        "FiraCode Nerd Font",
        "Hack",
        "Source Code Pro",
        "DejaVu Sans Mono",
        "Liberation Mono",
        "Ubuntu Mono",
    ];
    let mut id = names.iter().find_map(|name| {
        db.query(&Query {
            families: &[Family::Name(name)],
            weight: Weight::NORMAL,
            stretch: Stretch::Normal,
            style: Style::Normal,
        })
    });
    if id.is_none() {
        id = db.query(&Query {
            families: &[Family::Monospace],
            weight: Weight::NORMAL,
            stretch: Stretch::Normal,
            style: Style::Normal,
        });
    }
    if id.is_none() {
        id = db.faces().next().map(|f| f.id);
    }
    let id = id.ok_or_else(|| anyhow!("no system font found"))?;
    db.with_face_data(id, |data, _| data.to_vec())
        .ok_or_else(|| anyhow!("could not read font data"))
}

pub struct Atlas {
    pub font: Font,
    pub px_size: f32,
    pub bitmap: Vec<u8>,
    pub width: u32,
    pub height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_h: u32,
    pub glyphs: HashMap<char, GlyphInfo>,
    pub dirty: bool,
    pub cell_w: f32,
    pub cell_h: f32,
    pub baseline: f32,
}

impl Atlas {
    pub fn new(bytes: Vec<u8>, px_size: f32) -> Result<Self> {
        let font =
            Font::from_bytes(bytes, fontdue::FontSettings::default()).map_err(|e| anyhow!(e))?;
        let lm = font
            .horizontal_line_metrics(px_size)
            .ok_or_else(|| anyhow!("font has no horizontal metrics"))?;
        let mut a = Self {
            cell_w: font.metrics('M', px_size).advance_width.ceil(),
            cell_h: (lm.ascent - lm.descent + lm.line_gap).ceil(),
            baseline: lm.ascent,
            font,
            px_size,
            bitmap: vec![0; 1024 * 1024],
            width: 1024,
            height: 1024,
            cursor_x: 0,
            cursor_y: 0,
            row_h: 0,
            glyphs: HashMap::new(),
            dirty: true,
        };
        for b in 32u8..128 {
            a.rasterize(b as char);
        }
        Ok(a)
    }
    pub fn get(&mut self, ch: char) -> GlyphInfo {
        if !self.glyphs.contains_key(&ch) {
            self.rasterize(ch);
        }
        self.glyphs[&ch]
    }
    fn rasterize(&mut self, ch: char) {
        let (m, pixels) = self.font.rasterize(ch, self.px_size);
        let mut info = GlyphInfo {
            x: 0,
            y: 0,
            width: m.width as u32,
            height: m.height as u32,
            xmin: m.xmin,
            ymin: m.ymin,
            advance: m.advance_width,
        };
        if m.width > 0 && m.height > 0 {
            if self.cursor_x + info.width + 1 > self.width {
                self.cursor_x = 0;
                self.cursor_y += self.row_h + 1;
                self.row_h = 0;
            }
            if self.cursor_y + info.height <= self.height {
                info.x = self.cursor_x;
                info.y = self.cursor_y;
                for y in 0..info.height {
                    let dst = ((info.y + y) * self.width + info.x) as usize;
                    let src = (y * info.width) as usize;
                    self.bitmap[dst..dst + info.width as usize]
                        .copy_from_slice(&pixels[src..src + info.width as usize]);
                }
                self.cursor_x += info.width + 1;
                self.row_h = self.row_h.max(info.height);
                self.dirty = true;
            } else {
                info.width = 0;
                info.height = 0;
            }
        }
        self.glyphs.insert(ch, info);
    }
}
