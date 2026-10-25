//! One PTY per tile running `tmux attach` to that project's session. Output is
//! streamed to the webview as raw bytes over a Tauri channel. Closing a tile
//! kills only the attach client; the tmux session keeps running.

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, Emitter};

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

struct Pty {
    master: Box<dyn MasterPty + Send>,
    /// Own lock, so a blocked write to one PTY never holds up the others.
    writer: Writer,
    child: Box<dyn Child + Send + Sync>,
}

#[derive(Default)]
pub struct PtyManager {
    next: AtomicU32,
    ptys: Arc<Mutex<HashMap<u32, Pty>>>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyExit {
    pub id: u32,
    pub slug: String,
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(2),
        cols: cols.max(10),
        pixel_width: 0,
        pixel_height: 0,
    }
}

impl PtyManager {
    pub fn attach(
        &self,
        app: AppHandle,
        tmux: &crate::tmux::Tmux,
        slug: &str,
        cols: u16,
        rows: u16,
        on_data: Channel<Response>,
    ) -> Result<u32, String> {
        let pair = native_pty_system()
            .openpty(size(cols, rows))
            .map_err(|e| e.to_string())?;

        let base = tmux.command();
        let mut cmd = CommandBuilder::new(base.get_program());
        for a in base.get_args() {
            cmd.arg(a);
        }
        cmd.args(["attach-session", "-t", &format!("={slug}")]);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env_remove("TMUX");
        cmd.cwd(crate::env::home());

        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.ptys.lock().unwrap().insert(
            id,
            Pty {
                master: pair.master,
                writer: Arc::new(Mutex::new(writer)),
                child,
            },
        );

        let ptys = self.ptys.clone();
        let slug = slug.to_string();
        std::thread::Builder::new()
            .name(format!("pty-{id}"))
            .spawn(move || {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if on_data.send(Response::new(buf[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                    }
                }
                if let Some(mut p) = ptys.lock().unwrap().remove(&id) {
                    let _ = p.child.kill();
                    let _ = p.child.wait();
                }
                let _ = app.emit("pty-exit", PtyExit { id, slug });
            })
            .map_err(|e| e.to_string())?;
        Ok(id)
    }

    /// May block while the attach client is not reading; callers run it off
    /// the main thread. The map lock is released before writing.
    pub fn write(&self, id: u32, data: &[u8]) -> Result<(), String> {
        let writer = {
            let ptys = self.ptys.lock().unwrap();
            ptys.get(&id).ok_or("no such pty")?.writer.clone()
        };
        let mut w = writer.lock().unwrap();
        w.write_all(data)
            .and_then(|_| w.flush())
            .map_err(|e| e.to_string())
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        let ptys = self.ptys.lock().unwrap();
        let p = ptys.get(&id).ok_or("no such pty")?;
        p.master.resize(size(cols, rows)).map_err(|e| e.to_string())
    }

    /// Detach: kill the `tmux attach` client. The reader thread then sees EOF
    /// and cleans up.
    pub fn close(&self, id: u32) {
        if let Some(p) = self.ptys.lock().unwrap().get_mut(&id) {
            let _ = p.child.kill();
        }
    }

    pub fn close_all(&self) {
        for p in self.ptys.lock().unwrap().values_mut() {
            let _ = p.child.kill();
        }
    }
}
