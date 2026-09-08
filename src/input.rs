// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use shakmaty::Role;

use crate::game::{Game, MAX_SKILL, SKILL_STEP};

impl Game {
    pub(crate) fn key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return true;
        }
        let opens_config = key.code == KeyCode::Char('N')
            || (key.code == KeyCode::Char('n') && self.promotion.is_empty());
        if opens_config && !self.configuring {
            self.open_new_game_config();
            return false;
        }
        if self.configuring {
            match key.code {
                KeyCode::Char('q') => return true,
                KeyCode::Esc => self.cancel_new_game_config(),
                KeyCode::Enter => self.request_configured_game(),
                KeyCode::Left | KeyCode::Char('h') => {
                    self.config_side = self.config_side.previous();
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.config_side = self.config_side.next();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.engine_skill = self.engine_skill.saturating_add(SKILL_STEP).min(MAX_SKILL);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.engine_skill = self.engine_skill.saturating_sub(SKILL_STEP);
                }
                _ => {}
            }
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
            let direction = if self.board_flipped() { -1 } else { 1 };
            match key.code {
                KeyCode::Char('q') => return true,
                KeyCode::Char('s') if self.versus_engine => self.switch_sides(),
                KeyCode::Char('d') => self.claim_draw(),
                KeyCode::Enter => self.select(),
                KeyCode::Left | KeyCode::Char('h') => file -= direction,
                KeyCode::Right | KeyCode::Char('l') => file += direction,
                KeyCode::Up | KeyCode::Char('k') => rank += direction,
                KeyCode::Down | KeyCode::Char('j') => rank -= direction,
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
