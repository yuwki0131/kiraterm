mod app;
mod font;
mod particles;
mod pty;
mod renderer;
mod vt;

use anyhow::Result;
use winit::event_loop::EventLoop;

fn main() -> Result<()> {
    env_logger::init();
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app::App::default())?;
    Ok(())
}
