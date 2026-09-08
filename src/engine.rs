// SPDX-License-Identifier: GPL-3.0-or-later
//
// Stdlib-managed UCI engine subprocess. A dedicated thread reads the engine's
// stdout into an mpsc channel so the UI can poll non-blocking for the
// bestmove while the engine thinks. The engine is located from the
// STOCKFISH_PATH override, then from an executable-sibling binary; PATH is
// intentionally not consulted.
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const READY_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const SEARCH_TIMEOUT: Duration = Duration::from_secs(10);
const MOVE_TIME_MS: u32 = 1000;

#[derive(Debug)]
pub(crate) enum EngineError {
    NotFound,
    Spawn(std::io::Error),
    Startup(String),
    Ready(String),
    Protocol(String),
}

impl EngineError {
    pub(crate) fn message(&self) -> String {
        match self {
            EngineError::NotFound => {
                "engine not found (set STOCKFISH_PATH or place stockfish next to the binary)".into()
            }
            EngineError::Spawn(err) => format!("could not start engine: {err}"),
            EngineError::Startup(detail) => format!("engine startup failed: {detail}"),
            EngineError::Ready(detail) => format!("engine not ready: {detail}"),
            EngineError::Protocol(detail) => format!("engine protocol error: {detail}"),
        }
    }
}

pub(crate) struct Engine {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<String>,
    searching: bool,
    search_start: Option<Instant>,
    info_lines: Vec<String>,
    last_fen: Option<String>,
}

// Locate the engine in this order: a non-empty STOCKFISH_PATH override, an
// executable-sibling stockfish (stockfish.exe on Windows) for packaged
// builds, then the source-tree staged binary below the crate manifest for
// cargo/source runs. No PATH lookup. This function only looks; it never
// downloads. On a supported platform, main() calls fetch::ensure_staged() to
// stage that source-tree binary before trying this lookup a second time.
pub(crate) fn resolve_engine_path() -> Option<PathBuf> {
    let override_path = std::env::var("STOCKFISH_PATH")
        .ok()
        .filter(|p| !p.is_empty());
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()));
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // An empty staged_name can never match a real file, so a platform with
    // no verified fetch pin correctly falls through to just the override and
    // sibling-binary checks below.
    let staged_name = crate::fetch::staged_binary_name().unwrap_or("");
    resolve_engine_path_from(
        override_path.as_deref(),
        exe_dir.as_deref(),
        &manifest_dir,
        sibling_binary_name(),
        staged_name,
    )
}

#[cfg(windows)]
fn sibling_binary_name() -> &'static str {
    "stockfish.exe"
}

#[cfg(not(windows))]
fn sibling_binary_name() -> &'static str {
    "stockfish"
}

pub(crate) fn resolve_engine_path_from(
    override_path: Option<&str>,
    exe_dir: Option<&Path>,
    manifest_dir: &Path,
    sibling_name: &str,
    staged_name: &str,
) -> Option<PathBuf> {
    if let Some(path) = override_path {
        return Some(PathBuf::from(path));
    }
    if let Some(exe) = exe_dir {
        let sibling = exe.join(sibling_name);
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    let staged = manifest_dir
        .join("third_party/stockfish/bundle")
        .join(staged_name);
    if staged.is_file() {
        return Some(staged);
    }
    None
}

pub(crate) fn start() -> Result<Engine, EngineError> {
    let path = resolve_engine_path().ok_or(EngineError::NotFound)?;
    Engine::spawn(&path)
}

impl Engine {
    pub(crate) fn spawn(path: &Path) -> Result<Engine, EngineError> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(EngineError::Spawn)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| EngineError::Protocol("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| EngineError::Protocol("no stdout".into()))?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut engine = Engine {
            child,
            stdin,
            rx,
            searching: false,
            search_start: None,
            info_lines: Vec::new(),
            last_fen: None,
        };
        engine.init()?;
        Ok(engine)
    }

    fn send(&mut self, cmd: &str) -> Result<(), EngineError> {
        writeln!(self.stdin, "{cmd}").map_err(EngineError::Spawn)?;
        self.stdin.flush().map_err(EngineError::Spawn)
    }

    fn wait_for(&self, predicate: impl Fn(&str) -> bool, timeout: Duration) -> Result<(), ()> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(remaining) {
                Ok(line) if predicate(&line) => return Ok(()),
                Ok(_) => {}
                Err(_) => return Err(()),
            }
        }
    }

    fn init(&mut self) -> Result<(), EngineError> {
        self.send("uci")?;
        self.wait_for(|line| line == "uciok", STARTUP_TIMEOUT)
            .map_err(|_| EngineError::Startup("no uciok".into()))?;
        self.send("isready")?;
        self.wait_for(|line| line == "readyok", READY_TIMEOUT)
            .map_err(|_| EngineError::Ready("no readyok".into()))?;
        self.send("ucinewgame")?;
        Ok(())
    }

    pub(crate) fn configure_skill(&mut self, skill: u16) -> Result<(), EngineError> {
        if self.searching {
            self.reset();
        }
        self.send("setoption name UCI_LimitStrength value false")?;
        self.send(&format!("setoption name Skill Level value {skill}"))?;
        self.send("isready")?;
        self.wait_for(|line| line == "readyok", READY_TIMEOUT)
            .map_err(|_| EngineError::Ready("no readyok after skill change".into()))?;
        self.send("ucinewgame")
    }

    // Send the current position and ask the engine to search. The search runs
    // asynchronously; results arrive via try_bestmove().
    pub(crate) fn start_search(&mut self, fen: &str) -> Result<(), EngineError> {
        self.send(&format!("position fen {fen}"))?;
        self.send(&format!("go movetime {MOVE_TIME_MS}"))?;
        self.searching = true;
        self.search_start = Some(Instant::now());
        self.info_lines.clear();
        self.last_fen = Some(fen.to_string());
        Ok(())
    }

    // Cancel a pending search and discard its output (used on restart and before
    // a fresh search when the position changed under the engine). A UCI "stop"
    // prompts the engine to acknowledge with its bestmove, which is drained so no
    // stale result can leak into the next search.
    pub(crate) fn reset(&mut self) {
        let _ = self.send("stop");
        self.searching = false;
        self.search_start = None;
        self.info_lines.clear();
        self.last_fen = None;
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            match self.rx.recv_timeout(Duration::from_millis(10)) {
                Ok(_) => {}
                Err(_) => break,
            }
        }
        while self.rx.try_recv().is_ok() {}
    }

    pub(crate) fn last_fen(&self) -> Option<&str> {
        self.last_fen.as_deref()
    }

    pub(crate) fn is_searching(&self) -> bool {
        self.searching
    }

    pub(crate) fn search_timed_out(&self) -> bool {
        self.searching
            && self
                .search_start
                .is_some_and(|start| start.elapsed() > SEARCH_TIMEOUT)
    }

    // Drain the engine's output without blocking. Returns the bestmove when it
    // arrives; info lines are retained for the thinking status.
    pub(crate) fn try_bestmove(&mut self) -> Option<Result<String, EngineError>> {
        while let Ok(line) = self.rx.try_recv() {
            if let Some(rest) = line.strip_prefix("bestmove ") {
                self.searching = false;
                self.search_start = None;
                let mv = rest.split_whitespace().next().unwrap_or("");
                return Some(if mv.is_empty() || mv == "(none)" {
                    Err(EngineError::Protocol("empty bestmove".into()))
                } else {
                    Ok(mv.to_string())
                });
            }
            if line.starts_with("info") {
                self.info_lines.push(line);
            }
        }
        None
    }

    // A short human-readable thinking status from the latest info line.
    pub(crate) fn thinking_status(&self) -> String {
        if !self.searching {
            return String::new();
        }
        let Some(info) = self.info_lines.last() else {
            return "Engine thinking...".into();
        };
        let words: Vec<&str> = info.split_whitespace().collect();
        let depth = words.windows(2).find(|w| w[0] == "depth").map(|w| w[1]);
        let score = words.windows(2).find(|w| w[0] == "cp").map(|w| w[1]);
        match (depth, score) {
            (Some(d), Some(s)) => format!("Engine thinking (depth {d}, score cp {s})"),
            (Some(d), None) => format!("Engine thinking (depth {d})"),
            _ => "Engine thinking...".into(),
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.send("quit");
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
