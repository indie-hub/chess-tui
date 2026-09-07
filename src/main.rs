mod game;
mod input;
mod render;
mod sprites;
#[cfg(test)]
mod tests;

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use game::Game;
use render::{MIN_HEIGHT, MIN_WIDTH, draw};

fn main() -> io::Result<()> {
    ratatui::run(|terminal| {
        let mut game = Game::default();
        loop {
            terminal.draw(|frame| draw(frame, &game))?;
            if let Event::Key(key) = event::read()? {
                let size = terminal.size()?;
                if size.width < MIN_WIDTH || size.height < MIN_HEIGHT {
                    if key.kind == KeyEventKind::Press
                        && (key.code == KeyCode::Char('q')
                            || (key.code == KeyCode::Char('c')
                                && key.modifiers.contains(KeyModifiers::CONTROL)))
                    {
                        return Ok(());
                    }
                } else if game.key(key) {
                    return Ok(());
                }
            }
        }
    })
}
