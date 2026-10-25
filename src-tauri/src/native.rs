//! Sessions held by desk itself, for Windows (no tmux there): each session is
//! a process in a pseudo terminal (ConPTY) owned by this process. Tiles attach
//! and detach like with tmux: an attach replays the recent output, then
//! streams. Sessions end when desk quits.
//!
//! The same code runs on macOS/Linux (portable-pty), so it is tested there.

use crate::tmux::SessionInfo;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Output kept per session for a tile that attaches later.
const SCROLLBACK: usize = 2 * 1024 * 1024;

/// Receives output; returns false when the viewer is gone.
pub type Sink = Box<dyn Fn(Vec<u8>) -> bool + Send>;
/// Called with the slug and the viewer ids when a session's process exits.
pub type OnExit = Arc<dyn Fn(&str, Vec<u32>) + Send + Sync>;

struct Shared {
    scrollback: Vec<u8>,
    sinks: HashMap<u32, Sink>,
}

struct Session {
    info: SessionInfo,
    /// Taken (closing the pseudo console) when the process exits: on Windows
    /// the output pipe only ends then.
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    shared: Mutex<Shared>,
}

pub struct Native {
    sessions: Arc<Mutex<HashMap<String, Arc<Session>>>>,
    /// Viewer id -> session slug.
    viewers: Arc<Mutex<HashMap<u32, String>>>,
    next: AtomicU32,
    on_exit: OnExit,
    /// Held from the "exists?" check to the insert, so two opens of the same
    /// session cannot both start a process.
    starting: Mutex<()>,
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(2),
        cols: cols.max(10),
        pixel_width: 0,
        pixel_height: 0,
    }
}

impl Native {
    pub fn new(on_exit: OnExit) -> Self {
        Self {
            sessions: Arc::default(),
            viewers: Arc::default(),
            next: AtomicU32::new(0),
            on_exit,
            starting: Mutex::new(()),
        }
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        let mut v: Vec<SessionInfo> = self
            .sessions
            .lock()
            .unwrap()
            .values()
            .map(|s| s.info.clone())
            .collect();
        v.sort_by(|a, b| a.slug.cmp(&b.slug));
        v
    }

    pub fn has(&self, slug: &str) -> bool {
        self.sessions.lock().unwrap().contains_key(slug)
    }

    /// Start `argv` in `cwd` as session `slug`.
    pub fn start(
        &self,
        slug: &str,
        cwd: &Path,
        argv: &[String],
        env: &[(&str, String)],
        agent: &str,
        instance: u32,
    ) -> Result<(), String> {
        let _starting = self.starting.lock().unwrap();
        if self.has(slug) {
            return Err(format!("session {slug} already exists"));
        }
        let (program, args) = argv.split_first().ok_or("empty command")?;
        let pair = native_pty_system()
            .openpty(size(200, 50))
            .map_err(|e| e.to_string())?;
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for (k, v) in env {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let session = Arc::new(Session {
            info: SessionInfo {
                slug: slug.to_string(),
                path: cwd.display().to_string(),
                agent: agent.to_string(),
                instance,
            },
            master: Mutex::new(Some(pair.master)),
            writer: Mutex::new(Some(writer)),
            child: Mutex::new(child),
            shared: Mutex::new(Shared {
                scrollback: Vec::new(),
                sinks: HashMap::new(),
            }),
        });
        self.sessions
            .lock()
            .unwrap()
            .insert(slug.to_string(), session.clone());

        let out = session.clone();
        std::thread::Builder::new()
            .name(format!("session-out-{slug}"))
            .spawn(move || {
                let mut buf = vec![0u8; 64 * 1024];
                // ConPTY asks for the cursor position once at start and waits for
                // the answer. Answer it here: no tile may be open yet, and a reply
                // from a tile replaying the scrollback later would be typed in.
                let mut startup_query = cfg!(windows);
                while let Ok(mut n @ 1..) = reader.read(&mut buf) {
                    if startup_query && let Some(at) = find(&buf[..n], CURSOR_QUERY) {
                        startup_query = false;
                        buf.copy_within(at + CURSOR_QUERY.len()..n, at);
                        n -= CURSOR_QUERY.len();
                        if let Some(w) = out.writer.lock().unwrap().as_mut() {
                            let _ = w.write_all(b"\x1b[1;1R");
                        }
                        if n == 0 {
                            continue;
                        }
                    }
                    let mut sh = out.shared.lock().unwrap();
                    sh.scrollback.extend_from_slice(&buf[..n]);
                    if sh.scrollback.len() > SCROLLBACK {
                        let cut = sh.scrollback.len() - SCROLLBACK;
                        sh.scrollback.drain(..cut);
                    }
                    sh.sinks.retain(|_, sink| sink(buf[..n].to_vec()));
                }
            })
            .map_err(|e| e.to_string())?;

        // Watch the process (not the output, which on Windows stays open).
        let sessions = self.sessions.clone();
        let viewers = self.viewers.clone();
        let on_exit = self.on_exit.clone();
        let slug = slug.to_string();
        std::thread::Builder::new()
            .name(format!("session-wait-{slug}"))
            .spawn(move || {
                loop {
                    let exited = session.child.lock().unwrap().try_wait();
                    if !matches!(exited, Ok(None)) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
                std::thread::sleep(Duration::from_millis(150)); // last output
                session.writer.lock().unwrap().take();
                session.master.lock().unwrap().take();
                sessions.lock().unwrap().remove(&slug);
                let ids: Vec<u32> = session
                    .shared
                    .lock()
                    .unwrap()
                    .sinks
                    .drain()
                    .map(|(id, _)| id)
                    .collect();
                viewers.lock().unwrap().retain(|_, s| *s != slug);
                on_exit(&slug, ids);
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn kill(&self, slug: &str) -> Result<(), String> {
        let s = self
            .sessions
            .lock()
            .unwrap()
            .get(slug)
            .cloned()
            .ok_or_else(|| format!("no session {slug}"))?;
        s.child.lock().unwrap().kill().map_err(|e| e.to_string())
    }

    pub fn kill_all(&self) {
        let all: Vec<Arc<Session>> = self.sessions.lock().unwrap().values().cloned().collect();
        for s in all {
            let _ = s.child.lock().unwrap().kill();
        }
    }

    /// Show the session in a tile: the recent output first, then live output.
    pub fn attach(&self, slug: &str, cols: u16, rows: u16, sink: Sink) -> Result<u32, String> {
        let s = self.session(slug)?;
        if let Some(m) = s.master.lock().unwrap().as_ref() {
            let _ = m.resize(size(cols, rows));
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let mut sh = s.shared.lock().unwrap();
        if !sh.scrollback.is_empty() && !sink(sh.scrollback.clone()) {
            return Err("viewer closed".into());
        }
        sh.sinks.insert(id, sink);
        self.viewers.lock().unwrap().insert(id, slug.to_string());
        Ok(id)
    }

    /// Close a tile; the session keeps running.
    pub fn detach(&self, id: u32) {
        if let Some(slug) = self.viewers.lock().unwrap().remove(&id)
            && let Ok(s) = self.session(&slug)
        {
            s.shared.lock().unwrap().sinks.remove(&id);
        }
    }

    pub fn write(&self, id: u32, data: &[u8]) -> Result<(), String> {
        let s = self.session_of(id)?;
        let mut guard = s.writer.lock().unwrap();
        let w = guard.as_mut().ok_or("session ended")?;
        w.write_all(data)
            .and_then(|_| w.flush())
            .map_err(|e| e.to_string())
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        let s = self.session_of(id)?;
        let guard = s.master.lock().unwrap();
        let m = guard.as_ref().ok_or("session ended")?;
        m.resize(size(cols, rows)).map_err(|e| e.to_string())
    }

    fn session(&self, slug: &str) -> Result<Arc<Session>, String> {
        self.sessions
            .lock()
            .unwrap()
            .get(slug)
            .cloned()
            .ok_or_else(|| format!("no session {slug}"))
    }

    fn session_of(&self, id: u32) -> Result<Arc<Session>, String> {
        let slug = self
            .viewers
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or("no such pty")?;
        self.session(&slug)
    }
}

/// Device Status Report: "where is the cursor?"
const CURSOR_QUERY: &[u8] = b"\x1b[6n";

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn collect(rx: &mpsc::Receiver<Vec<u8>>, until: &str) -> String {
        let mut out = String::new();
        while !out.contains(until) {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(b) => out.push_str(&String::from_utf8_lossy(&b)),
                Err(_) => panic!("timed out waiting for {until:?}; got {out:?}"),
            }
        }
        out
    }

    /// A shell that prints "start", then echoes what is typed (sh, or cmd.exe on Windows).
    fn echo_shell() -> (Vec<String>, &'static [u8]) {
        if cfg!(windows) {
            (
                ["cmd.exe", "/Q", "/K", "echo start"]
                    .map(String::from)
                    .to_vec(),
                b"echo hello\r\n",
            )
        } else {
            (
                ["sh", "-c", "echo start; cat"].map(String::from).to_vec(),
                b"hello\n",
            )
        }
    }

    fn long_running() -> Vec<String> {
        if cfg!(windows) {
            ["ping", "-n", "30", "127.0.0.1"].map(String::from).to_vec()
        } else {
            ["sh", "-c", "sleep 30"].map(String::from).to_vec()
        }
    }

    fn sink(tx: mpsc::Sender<Vec<u8>>) -> Sink {
        Box::new(move |b| tx.send(b).is_ok())
    }

    #[test]
    fn sessions_outlive_tiles_and_replay_output() {
        let (exit_tx, exit_rx) = mpsc::channel();
        let exit_tx = Mutex::new(exit_tx);
        let n = Native::new(Arc::new(move |slug, ids| {
            let _ = exit_tx.lock().unwrap().send((slug.to_string(), ids));
        }));
        let tmp = tempfile::tempdir().unwrap();
        let (argv, typed) = echo_shell();
        n.start(
            "s1",
            tmp.path(),
            &argv,
            &[("DESK_X", "1".into())],
            "shell",
            2,
        )
        .unwrap();
        let info = &n.list()[0];
        assert_eq!((info.agent.as_str(), info.instance), ("shell", 2));

        // First tile: sees output, types, closes. The session keeps running.
        let (tx, rx) = mpsc::channel();
        let a = n.attach("s1", 80, 24, sink(tx)).unwrap();
        collect(&rx, "start");
        n.write(a, typed).unwrap();
        collect(&rx, "hello");
        n.detach(a);
        assert!(n.has("s1"));

        // Second tile: gets the earlier output replayed, then live output.
        let (tx, rx) = mpsc::channel();
        let b = n.attach("s1", 100, 30, sink(tx)).unwrap();
        let replay = collect(&rx, "hello");
        assert!(replay.contains("start"));
        n.resize(b, 120, 40).unwrap();
        assert!(n.write(a, b"x").is_err(), "closed tile cannot type");

        n.kill("s1").unwrap();
        let (slug, ids) = exit_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!((slug.as_str(), ids), ("s1", vec![b]));
        assert!(!n.has("s1"));
        assert!(n.attach("s1", 80, 24, Box::new(|_| true)).is_err());
    }

    #[test]
    fn the_same_session_starts_once() {
        let n = Arc::new(Native::new(Arc::new(|_, _| {})));
        let tmp = tempfile::tempdir().unwrap();
        let argv = long_running();
        let ok: usize = std::thread::scope(|s| {
            let runs: Vec<_> = (0..8)
                .map(|_| s.spawn(|| n.start("dup", tmp.path(), &argv, &[], "shell", 0).is_ok()))
                .collect();
            runs.into_iter()
                .map(|r| usize::from(r.join().unwrap()))
                .sum()
        });
        assert_eq!(ok, 1);
        n.kill_all();
    }
}
