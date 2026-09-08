use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use shakmaty::Role;

use crate::game::Game;

impl Game {
    pub(crate) fn key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        // On the engine's turn only cursor, quit, restart and side-switch are
        // accepted; piece-selection, promotion and draw keys stay blocked.
        if self.engine_to_move()
            && !matches!(
                key.code,
                KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Char('h' | 'j' | 'k' | 'l' | 'q' | 'N' | 's')
            )
        {
            return false;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return true;
        }
        if key.code == KeyCode::Esc {
            self.selected = None;
            self.promotion.clear();
            self.notice.clear();
        } else if !self.promotion.is_empty() {
            let role = match key.code {
                KeyCode::Char('q') => Some(Role::Queen),
                KeyCode::Char('r') => Some(Role::Rook),
                KeyCode::Char('b') => Some(Role::Bishop),
                KeyCode::Char('n') => Some(Role::Knight),
                _ => None,
            };
            if let Some(m) = self
                .promotion
                .iter()
                .find(|m| m.promotion() == role && role.is_some())
                .copied()
            {
                self.play(m);
            } else if key.code == KeyCode::Char('d') {
                self.claim_draw();
            }
        } else {
            let (mut file, mut rank) =
                (i32::from(self.cursor.file()), i32::from(self.cursor.rank()));
            match key.code {
                KeyCode::Char('q') => return true,
                KeyCode::Char('N') => {
                    let side = self.engine_side;
                    *self = if self.versus_engine {
                        Self::new_vs_engine(side)
                    } else {
                        Self::default()
                    };
                }
                KeyCode::Char('s') if self.versus_engine => self.switch_sides(),
                KeyCode::Char('d') => self.claim_draw(),
                KeyCode::Enter => self.select(),
                KeyCode::Left | KeyCode::Char('h') => file -= 1,
                KeyCode::Right | KeyCode::Char('l') => file += 1,
                KeyCode::Up | KeyCode::Char('k') => rank += 1,
                KeyCode::Down | KeyCode::Char('j') => rank -= 1,
                _ => {}
            }
            if matches!(
                key.code,
                KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Char('h' | 'j' | 'k' | 'l')
            ) {
                self.cursor =
                    shakmaty::Square::new((rank.clamp(0, 7) * 8 + file.clamp(0, 7)) as u32);
            }
        }
        false
    }
}
