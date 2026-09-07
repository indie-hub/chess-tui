use shakmaty::{Chess, Move, Position, Square, san::SanPlus};

pub(crate) struct Game {
    pub(crate) position: Chess,
    pub(crate) positions: Vec<Chess>,
    pub(crate) history: Vec<String>,
    pub(crate) cursor: Square,
    pub(crate) selected: Option<Square>,
    pub(crate) promotion: Vec<Move>,
    pub(crate) last_move: Option<(Square, Square)>,
    pub(crate) claimed: bool,
    pub(crate) notice: String,
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
            notice: "White: light pieces. Black: dark pieces.".into(),
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

    pub(crate) fn play(&mut self, m: Move) {
        if self.ending().is_some() || !self.position.is_legal(m) {
            self.notice = "Illegal move.".into();
            return;
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
}
