mod engine;
mod game;
mod input;
mod render;
mod sprites;
#[cfg(test)]
mod tests;

use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use engine::Engine;
use game::{Game, HumanSide};
use render::{MIN_HEIGHT, MIN_WIDTH, draw};
use shakmaty::{Color as Side, uci::UciMove};

fn resolve_human_side(choice: HumanSide, random_white: bool) -> Side {
    match choice {
        HumanSide::White => Side::White,
        HumanSide::Black => Side::Black,
        HumanSide::Random if random_white => Side::White,
        HumanSide::Random => Side::Black,
    }
}

fn start_configured_game(
    engine: &mut Option<Engine>,
    game: &mut Game,
    choice: HumanSide,
    skill: u16,
) {
    let random_white = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(true, |duration| duration.subsec_nanos() % 2 == 0);
    let human_side = resolve_human_side(choice, random_white);
    if let Some(engine) = engine.as_mut() {
        match engine.configure_skill(skill) {
            Ok(()) => {
                *game = Game::new_vs_engine(!human_side);
                game.config_side = choice;
                game.engine_skill = skill;
            }
            Err(err) => {
                *game = Game {
                    engine_status: format!("Engine unavailable: {}", err.message()),
                    engine_failed: true,
                    config_side: choice,
                    engine_skill: skill,
                    ..Game::default()
                };
            }
        }
    } else {
        *game = Game {
            engine_status: "Engine unavailable: 2-player mode".into(),
            config_side: choice,
            engine_skill: skill,
            ..Game::default()
        };
    }
}

// Drive the engine while it is the engine's turn: start the search when the
// turn begins, then poll for the bestmove without blocking. Every engine move
// is parsed with shakmaty's UciMove and entered through Game::play, which is
// the legal-move choke point.
fn drive_engine(engine: &mut Engine, game: &mut Game) {
    if !game.engine_to_move() {
        // The game left the engine's turn (e.g. restart); cancel any in-flight
        // search so its result can never be applied to a new position.
        if engine.is_searching() {
            engine.reset();
        }
        return;
    }
    if game.engine_failed {
        return;
    }
    let fen = game.to_fen();
    if !engine.is_searching() || engine.last_fen() != Some(fen.as_str()) {
        if engine.is_searching() {
            // The position changed under a pending search (e.g. restart):
            // cancel it so a stale bestmove can never be applied.
            engine.reset();
        }
        if let Err(err) = engine.start_search(&fen) {
            game.engine_failed = true;
            game.engine_status = format!("Engine error: {}", err.message());
            return;
        }
    }
    match engine.try_bestmove() {
        Some(Ok(mv)) => match UciMove::from_ascii(mv.as_bytes())
            .ok()
            .and_then(|uci| uci.to_move(&game.position).ok())
        {
            Some(m) => {
                game.engine_status.clear();
                game.play(m);
            }
            None => {
                game.engine_failed = true;
                game.engine_status = format!("Engine error: illegal move {mv}");
            }
        },
        Some(Err(err)) => {
            game.engine_failed = true;
            game.engine_status = format!("Engine error: {}", err.message());
        }
        None => {
            if engine.search_timed_out() {
                game.engine_failed = true;
                game.engine_status = "Engine error: search timed out".into();
            } else {
                game.engine_status = engine.thinking_status();
            }
        }
    }
}

fn main() -> io::Result<()> {
    let mut engine = match engine::start() {
        Ok(engine) => Some(engine),
        Err(err) => {
            eprintln!("[chess] engine unavailable: {}", err.message());
            None
        }
    };
    let mut game = if engine.is_some() {
        Game::new_vs_engine(Side::Black)
    } else {
        Game {
            engine_status: "Engine unavailable: 2-player mode (set STOCKFISH_PATH)".into(),
            ..Game::default()
        }
    };
    ratatui::run(|terminal| {
        loop {
            terminal.draw(|frame| draw(frame, &game))?;
            if let Some(engine) = engine.as_mut() {
                drive_engine(engine, &mut game);
            }
            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
            {
                let size = terminal.size()?;
                let small = size.width < MIN_WIDTH || size.height < MIN_HEIGHT;
                let press = key.kind == KeyEventKind::Press;
                let quit_key = key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL));
                if small && press && quit_key {
                    return Ok(());
                }
                if !small {
                    if game.key(key) {
                        return Ok(());
                    }
                    if let Some((side, skill)) = game.take_new_game_request() {
                        start_configured_game(&mut engine, &mut game, side, skill);
                    }
                }
            }
        }
    })
}
