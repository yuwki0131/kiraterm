use anyhow::Result;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver};

pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    pub reader_rx: Receiver<Vec<u8>>,
    _child: Box<dyn Child + Send + Sync>,
}

impl Pty {
    pub fn spawn(cols: u16, rows: u16) -> Result<Self> {
        let pair = native_pty_system().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
        let child = pair.slave.spawn_command(CommandBuilder::new(shell))?;
        drop(pair.slave);
        let writer = pair.master.take_writer()?;
        let mut reader = pair.master.try_clone_reader()?;
        let (tx, reader_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            master: pair.master,
            writer,
            reader_rx,
            _child: child,
        })
    }

    pub fn write(&mut self, data: &[u8]) {
        if let Err(e) = self
            .writer
            .write_all(data)
            .and_then(|_| self.writer.flush())
        {
            log::warn!("PTY write: {e}");
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        if let Err(e) = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            log::warn!("PTY resize: {e}");
        }
    }
}
