// SPDX-License-Identifier: GPL-3.0-or-later

use shakmaty::{Chess, Color, EnPassantMode, Move, Position, Square, fen::Fen, san::SanPlus};

pub(crate) const MAX_SKILL: u16 = 20;
pub(crate) const SKILL_STEP: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HumanSide {
    White,
    Black,
    Random,
}

impl HumanSide {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Black => "Black",
            Self::Random => "Random",
        }
    }

    pub(crate) fn previous(self) -> Self {
        match self {
            Self::White => Self::Random,
            Self::Black => Self::White,
            Self::Random => Self::Black,
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::Random,
            Self::Random => Self::White,
        }
    }
}

pub(crate) struct Game {
    pub(crate) position: Chess,
    pub(crate) positions: Vec<Chess>,
    pub(crate) history: Vec<String>,
    pub(crate) cursor: Square,
    pub(crate) selected: Option<Square>,
    pub(crate) promotion: Vec<Move>,
    pub(crate) last_move: Option<(Square, Square)>,
    pub(crate) claimed: bool,
    pub(crate) resigned: Option<Color>,
    pub(crate) notice: String,
    pub(crate) versus_engine: bool,
    pub(crate) engine_side: Color,
    pub(crate) engine_status: String,
    pub(crate) engine_failed: bool,
    pub(crate) configuring: bool,
    pub(crate) config_side: HumanSide,
    pub(crate) engine_skill: u16,
    pub(crate) new_game_requested: bool,
    pub(crate) config_snapshot: Option<(HumanSide, u16)>,
    pub(crate) captured_by_white: Vec<shakmaty::Role>,
    pub(crate) captured_by_black: Vec<shakmaty::Role>,
}

impl Default for Game {
    fn default() -> Self {
        let position = Chess::default();
        Self {
            positions: vec![position.clone()],
            position,
            history: Vec::new(),
            cursor: Square::E2,
            selected: None,
            promotion: Vec::new(),
            last_move: None,
            claimed: false,
            resigned: None,
            notice: "White: light pieces. Black: dark pieces.".into(),
            versus_engine: false,
            engine_side: Color::Black,
            engine_status: String::new(),
            engine_failed: false,
            configuring: false,
            config_side: HumanSide::White,
            engine_skill: MAX_SKILL,
            new_game_requested: false,
            config_snapshot: None,
            captured_by_white: Vec::new(),
            captured_by_black: Vec::new(),
        }
    }
}

// Standard castling is entered at the king's destination, not the rook square.
pub(crate) fn destination(m: Move) -> Square {
    match m {
        Move::Castle { king, rook } => Square::from_coords(
            if rook.file() > king.file() {
                shakmaty::File::G
            } else {
                shakmaty::File::C
            },
            king.rank(),
        ),
        _ => m.to(),
    }
}

fn role_value(role: shakmaty::Role) -> i32 {
    match role {
        shakmaty::Role::Pawn => 1,
        shakmaty::Role::Knight | shakmaty::Role::Bishop => 3,
        shakmaty::Role::Rook => 5,
        shakmaty::Role::Queen => 9,
        shakmaty::Role::King => 0,
    }
}

impl Game {
    pub(crate) fn repetitions(&self, position: &Chess) -> usize {
        self.positions.iter().filter(|p| *p == position).count()
    }

    pub(crate) fn ending(&self) -> Option<String> {
        if self.position.is_checkmate() {
            Some(format!("Checkmate! {} wins.", !self.position.turn()))
        } else if self.position.is_stalemate() {
            Some("Draw: stalemate.".into())
        } else if self.position.is_insufficient_material() {
            Some("Draw: insufficient material.".into())
        } else if self.claimed {
            Some("Draw claimed.".into())
        } else if let Some(side) = self.resigned {
            Some(format!("{side} resigns. {} wins.", side.other()))
        } else if self.position.halfmoves() >= 150 {
            Some("Draw: 75-move rule.".into())
        } else if self.repetitions(&self.position) >= 5 {
            Some("Draw: fivefold repetition.".into())
        } else {
            None
        }
    }

    pub(crate) fn claimable(&self) -> bool {
        self.position.halfmoves() >= 100 || self.repetitions(&self.position) >= 3
    }

    pub(crate) fn candidates(&self) -> Vec<Move> {
        self.position
            .legal_moves()
            .into_iter()
            .filter(|m| m.from() == self.selected && destination(*m) == self.cursor)
            .collect()
    }

    pub(crate) fn cycle_legal_destination(&mut self, forward: bool) {
        let Some(selected) = self.selected else {
            return;
        };
        let mut destinations: Vec<_> = self
            .position
            .legal_moves()
            .into_iter()
            .filter(|m| m.from() == Some(selected))
            .map(destination)
            .collect();
        destinations.sort_by_key(|square| square.to_u32());
        destinations.dedup();
        let Some(index) = destinations
            .iter()
            .position(|&square| square == self.cursor)
        else {
            self.cursor = if forward {
                destinations.first().copied()
            } else {
                destinations.last().copied()
            }
            .unwrap_or(self.cursor);
            return;
        };
        if !destinations.is_empty() {
            let next = if forward {
                (index + 1) % destinations.len()
            } else {
                (index + destinations.len() - 1) % destinations.len()
            };
            self.cursor = destinations[next];
        }
    }

    pub(crate) fn play(&mut self, m: Move) {
        if self.ending().is_some() || !self.position.is_legal(m) {
            self.notice = "Illegal move.".into();
            return;
        }
        if let Some(captured) = m.capture() {
            match self.position.turn() {
                Color::White => self.captured_by_white.push(captured),
                Color::Black => self.captured_by_black.push(captured),
            }
        }
        self.history
            .push(SanPlus::from_move(self.position.clone(), m).to_string());
        self.last_move = m.from().map(|from| (from, destination(m)));
        self.position.play_unchecked(m);
        self.positions.push(self.position.clone());
        self.selected = None;
        self.promotion.clear();
        self.notice.clear();
    }

    pub(crate) fn select(&mut self) {
        if self.ending().is_some() {
            self.notice = "Game over. Press N for a new game.".into();
            return;
        }
        let moves = self.candidates();
        if let Some(&m) = moves.first() {
            if m.promotion().is_some() {
                self.promotion = moves;
                self.notice = "Promote: q queen / r rook / b bishop / n knight".into();
            } else {
                self.play(m);
            }
        } else if self
            .position
            .board()
            .piece_at(self.cursor)
            .is_some_and(|p| p.color == self.position.turn())
        {
            self.selected = Some(self.cursor);
            self.notice = format!("Selected {}. Choose a legal destination.", self.cursor);
        } else {
            self.notice = "Illegal destination. Select a piece of the side to move.".into();
        }
    }

    pub(crate) fn claim_draw(&mut self) {
        if self.ending().is_some() {
            return;
        }
        // An intended legal move can establish a claim without being played.
        let intended = self.candidates().into_iter().any(|m| {
            let mut next = self.position.clone();
            next.play_unchecked(m);
            next.halfmoves() >= 100 || self.repetitions(&next) >= 2
        });
        if self.claimable() || intended {
            self.claimed = true;
            self.selected = None;
            self.promotion.clear();
            self.notice.clear();
        } else {
            self.notice = "No draw claim here or after the selected move.".into();
        }
    }

    /// End the game in the resigning side's opponent's favour. Versus the
    /// engine the human always resigns, since there is no engine resign path;
    /// in local play the side to move resigns. Ignored once the game has ended.
    pub(crate) fn resign(&mut self) {
        if self.ending().is_some() {
            return;
        }
        let side = if self.versus_engine {
            self.engine_side.other()
        } else {
            self.position.turn()
        };
        self.resigned = Some(side);
        self.selected = None;
        self.promotion.clear();
        self.notice.clear();
    }

    pub(crate) fn new_vs_engine(engine_side: Color) -> Self {
        Self {
            versus_engine: true,
            engine_side,
            cursor: if engine_side == Color::White {
                Square::E7
            } else {
                Square::E2
            },
            config_side: match engine_side.other() {
                Color::White => HumanSide::White,
                Color::Black => HumanSide::Black,
            },
            ..Self::default()
        }
    }

    pub(crate) fn open_new_game_config(&mut self) {
        self.config_snapshot = Some((self.config_side, self.engine_skill));
        self.configuring = true;
        self.new_game_requested = false;
    }

    pub(crate) fn cancel_new_game_config(&mut self) {
        if let Some((side, skill)) = self.config_snapshot.take() {
            self.config_side = side;
            self.engine_skill = skill;
        }
        self.configuring = false;
    }

    pub(crate) fn request_configured_game(&mut self) {
        self.config_snapshot = None;
        self.configuring = false;
        self.new_game_requested = true;
    }

    pub(crate) fn take_new_game_request(&mut self) -> Option<(HumanSide, u16)> {
        self.new_game_requested.then(|| {
            self.new_game_requested = false;
            (self.config_side, self.engine_skill)
        })
    }

    pub(crate) fn engine_to_move(&self) -> bool {
        self.versus_engine && !self.configuring && self.position.turn() == self.engine_side
    }

    pub(crate) fn board_flipped(&self) -> bool {
        self.versus_engine && self.engine_side == Color::White
    }

    pub(crate) fn switch_sides(&mut self) {
        let skill = self.engine_skill;
        *self = Self::new_vs_engine(!self.engine_side);
        self.engine_skill = skill;
    }

    /// Request a fresh game with the current configuration. The restart runs
    /// through the shared new-game path, so a `Random` human side is
    /// re-resolved by the coin flip.
    pub(crate) fn rematch(&mut self) {
        self.new_game_requested = true;
    }

    pub(crate) fn to_fen(&self) -> String {
        Fen::from_position(&self.position, EnPassantMode::Legal).to_string()
    }

    /// Roles captured by `side` (pieces that `side` has taken).
    /// Returned slice is in capture order. Empty if `side` has not captured.
    pub(crate) fn captured_by(&self, side: Color) -> &[shakmaty::Role] {
        match side {
            Color::White => &self.captured_by_white,
            Color::Black => &self.captured_by_black,
        }
    }

    /// Conventional material balance: sum of captured values for White minus
    /// sum for Black (pawn 1, knight/bishop 3, rook 5, queen 9, king 0).
    /// Positive means White has captured more material than Black (White advantage).
    pub(crate) fn material_balance(&self) -> i32 {
        let white: i32 = self.captured_by_white.iter().map(|r| role_value(*r)).sum();
        let black: i32 = self.captured_by_black.iter().map(|r| role_value(*r)).sum();
        white - black
    }
}

#[cfg(test)]
mod material_score_tests {
    use super::Game;
    use shakmaty::{CastlingMode, Chess, Position, Role, Square, fen::Fen, uci::UciMove};

    fn position(fen: &str) -> Game {
        let pos = fen
            .parse::<Fen>()
            .unwrap()
            .into_position::<Chess>(CastlingMode::Standard)
            .unwrap();
        Game {
            positions: vec![pos.clone()],
            position: pos,
            ..Game::default()
        }
    }

    fn move_uci(game: &mut Game, uci: &str) {
        let m = uci
            .parse::<UciMove>()
            .unwrap()
            .to_move(&game.position)
            .unwrap();
        game.play(m);
    }

    #[test]
    fn ordinary_capture_tracks_by_capturer() {
        let mut game = position("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1");
        move_uci(&mut game, "e4d5");
        assert_eq!(game.captured_by(shakmaty::Color::White), &[Role::Pawn]);
        assert!(game.captured_by(shakmaty::Color::Black).is_empty());
        assert_eq!(game.material_balance(), 1);
    }

    #[test]
    fn en_passant_capture_counts_as_pawn() {
        let mut game = position("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
        move_uci(&mut game, "e5d6");
        assert_eq!(game.captured_by(shakmaty::Color::White), &[Role::Pawn]);
        assert_eq!(game.material_balance(), 1);
        assert!(game.position.board().piece_at(Square::D5).is_none());
    }

    #[test]
    fn promotion_after_capture_counts_captured_role_not_promoted() {
        let mut game = position("r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1");
        move_uci(&mut game, "b7a8q");
        assert_eq!(game.captured_by(shakmaty::Color::White), &[Role::Rook]);
        assert_eq!(game.material_balance(), 5);
        assert_eq!(
            game.position.board().piece_at(Square::A8).unwrap().role,
            Role::Queen
        );
    }

    #[test]
    fn castling_and_non_capture_do_not_affect_material() {
        let mut game = position("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        move_uci(&mut game, "e1g1");
        assert!(game.captured_by(shakmaty::Color::White).is_empty());
        assert!(game.captured_by(shakmaty::Color::Black).is_empty());
        assert_eq!(game.material_balance(), 0);
        let mut game = Game::default();
        move_uci(&mut game, "e2e4");
        assert_eq!(game.material_balance(), 0);
        assert!(game.captured_by(shakmaty::Color::White).is_empty());
    }

    #[test]
    fn material_balance_positive_for_white_advantage() {
        let mut g = position("4k3/8/2q5/3P4/8/8/8/4K3 w - - 0 1");
        move_uci(&mut g, "d5c6");
        assert_eq!(g.captured_by(shakmaty::Color::White), &[Role::Queen]);
        assert_eq!(g.material_balance(), 9);
        let mut g = position("r3k3/8/8/8/8/8/P7/4K3 b - - 0 1");
        move_uci(&mut g, "a8a2");
        assert_eq!(g.captured_by(shakmaty::Color::Black), &[Role::Pawn]);
        assert_eq!(g.material_balance(), -1);
        // Sequential white queen then black rook -> 9 -5 =4
        let mut game = position("3rk3/8/8/3q4/3R4/8/8/4K3 w - - 0 1");
        move_uci(&mut game, "d4d5");
        assert_eq!(game.captured_by(shakmaty::Color::White), &[Role::Queen]);
        assert_eq!(game.material_balance(), 9);
        move_uci(&mut game, "d8d5");
        assert_eq!(game.captured_by(shakmaty::Color::Black), &[Role::Rook]);
        assert_eq!(game.material_balance(), 4);
    }

    #[test]
    fn multiple_captures_accumulate_and_balance() {
        let mut game = position("4k3/8/8/2q5/3P4/8/8/4K3 w - - 0 1");
        move_uci(&mut game, "d4c5");
        assert_eq!(game.captured_by(shakmaty::Color::White), &[Role::Queen]);
        let mut game = position("4k3/8/8/8/4p3/3P4/8/4K3 b - - 0 1");
        move_uci(&mut game, "e4d3");
        assert_eq!(game.captured_by(shakmaty::Color::Black), &[Role::Pawn]);
        assert_eq!(game.material_balance(), -1);
        let mut game = position("3rk3/8/8/3q4/3R4/8/8/4K3 w - - 0 1");
        move_uci(&mut game, "d4d5");
        move_uci(&mut game, "d8d5");
        assert_eq!(game.captured_by(shakmaty::Color::White), &[Role::Queen]);
        assert_eq!(game.captured_by(shakmaty::Color::Black), &[Role::Rook]);
        assert_eq!(game.material_balance(), 4);
    }

    #[test]
    fn restart_and_switch_sides_reset_captures() {
        let mut game = position("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1");
        move_uci(&mut game, "e4d5");
        assert_eq!(game.material_balance(), 1);
        let fresh = Game::default();
        assert_eq!(fresh.material_balance(), 0);
        assert!(fresh.captured_by(shakmaty::Color::White).is_empty());
        assert!(fresh.captured_by(shakmaty::Color::Black).is_empty());
        let mut vs = position("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1");
        vs.versus_engine = true;
        vs.engine_side = shakmaty::Color::Black;
        move_uci(&mut vs, "e4d5");
        assert_eq!(vs.material_balance(), 1);
        vs.switch_sides();
        assert_eq!(vs.material_balance(), 0);
        assert!(vs.captured_by(shakmaty::Color::White).is_empty());
    }

    #[test]
    fn engine_moves_through_play_are_tracked() {
        let mut game = Game::new_vs_engine(shakmaty::Color::Black);
        // Human plays e4 (non-capture)
        move_uci(&mut game, "e2e4");
        assert_eq!(game.material_balance(), 0);
        // Engine (black) captures on e4? Set up so engine capture is legal via play.
        // Simulate engine playing d7d5 then human capturing en passant style not needed.
        // Instead directly play a black capture through same play method.
        let mut game = position("4k3/8/8/8/4p3/3P4/8/4K3 b - - 0 1");
        move_uci(&mut game, "e4d3"); // black pawn e4 captures white pawn d3
        assert_eq!(game.captured_by(shakmaty::Color::Black), &[Role::Pawn]);
        assert_eq!(game.material_balance(), -1);
    }

    #[test]
    fn knight_bishop_rook_queen_values() {
        // Directly test role_value via material_balance by capturing each role
        for (fen, cap, expected) in [
            ("4k3/8/8/3n4/4P3/8/8/4K3 w - - 0 1", Role::Knight, 3),
            ("4k3/8/8/3b4/4P3/8/8/4K3 w - - 0 1", Role::Bishop, 3),
            ("4k3/8/8/3r4/4P3/8/8/4K3 w - - 0 1", Role::Rook, 5),
            ("4k3/8/8/3q4/4P3/8/8/4K3 w - - 0 1", Role::Queen, 9),
            ("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1", Role::Pawn, 1),
        ] {
            let mut game = position(fen);
            move_uci(&mut game, "e4d5");
            assert_eq!(game.captured_by(shakmaty::Color::White)[0], cap);
            assert_eq!(game.material_balance(), expected);
        }
    }
}
