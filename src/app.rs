use crate::{
    font::find_fonts,
    pty::Pty,
    renderer::{Overlay, Renderer},
    vt::Grid,
};
use std::{
    sync::mpsc::TryRecvError,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, Ime, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowAttributes, WindowId},
};

pub struct State {
    pub window: Arc<Window>,
    pub renderer: Renderer,
    pub grid: Grid,
    pub parser: vte::Parser,
    pub pty: Pty,
    pub glitch: f32,
    pub preediting: bool,
    pub executing: bool,
    pub exec_start: Instant,
    pub exec_last_output: Option<Instant>,
    pub exec_had_output: bool,
    pub exec_bytes_accum: u32,
    pub exec_phase: f32,
    last_tick: Instant,
    render_dt: f32,
}
#[derive(Default)]
pub struct App {
    state: Option<State>,
    mods: ModifiersState,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        match self.init(el) {
            Ok(s) => self.state = Some(s),
            Err(e) => {
                log::error!("startup failed: {e:#}");
                el.exit();
            }
        }
    }
    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(s) = self.state.as_mut() else { return };
        if id != s.window.id() {
            return;
        }
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(z) => {
                s.renderer.resize(z);
                let (c, r) = dims(z.width, z.height, s.renderer.cell_size());
                s.grid.resize(c, r);
                s.pty.resize(c as u16, r as u16);
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !s.preediting =>
            {
                handle_key(s, &event.logical_key, self.mods)
            }
            WindowEvent::Ime(ev) => match ev {
                Ime::Enabled | Ime::Disabled => {
                    s.preediting = false;
                }
                Ime::Preedit(text, _) => {
                    s.preediting = !text.is_empty();
                }
                Ime::Commit(text) => {
                    s.preediting = false;
                    if !text.is_empty() {
                        s.pty.write(text.as_bytes());
                        let (cw, ch) = s.renderer.cell_size();
                        s.renderer.particles.emit(
                            [
                                s.grid.cx() as f32 * cw + cw / 2.0,
                                s.grid.cy() as f32 * ch + ch / 2.0,
                            ],
                            [1.0, 0.7, 0.2],
                            (text.chars().count() * 6).min(48),
                        );
                        s.glitch = (s.glitch + 0.5).min(1.0);
                    }
                }
            },
            WindowEvent::RedrawRequested => {
                if let Err(e) = s.renderer.render(&s.grid, s.glitch, s.render_dt) {
                    log::warn!("render: {e}");
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(s) = self.state.as_mut() else { return };
        s.grid.bytes_since_last = 0;
        loop {
            match s.pty.reader_rx.try_recv() {
                Ok(bytes) => {
                    for b in bytes {
                        s.parser.advance(&mut s.grid, b);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    el.exit();
                    return;
                }
            }
        }
        let now = Instant::now();
        if s.grid.bytes_since_last > 0 {
            let (cw, ch) = s.renderer.cell_size();
            let burst = (s.grid.bytes_since_last as f32 / 40.0).min(2.0);
            s.glitch = (s.glitch + 0.15 + 0.15 * burst).min(1.0);
            let n = (2.0 + burst * 6.0) as usize;
            s.renderer.particles.emit(
                [
                    s.grid.cx() as f32 * cw + cw / 2.0,
                    s.grid.cy() as f32 * ch + ch / 2.0,
                ],
                [1.0, 0.6, 0.9],
                n,
            );
            if s.executing {
                // ignore the raw enter echo — only start counting "real" output
                // once we've seen more than a couple of bytes or a bit of time
                // has passed. otherwise `sleep 5` would end the spinner ~250ms
                // after enter (fooled by the \n echo).
                s.exec_bytes_accum = s
                    .exec_bytes_accum
                    .saturating_add(s.grid.bytes_since_last);
                let since_enter = now.duration_since(s.exec_start).as_millis();
                if s.exec_bytes_accum > 4 || since_enter > 400 {
                    s.exec_had_output = true;
                    s.exec_last_output = Some(now);
                }
            }
        }
        // Loading/executing effect: pulses glitch and streams particles from
        // the window edges until the child has quiesced (or a TUI takes over).
        if s.executing {
            update_executing(s, now);
        }
        let dt = (now - s.last_tick).as_secs_f32().min(0.05);
        s.last_tick = now;
        s.render_dt = dt;
        s.renderer.particles.update(dt);
        s.glitch = (s.glitch - dt * 2.0).max(0.0);
        s.window.request_redraw();
    }
}
impl App {
    fn init(&self, el: &ActiveEventLoop) -> anyhow::Result<State> {
        let attrs = WindowAttributes::default()
            .with_title("kiraterm")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 800));
        let window = Arc::new(el.create_window(attrs)?);
        window.set_ime_allowed(true);
        let size = window.inner_size();
        let fonts = find_fonts()?;
        let renderer = pollster::block_on(Renderer::new(window.clone(), fonts))?;
        let (c, r) = dims(size.width, size.height, renderer.cell_size());
        Ok(State {
            window,
            renderer,
            grid: Grid::new(c, r),
            parser: vte::Parser::new(),
            pty: Pty::spawn(c as u16, r as u16)?,
            glitch: 0.0,
            preediting: false,
            executing: false,
            exec_start: Instant::now(),
            exec_last_output: None,
            exec_had_output: false,
            exec_bytes_accum: 0,
            exec_phase: 0.0,
            last_tick: Instant::now(),
            render_dt: 0.0,
        })
    }
}
fn dims(w: u32, h: u32, cell: (f32, f32)) -> (usize, usize) {
    (
        (w as f32 / cell.0).floor().max(1.0) as usize,
        (h as f32 / cell.1).floor().max(1.0) as usize,
    )
}
const SPINNER: &[char] = &['⣷', '⣯', '⣟', '⡿', '⢿', '⣻', '⣽', '⣾'];

fn update_executing(s: &mut State, now: Instant) {
    // TUIs (vim, less, claude code, htop) take over via the alternate screen
    // buffer; a loading badge on top of them just gets in the way.
    if s.grid.is_alt() {
        s.executing = false;
        s.renderer.overlay = None;
        return;
    }
    let quiet_ms = 250u128;
    let safety = Duration::from_secs(600);
    let done = if let Some(last) = s.exec_last_output {
        s.exec_had_output && now.duration_since(last).as_millis() >= quiet_ms
    } else {
        false
    };
    if done || now.duration_since(s.exec_start) >= safety {
        s.executing = false;
        s.renderer.overlay = None;
        return;
    }
    let elapsed = now.duration_since(s.exec_start).as_secs_f32();
    s.exec_phase = elapsed;
    // pulsing glitch — a sine floor so it never fully settles while waiting.
    let pulse = 0.5 + 0.5 * (elapsed * 3.0).sin();
    s.glitch = s.glitch.max(0.25 + 0.25 * pulse);
    // rotating braille spinner in the top-right corner, with elapsed time.
    let idx = ((elapsed * 10.0) as usize) % SPINNER.len();
    let spinner = SPINNER[idx];
    let color_pulse = 0.75 + 0.25 * (elapsed * 4.0).sin();
    let text = if elapsed >= 1.0 {
        format!("{spinner} 実行中… {:.1}s", elapsed)
    } else {
        format!("{spinner} 実行中…")
    };
    s.renderer.overlay = Some(Overlay {
        text,
        color: [1.0, 0.85 * color_pulse, 0.25 * color_pulse, 1.0],
        scale: 1.6,
    });
}

fn handle_key(s: &mut State, key: &Key, mods: ModifiersState) {
    let mut out = match key {
        Key::Named(n) => match n {
            NamedKey::Enter => b"\r".to_vec(),
            NamedKey::Backspace => b"\x7f".to_vec(),
            NamedKey::Tab => b"\t".to_vec(),
            NamedKey::Escape => b"\x1b".to_vec(),
            NamedKey::Space => b" ".to_vec(),
            NamedKey::ArrowUp => b"\x1b[A".to_vec(),
            NamedKey::ArrowDown => b"\x1b[B".to_vec(),
            NamedKey::ArrowRight => b"\x1b[C".to_vec(),
            NamedKey::ArrowLeft => b"\x1b[D".to_vec(),
            NamedKey::Home => b"\x1b[H".to_vec(),
            NamedKey::End => b"\x1b[F".to_vec(),
            NamedKey::PageUp => b"\x1b[5~".to_vec(),
            NamedKey::PageDown => b"\x1b[6~".to_vec(),
            NamedKey::Delete => b"\x1b[3~".to_vec(),
            _ => return,
        },
        Key::Character(v) => {
            let mut b = if mods.control_key() {
                let c = v.chars().next().unwrap_or('\0').to_ascii_uppercase();
                if c.is_ascii_uppercase() {
                    vec![c as u8 - b'A' + 1]
                } else {
                    return;
                }
            } else {
                v.as_bytes().to_vec()
            };
            if mods.alt_key() {
                b.insert(0, 0x1b)
            }
            b
        }
        _ => return,
    };
    let is_enter = matches!(key, Key::Named(NamedKey::Enter))
        || matches!(key, Key::Character(v) if v.as_ref() == "\r" || v.as_ref() == "\n");
    s.pty.write(&out);
    out.clear();
    let (cw, ch) = s.renderer.cell_size();
    s.renderer.particles.emit(
        [
            s.grid.cx() as f32 * cw + cw / 2.0,
            s.grid.cy() as f32 * ch + ch / 2.0,
        ],
        [0.0, 1.0, 0.9],
        12,
    );
    s.glitch = (s.glitch + 0.6).min(1.0);
    if is_enter && !s.grid.is_alt() {
        s.executing = true;
        s.exec_start = Instant::now();
        s.exec_last_output = None;
        s.exec_had_output = false;
        s.exec_bytes_accum = 0;
        s.exec_phase = 0.0;
        // paint the spinner immediately so short commands still flash it.
        update_executing(s, Instant::now());
    }
}
