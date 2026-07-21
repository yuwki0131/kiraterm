use anyhow::{anyhow, Result};
use fontdb::{Database, Family, Query, Stretch, Style, Weight};
use fontdue::{Font, FontSettings};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub struct GlyphInfo {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub xmin: i32,
    pub ymin: i32,
    pub font_ix: u8,
}

pub struct FontBlob {
    pub bytes: Vec<u8>,
    pub index: u32,
}

pub fn find_fonts() -> Result<Vec<FontBlob>> {
    let mut db = Database::new();
    db.load_system_fonts();
    let primary = [
        "HackGen Console",
        "HackGen",
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
    let cjk = [
        "HackGen Console",
        "HackGen",
        "Noto Sans Mono CJK JP",
        "Noto Sans CJK JP",
        "IPAGothic",
        "TakaoGothic",
        "Source Han Code JP",
        "Sarasa Mono J",
    ];
    // symbols / braille / geometric shapes fallback — many programming TUIs
    // (spinners in claude code, ⏵ / ▶ style indicators) depend on these blocks.
    let symbols = [
        "Noto Sans Symbols 2",
        "Noto Sans Symbols",
        "DejaVu Sans Mono",
        "DejaVu Sans",
        "Symbola",
        "Unifont",
    ];
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |db: &Database, name: &str, out: &mut Vec<FontBlob>, seen: &mut std::collections::HashSet<String>| {
        if seen.contains(name) {
            return;
        }
        if let Some(id) = db.query(&Query {
            families: &[Family::Name(name)],
            weight: Weight::NORMAL,
            stretch: Stretch::Normal,
            style: Style::Normal,
        }) {
            if let Some((bytes, index)) = db.with_face_data(id, |data, ix| (data.to_vec(), ix)) {
                out.push(FontBlob { bytes, index });
                seen.insert(name.to_string());
            }
        }
    };
    for name in primary.iter().chain(cjk.iter()).chain(symbols.iter()) {
        push(&db, name, &mut out, &mut seen);
    }
    if out.is_empty() {
        if let Some(id) = db.query(&Query {
            families: &[Family::Monospace],
            weight: Weight::NORMAL,
            stretch: Stretch::Normal,
            style: Style::Normal,
        }) {
            if let Some((bytes, index)) = db.with_face_data(id, |data, ix| (data.to_vec(), ix)) {
                out.push(FontBlob { bytes, index });
            }
        }
    }
    if out.is_empty() {
        if let Some(id) = db.faces().next().map(|f| f.id) {
            if let Some((bytes, index)) = db.with_face_data(id, |data, ix| (data.to_vec(), ix)) {
                out.push(FontBlob { bytes, index });
            }
        }
    }
    if out.is_empty() {
        return Err(anyhow!("no system font found"));
    }
    Ok(out)
}

pub struct Atlas {
    pub fonts: Vec<Font>,
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
    pub fn new(blobs: Vec<FontBlob>, px_size: f32) -> Result<Self> {
        let mut fonts = Vec::new();
        for b in blobs {
            let f = Font::from_bytes(
                b.bytes,
                FontSettings {
                    collection_index: b.index,
                    ..FontSettings::default()
                },
            )
            .map_err(|e| anyhow!(e))?;
            fonts.push(f);
        }
        let primary = &fonts[0];
        let lm = primary
            .horizontal_line_metrics(px_size)
            .ok_or_else(|| anyhow!("font has no horizontal metrics"))?;
        let cell_w = primary.metrics('M', px_size).advance_width.ceil();
        let cell_h = (lm.ascent - lm.descent + lm.line_gap).ceil();
        let baseline = lm.ascent;
        let mut a = Self {
            fonts,
            px_size,
            bitmap: vec![0; 1024 * 1024],
            width: 1024,
            height: 1024,
            cursor_x: 0,
            cursor_y: 0,
            row_h: 0,
            glyphs: HashMap::new(),
            dirty: true,
            cell_w,
            cell_h,
            baseline,
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
    fn pick_font(&self, ch: char) -> u8 {
        for (i, f) in self.fonts.iter().enumerate() {
            if f.lookup_glyph_index(ch) != 0 {
                return i as u8;
            }
        }
        0
    }
    fn rasterize(&mut self, ch: char) {
        let ix = self.pick_font(ch);
        let (m, pixels) = self.fonts[ix as usize].rasterize(ch, self.px_size);
        let mut info = GlyphInfo {
            x: 0,
            y: 0,
            width: m.width as u32,
            height: m.height as u32,
            xmin: m.xmin,
            ymin: m.ymin,
            font_ix: ix,
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
