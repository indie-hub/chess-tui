use crate::game::{Game, destination};
use crate::render::{
    BOARD_CELLS_H, BOARD_CELLS_W, BOARD_X, BOARD_Y, MIN_HEIGHT, MIN_WIDTH, SQUARE_H, SQUARE_W,
    base_bg, draw, sprite_bytes, sprite_index,
};
use crate::sprites::{SPRITE_SIZE, sprite_pixels};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use ratatui::{Terminal, backend::TestBackend, style::Color};
use sha2::{Digest, Sha256};
use shakmaty::{CastlingMode, Chess, Position, Role, Square, fen::Fen, uci::UciMove};
use std::collections::HashSet;

fn position(fen: &str) -> Game {
    let position = fen
        .parse::<Fen>()
        .unwrap()
        .into_position::<Chess>(CastlingMode::Standard)
        .unwrap();
    Game {
        positions: vec![position.clone()],
        position,
        ..Game::default()
    }
}

fn key(game: &mut Game, code: KeyCode) -> bool {
    game.key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn move_uci(game: &mut Game, uci: &str) {
    let m = uci
        .parse::<UciMove>()
        .unwrap()
        .to_move(&game.position)
        .unwrap();
    game.play(m);
}

fn choose(game: &mut Game, from: Square, to: Square) {
    game.cursor = from;
    key(game, KeyCode::Enter);
    game.cursor = to;
    key(game, KeyCode::Enter);
}

fn draw_terminal(terminal: &mut Terminal<TestBackend>, game: &Game) {
    terminal.draw(|frame| draw(frame, game)).unwrap();
}

fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}

fn sq(file: u16, row: u16) -> (u16, u16) {
    (BOARD_X + file * SQUARE_W, BOARD_Y + row * SQUARE_H)
}

// The colour a sprite pixel paints: the exact pixel colour, or the square
// colour when the pixel is transparent.
fn painted(data: &[u8; SPRITE_SIZE * SPRITE_SIZE * 4], x: u32, y: u32, bg: [u8; 3]) -> [u8; 3] {
    let i = ((y * SPRITE_SIZE as u32 + x) * 4) as usize;
    if data[i + 3] == 0 {
        bg
    } else {
        [data[i], data[i + 1], data[i + 2]]
    }
}

#[test]
fn keyboard_selection_illegal_move_cancel_restart_and_exit() {
    let mut game = Game::default();
    key(&mut game, KeyCode::Enter);
    assert_eq!(game.selected, Some(Square::E2));
    key(&mut game, KeyCode::Char('k'));
    key(&mut game, KeyCode::Up);
    key(&mut game, KeyCode::Enter);
    assert_eq!(game.history, ["e4"]);
    assert_eq!(game.last_move, Some((Square::E2, Square::E4)));
    assert_eq!(game.position.turn(), shakmaty::Color::Black);
    choose(&mut game, Square::E7, Square::E4);
    assert_eq!(game.history.len(), 1);
    key(&mut game, KeyCode::Esc);
    assert_eq!(game.selected, None);
    game.cursor = Square::A8;
    key(&mut game, KeyCode::Left);
    key(&mut game, KeyCode::Up);
    assert_eq!(game.cursor, Square::A8);
    key(&mut game, KeyCode::Char('N'));
    assert_eq!(game.position, Chess::default());
    assert!(game.history.is_empty());
    assert!(key(&mut game, KeyCode::Char('q')));
    assert!(game.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)));
    let mut release = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    assert!(!game.key(release));
}

#[test]
fn castles_en_passant_and_promotion_choices() {
    for (from, to, rook) in [
        (Square::E1, Square::G1, Square::F1),
        (Square::E1, Square::C1, Square::D1),
    ] {
        let mut game = position("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        choose(&mut game, from, to);
        assert_eq!(game.position.board().piece_at(to).unwrap().role, Role::King);
        assert_eq!(
            game.position.board().piece_at(rook).unwrap().role,
            Role::Rook
        );
        assert_eq!(game.last_move, Some((from, to)));
    }
    let mut game = position("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
    choose(&mut game, Square::E5, Square::D6);
    assert!(game.position.board().piece_at(Square::D5).is_none());
    assert_eq!(game.history, ["exd6"]);
    for (letter, role) in [
        ('q', Role::Queen),
        ('r', Role::Rook),
        ('b', Role::Bishop),
        ('n', Role::Knight),
    ] {
        let mut game = position("4k3/P7/8/8/8/8/8/4K3 w - - 0 1");
        choose(&mut game, Square::A7, Square::A8);
        assert_eq!(game.promotion.len(), 4);
        assert!(game.history.is_empty());
        key(&mut game, KeyCode::Char(letter));
        assert_eq!(
            game.position.board().piece_at(Square::A8).unwrap().role,
            role
        );
        assert_eq!(game.history.len(), 1);
    }
    let mut game = position("4k3/P7/8/8/8/8/8/4K3 w - - 0 1");
    choose(&mut game, Square::A7, Square::A8);
    key(&mut game, KeyCode::Esc);
    assert!(game.promotion.is_empty());
    assert!(game.history.is_empty());
}

#[test]
fn check_mate_stalemate_material_and_illegal_king_exposure() {
    let mut game = Game::default();
    for m in ["f2f3", "e7e5", "g2g4", "d8h4"] {
        move_uci(&mut game, m);
    }
    assert!(game.ending().unwrap().contains("Checkmate"));
    choose(&mut game, Square::A2, Square::A3);
    assert_eq!(game.history.len(), 4);
    assert!(
        position("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1")
            .ending()
            .unwrap()
            .contains("stalemate")
    );
    assert!(
        position("7k/8/6K1/8/8/8/8/8 w - - 0 1")
            .ending()
            .unwrap()
            .contains("insufficient")
    );
    let mut pinned = position("4r1k1/8/8/8/8/8/4R3/4K3 w - - 0 1");
    choose(&mut pinned, Square::E2, Square::D2);
    assert!(pinned.history.is_empty());
    let mut castle_through_check = position("4kr2/8/8/8/8/8/8/4K2R w K - 0 1");
    choose(&mut castle_through_check, Square::E1, Square::G1);
    assert!(castle_through_check.history.is_empty());
}

#[test]
fn claimable_and_automatic_draws_with_mate_precedence() {
    let mut game = Game::default();
    key(&mut game, KeyCode::Char('d'));
    assert!(!game.claimed);
    let cycle = ["g1f3", "g8f6", "f3g1", "f6g8"];
    for _ in 0..2 {
        for m in cycle {
            move_uci(&mut game, m);
        }
    }
    assert!(game.claimable());
    assert!(game.ending().is_none());
    key(&mut game, KeyCode::Char('d'));
    assert!(game.claimed);
    let mut game = Game::default();
    for _ in 0..4 {
        for m in cycle {
            move_uci(&mut game, m);
        }
    }
    assert!(game.ending().unwrap().contains("fivefold"));
    let mut intended = Game::default();
    for m in cycle.into_iter().cycle().take(7) {
        move_uci(&mut intended, m);
    }
    intended.cursor = Square::F6;
    key(&mut intended, KeyCode::Enter);
    intended.cursor = Square::G8;
    key(&mut intended, KeyCode::Char('d'));
    assert!(intended.claimed);
    assert_eq!(intended.history.len(), 7);
    let mut fifty = position("7k/8/6K1/8/8/8/8/R7 w - - 99 51");
    assert!(!fifty.claimable());
    fifty.cursor = Square::A1;
    key(&mut fifty, KeyCode::Enter);
    fifty.cursor = Square::A2;
    key(&mut fifty, KeyCode::Char('d'));
    assert!(fifty.claimed);
    assert!(fifty.history.is_empty());
    let fifty = position("7k/8/6K1/8/8/8/8/R7 w - - 100 51");
    assert!(fifty.claimable());
    assert!(fifty.ending().is_none());
    let mut seventy_five = position("7k/8/6K1/8/8/8/8/R7 w - - 149 76");
    move_uci(&mut seventy_five, "a1a2");
    assert!(seventy_five.ending().unwrap().contains("75-move"));
    let mut mate = position("7k/8/5KQ1/8/8/8/8/8 w - - 149 76");
    move_uci(&mut mate, "g6g7");
    assert!(mate.ending().unwrap().contains("Checkmate"));
}

#[test]
fn repetition_identity_includes_rights_and_legal_en_passant() {
    let a = position("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    let b = position("r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1");
    assert_ne!(a.position, b.position);
    let a = position("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
    let b = position("4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1");
    assert_ne!(a.position, b.position);
    let a = position("4k3/8/8/3p4/8/8/8/4K3 w - d6 0 1");
    let b = position("4k3/8/8/3p4/8/8/8/4K3 w - - 7 4");
    assert_eq!(a.position, b.position);
}

#[test]
fn modules_split_rules_input_and_rendering() {
    let (min_width, min_height) = (MIN_WIDTH, MIN_HEIGHT);
    assert!(min_width == 170);
    assert!(min_height == 72);
    assert_ne!(base_bg(0, 0), base_bg(0, 1));
    assert_ne!(
        sprite_index(shakmaty::Color::White, Role::King),
        sprite_index(shakmaty::Color::Black, Role::King)
    );
    let game = Game::default();
    assert_eq!(game.cursor, Square::E2);
    assert!(game.selected.is_none());
    let legal = game.position.legal_moves();
    assert!(legal.iter().any(|m| destination(*m) == Square::E4));
}

#[test]
fn sprites_are_exact_vendored_16x16_with_checksums() {
    let expected = [
        (
            shakmaty::Color::White,
            Role::King,
            "05742e4d4b7d630951253c31da308d66cc52f5cade6b9095e200f35ffc20a742",
        ),
        (
            shakmaty::Color::White,
            Role::Queen,
            "193883cddb9dc38c91bdf793b9ff2e70c6b6f581214a8cfc3cdd3b5ab1b16b88",
        ),
        (
            shakmaty::Color::White,
            Role::Rook,
            "5373726bc21eb3d0a0337ec80bcb72dd2e288337834c3b08ae6a168d887925ff",
        ),
        (
            shakmaty::Color::White,
            Role::Bishop,
            "5329105995e44bd53ee8645b86dfda07b6e68df46af676a90ad1aba210e8ff15",
        ),
        (
            shakmaty::Color::White,
            Role::Knight,
            "9abe602f4bc32edd8f5b0c819f82e88972279eb0792da759f76c682f9be078cb",
        ),
        (
            shakmaty::Color::White,
            Role::Pawn,
            "5985267bfb128d25e187eba9e80dc60a8e2ca308a7a65273e2dc3e523099b781",
        ),
        (
            shakmaty::Color::Black,
            Role::King,
            "d468b0624b105cfbc505d009ade1ef851d19cb0d84b7ebd56237b48701e4bad4",
        ),
        (
            shakmaty::Color::Black,
            Role::Queen,
            "ffd70c3aa15a291dacc244382992ed4566354a764dccf66834317744bd7633fc",
        ),
        (
            shakmaty::Color::Black,
            Role::Rook,
            "ba36aa764140edbeab840e9fc4bf3ebba866d8b3c80cec3ce2e8791cde5a0840",
        ),
        (
            shakmaty::Color::Black,
            Role::Bishop,
            "73e1a56ba9ce3364043970d84605aad6e4d9e1e337523a502511becca653961c",
        ),
        (
            shakmaty::Color::Black,
            Role::Knight,
            "d5a703c2d857081d61237d3bbe63f31281eab2e4532ea12b0797191820e69ba1",
        ),
        (
            shakmaty::Color::Black,
            Role::Pawn,
            "e6fc1090ad72b27bd721faf8857f54f360b0b52aad3a3ddf603b9382bb1330d4",
        ),
    ];
    assert_eq!(expected.len(), 12);
    let mut unique: HashSet<&[u8]> = HashSet::new();
    for (side, role, hex) in expected {
        let bytes = sprite_bytes(side, role);
        let hash = format!("{:x}", Sha256::digest(bytes));
        assert_eq!(hash, hex, "{side:?} {role:?} checksum");
        let img = image::load_from_memory(bytes).expect("valid png");
        assert_eq!(
            (img.width(), img.height()),
            (16, 16),
            "{side:?} {role:?} size"
        );
        assert!(unique.insert(bytes), "{side:?} {role:?} duplicate bytes");
    }
    assert_eq!(unique.len(), 12);
}

#[test]
fn static_pixel_data_matches_vendored_pngs() {
    let roles = [
        Role::King,
        Role::Queen,
        Role::Rook,
        Role::Bishop,
        Role::Knight,
        Role::Pawn,
    ];
    for side in [shakmaty::Color::White, shakmaty::Color::Black] {
        for role in roles {
            let png = image::load_from_memory(sprite_bytes(side, role))
                .expect("valid png")
                .into_rgba8();
            assert_eq!((png.width(), png.height()), (16, 16));
            let static_px = sprite_pixels(side, role);
            let mut i = 0;
            for y in 0..16u32 {
                for x in 0..16u32 {
                    let p = png.get_pixel(x, y);
                    let expected = [p[0], p[1], p[2], p[3]];
                    let actual = [
                        static_px[i],
                        static_px[i + 1],
                        static_px[i + 2],
                        static_px[i + 3],
                    ];
                    assert_eq!(actual, expected, "{side:?} {role:?} pixel {x},{y}");
                    i += 4;
                }
            }
        }
    }
}

#[test]
fn squares_paint_exact_sprite_pixels() {
    let game = Game {
        cursor: Square::A1,
        ..Game::default()
    };
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (x, y) = sq(4, 6);
    let sprite = sprite_pixels(shakmaty::Color::White, Role::Pawn);
    let e2_bg = base_bg(6, 4);
    for cy in 0..SQUARE_H {
        for cx in 0..SQUARE_W {
            let top = painted(sprite, cx as u32, (cy * 2) as u32, e2_bg);
            let bottom = painted(sprite, cx as u32, (cy * 2 + 1) as u32, e2_bg);
            let cell = &buffer[(x + cx, y + cy)];
            let expected_char = if top == bottom { ' ' } else { '▀' };
            assert_eq!(
                cell.symbol(),
                &expected_char.to_string(),
                "char at {cx},{cy}"
            );
            assert_eq!(
                cell.fg,
                Color::Rgb(top[0], top[1], top[2]),
                "fg at {cx},{cy}"
            );
            assert_eq!(
                cell.bg,
                Color::Rgb(bottom[0], bottom[1], bottom[2]),
                "bg at {cx},{cy}"
            );
        }
    }
}

#[test]
fn board_paints_exact_sprites_with_highlights() {
    let mut game = Game::default();
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    for row in 0..BOARD_CELLS_H {
        for col in 0..BOARD_CELLS_W {
            let s = buffer[(BOARD_X + col, BOARD_Y + row)].symbol();
            assert!(
                s == " "
                    || s == "▀"
                    || s == "▄"
                    || s == "█"
                    || s == "┌"
                    || s == "─"
                    || s == "┐"
                    || s == "│"
                    || s == "└"
                    || s == "┘",
                "unexpected cell {s} at {col},{row}"
            );
        }
    }
    // White pawn on e2: bright interior and dark outline cells present.
    let (x, y) = sq(4, 6);
    let (mut bright, mut dark) = (false, false);
    for dy in 0..SQUARE_H {
        for dx in 0..SQUARE_W {
            let c = &buffer[(x + dx, y + dy)];
            let bright_cell = |color: Color| match color {
                Color::Rgb(r, g, b) => r > 240 && g > 240 && b > 240,
                _ => false,
            };
            let dark_cell = |color: Color| match color {
                Color::Rgb(r, g, b) => r < 120 && g < 120 && b < 120,
                _ => false,
            };
            if bright_cell(c.fg) || bright_cell(c.bg) {
                bright = true;
            }
            if dark_cell(c.fg) || dark_cell(c.bg) {
                dark = true;
            }
        }
    }
    assert!(bright, "white pawn interior not rendered on e2");
    assert!(dark, "white pawn outline not rendered on e2");

    // Select e2: empty legal e4 square keeps the exact green highlight.
    key(&mut game, KeyCode::Enter);
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (gx, gy) = sq(4, 4);
    assert_eq!(buffer[(gx + 1, gy + 1)].bg, Color::Rgb(80, 150, 80));

    // Move e2-e4: the now-empty e2 square keeps the exact blue last-move.
    key(&mut game, KeyCode::Char('k'));
    key(&mut game, KeyCode::Char('k'));
    key(&mut game, KeyCode::Enter);
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(x + 1, y + 1)].bg, Color::Rgb(80, 120, 190));

    let text = buffer_text(&terminal);
    for g in ['♔', '♕', '♖', '♗', '♘', '♙', '♚', '♛', '♜', '♝', '♞', '♟'] {
        assert!(!text.contains(g), "old Unicode glyph {g} still present");
    }
}

#[test]
fn selected_square_shows_box_outline() {
    let mut game = Game::default();
    key(&mut game, KeyCode::Enter);
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (x, y) = sq(4, 6);
    let gold = Color::Rgb(255, 210, 40);
    assert_eq!(buffer[(x, y)].symbol(), "┌");
    assert_eq!(buffer[(x, y)].fg, gold);
    assert_eq!(buffer[(x + SQUARE_W - 1, y)].symbol(), "┐");
    assert_eq!(buffer[(x, y + SQUARE_H - 1)].symbol(), "└");
    assert_eq!(buffer[(x + SQUARE_W - 1, y + SQUARE_H - 1)].symbol(), "┘");
    assert_eq!(buffer[(x + 5, y)].symbol(), "─");
    assert_eq!(buffer[(x, y + 4)].symbol(), "│");
    // The interior keeps the exact sprite pixels.
    let data = sprite_pixels(shakmaty::Color::White, Role::Pawn);
    let e2_bg = base_bg(6, 4);
    let (cx, cy) = (5u16, 3u16);
    let top = painted(data, cx as u32, (cy * 2) as u32, e2_bg);
    let bottom = painted(data, cx as u32, (cy * 2 + 1) as u32, e2_bg);
    let cell = &buffer[(x + cx, y + cy)];
    assert_eq!(cell.fg, Color::Rgb(top[0], top[1], top[2]));
    assert_eq!(cell.bg, Color::Rgb(bottom[0], bottom[1], bottom[2]));
    // Legal destination e4 keeps the green fill.
    let (gx, gy) = sq(4, 4);
    assert_eq!(buffer[(gx + 1, gy + 1)].bg, Color::Rgb(80, 150, 80));
}

#[test]
fn cursor_shows_corner_brackets() {
    let game = Game::default();
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (x, y) = sq(4, 6);
    let white = Color::Rgb(255, 255, 255);
    assert_eq!(buffer[(x, y)].symbol(), "┌");
    assert_eq!(buffer[(x, y)].fg, white);
    assert_eq!(buffer[(x + SQUARE_W - 1, y)].symbol(), "┐");
    assert_eq!(buffer[(x, y + SQUARE_H - 1)].symbol(), "└");
    assert_eq!(buffer[(x + SQUARE_W - 1, y + SQUARE_H - 1)].symbol(), "┘");
    // Non-corner cells are the exact sprite pixels, not box characters.
    let cell = &buffer[(x + 5, y + 3)];
    assert!(
        cell.symbol() == " "
            || cell.symbol() == "▀"
            || cell.symbol() == "▄"
            || cell.symbol() == "█",
        "interior must be sprite pixels"
    );
}

#[test]
fn file_labels_centered_under_each_column() {
    let game = Game::default();
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    for file in 0..8u16 {
        let col = BOARD_X + file * SQUARE_W + SQUARE_W / 2;
        let row = BOARD_Y + BOARD_CELLS_H;
        let expected = char::from(b'a' + file as u8);
        assert_eq!(
            buffer[(col, row)].symbol(),
            &expected.to_string(),
            "file label {expected} at column {col}"
        );
    }
}

#[test]
fn promotion_chooser_supports_draw_claim() {
    let mut game = position("4k3/P7/8/8/8/8/8/4K3 w - - 100 60");
    choose(&mut game, Square::A7, Square::A8);
    assert_eq!(game.promotion.len(), 4);
    key(&mut game, KeyCode::Char('d'));
    assert!(game.claimed);
    assert!(game.promotion.is_empty());
    assert!(game.history.is_empty());
    assert!(game.ending().unwrap().contains("Draw claimed"));
    let mut game = position("4k3/P7/8/8/8/8/8/4K3 w - - 0 1");
    choose(&mut game, Square::A7, Square::A8);
    assert_eq!(game.promotion.len(), 4);
    key(&mut game, KeyCode::Char('d'));
    assert!(!game.claimed);
    assert_eq!(game.promotion.len(), 4);
    assert!(game.history.is_empty());
    key(&mut game, KeyCode::Char('q'));
    assert_eq!(
        game.position.board().piece_at(Square::A8).unwrap().role,
        Role::Queen
    );
}

#[test]
fn promotion_choice_is_readable() {
    let mut game = position("4k3/P7/8/8/8/8/8/4K3 w - - 0 1");
    choose(&mut game, Square::A7, Square::A8);
    assert_eq!(game.promotion.len(), 4);
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let text = buffer_text(&terminal);
    for expected in ["Promote", "q queen", "r rook", "b bishop", "n knight"] {
        assert!(text.contains(expected), "missing {expected}");
    }
}

#[test]
fn layout_at_minimum_size_and_resize_guidance() {
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    let mut game = Game::default();
    key(&mut game, KeyCode::Enter);
    draw_terminal(&mut terminal, &game);
    let text = buffer_text(&terminal);
    for expected in [
        "Local two-player",
        "white to move",
        "Recent moves",
        "Esc: cancel",
        "Yellow box: selected",
        "Green: legal",
        "Blue: last move",
        "d: claim draw",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
    assert!(!text.contains("[P]"));
    assert!(!text.contains("Pawn _"));
    let buffer = terminal.backend().buffer();
    let (gx, gy) = sq(4, 4);
    assert_eq!(buffer[(gx + 1, gy + 1)].bg, Color::Rgb(80, 150, 80));
    terminal.backend_mut().resize(60, 20);
    draw_terminal(&mut terminal, &game);
    let text = buffer_text(&terminal);
    assert!(text.contains(&format!("Resize to at least {MIN_WIDTH}x{MIN_HEIGHT}")));
    assert!(text.contains("60x20"));
}
