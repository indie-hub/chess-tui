// Test-only deterministic UCI engine used by the engine integration tests.
// It is driven through the same subprocess protocol as a real engine. The
// FAKE_ENGINE_MODE environment variable selects the scripted behaviour:
//   (empty)     normal: readyok and a legal bestmove (e2e4 / e7e5 by side)
//   "no-uci"    never answers uci (startup uciok timeout)
//   "no-ready"  never answers isready (startup ready timeout)
//   "delayed"   answers uciok/readyok after 300ms (within the timeouts)
//   "early-exit" exits immediately without answering (startup failure)
//   "timeout"   never answers a search (search stays pending)
//   "slow"      answers a search after 1200ms (delayed bestmove)
//   "illegal"   answers a search with an illegal move e2e5
//   "malformed" answers a search with an unparseable move x1y2
//   "none"      answers a search with "bestmove (none)"
//   "ignore-quit" ignores the quit command (kill fallback path)
use std::io::{self, BufRead, Write};

fn best_move_for(fen: &str) -> &'static str {
    // The FEN's side-to-move is the second whitespace token.
    if fen.split_whitespace().nth(1) == Some("b") {
        "e7e5"
    } else {
        "e2e4"
    }
}

fn main() {
    let mode = std::env::var("FAKE_ENGINE_MODE").unwrap_or_default();
    if mode == "early-exit" {
        std::process::exit(1);
    }
    let stdin = io::stdin();
    let mut out = io::stdout();
    let mut current_fen = String::new();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if let Some(fen) = line.strip_prefix("position fen ") {
            current_fen = fen.to_string();
        }
        match line.as_str() {
            "uci" if mode != "no-uci" => {
                if mode == "delayed" {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
                let _ = writeln!(out, "id name fake_engine\nid author room-4\nuciok");
                let _ = out.flush();
            }
            "uci" => {}
            "isready" if mode != "no-ready" => {
                if mode == "delayed" {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
                let _ = writeln!(out, "readyok");
                let _ = out.flush();
            }
            "isready" => {}
            "ucinewgame" => {}
            "quit" if mode != "ignore-quit" => std::process::exit(0),
            "quit" => {}
            _ if line.starts_with("position") || line.starts_with("go") => match mode.as_str() {
                "timeout" => {}
                "illegal" => {
                    let _ = writeln!(out, "bestmove e2e5");
                    let _ = out.flush();
                }
                "malformed" => {
                    let _ = writeln!(out, "bestmove x1y2");
                    let _ = out.flush();
                }
                "none" => {
                    let _ = writeln!(out, "bestmove (none)");
                    let _ = out.flush();
                }
                "slow" => {
                    std::thread::sleep(std::time::Duration::from_millis(1200));
                    let mv = best_move_for(&current_fen);
                    let _ = writeln!(out, "info depth 3 score cp 25");
                    let _ = writeln!(out, "bestmove {mv}");
                    let _ = out.flush();
                }
                _ => {
                    let mv = best_move_for(&current_fen);
                    let _ = writeln!(out, "info depth 3 score cp 25");
                    let _ = writeln!(out, "bestmove {mv}");
                    let _ = out.flush();
                }
            },
            _ => {}
        }
    }
}
