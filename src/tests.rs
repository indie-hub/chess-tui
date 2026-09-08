use crate::drive_engine;
use crate::engine::Engine;
use crate::game::{Game, HumanSide, MAX_SKILL, destination};
use crate::render::{
    BOARD_CELLS_H, BOARD_CELLS_W, BOARD_X, BOARD_Y, MATERIAL_H, MATERIAL_Y, MIN_HEIGHT, MIN_WIDTH,
    PANEL_X, SQUARE_H, SQUARE_W, base_bg, draw, sprite_bytes, sprite_index,
};
use crate::sprites::{SPRITE_SIZE, sprite_pixels};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, style::Color};
use sha2::{Digest, Sha256};
use shakmaty::{CastlingMode, Chess, Position, Role, Square, fen::Fen, uci::UciMove};
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static ENGINE_TEST_LOCK: Mutex<()> = Mutex::new(());

fn engine_lock() -> std::sync::MutexGuard<'static, ()> {
    ENGINE_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn fake_engine_path() -> String {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_fake_engine") {
        return path;
    }
    // Fallback: the fake engine sits in the same target directory as the test binary.
    let target = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .and_then(|parent| parent.parent().map(|p| p.to_path_buf()));
    match target {
        Some(dir) => dir.join("fake_engine").to_string_lossy().into_owned(),
        None => "fake_engine".into(),
    }
}

fn wait_bestmove(engine: &mut Engine, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match engine.try_bestmove() {
            Some(Ok(mv)) => return Some(mv),
            Some(Err(_)) => return None,
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    None
}

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
    key(&mut game, KeyCode::Char('n'));
    assert!(game.configuring);
    assert_eq!(game.history, ["e4"]);
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
    assert!(min_height == 68);
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

    // Select e2: empty legal e4 shows the small centred green marker.
    key(&mut game, KeyCode::Enter);
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (gx, gy) = sq(4, 4);
    assert_eq!(buffer[(gx + 7, gy + 3)].bg, Color::Rgb(60, 200, 60));
    assert_eq!(buffer[(gx + 1, gy + 1)].bg, Color::Rgb(158, 158, 158));

    // Move e2-e4: the now-empty e2 keeps a subtle blue outline.
    key(&mut game, KeyCode::Char('k'));
    key(&mut game, KeyCode::Char('k'));
    key(&mut game, KeyCode::Enter);
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(x, y)].symbol(), "┌");
    assert_eq!(buffer[(x, y)].fg, Color::Rgb(90, 130, 205));
    assert_eq!(buffer[(x + 5, y + 3)].bg, Color::Rgb(158, 158, 158));

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
    // Legal destination e4 shows the small centred green marker.
    let (gx, gy) = sq(4, 4);
    assert_eq!(buffer[(gx + 7, gy + 3)].bg, Color::Rgb(60, 200, 60));
    assert_eq!(buffer[(gx + 1, gy + 1)].bg, Color::Rgb(158, 158, 158));
}

#[test]
fn cursor_shows_corner_brackets() {
    let game = Game::default();
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (x, y) = sq(4, 6);
    let white = Color::Rgb(0, 220, 255);
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
fn capturable_destination_shows_amber_outline() {
    let mut game = position("4k3/8/8/8/1b6/8/3QK3/8 w - - 0 1");
    game.cursor = Square::D2;
    key(&mut game, KeyCode::Enter);
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    // b4 is a capturable destination: amber outline, no fill, piece stays exact.
    let (dx, dy) = sq(1, 4);
    assert_eq!(buffer[(dx, dy)].symbol(), "┌");
    assert_eq!(buffer[(dx, dy)].fg, Color::Rgb(230, 80, 20));
    // e1 is an empty legal destination: green centred marker.
    let (ex, ey) = sq(4, 7);
    assert_eq!(buffer[(ex + 7, ey + 3)].bg, Color::Rgb(60, 200, 60));
}

#[test]
fn selection_takes_precedence_over_last_move_outline() {
    let mut game = position("4k3/8/8/8/4P3/8/8/4K3 w - - 0 1");
    game.last_move = Some((Square::E2, Square::E4));
    game.cursor = Square::E4;
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (x, y) = sq(4, 4);
    assert_eq!(
        buffer[(x + 5, y)].symbol(),
        "─",
        "top edge line before selection"
    );
    assert_eq!(
        buffer[(x + 5, y)].fg,
        Color::Rgb(90, 130, 205),
        "last-move blue outline before selection"
    );
    key(&mut game, KeyCode::Enter); // cursor is on e4; select the pawn
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    assert_eq!(
        buffer[(x + 5, y)].fg,
        Color::Rgb(255, 210, 40),
        "selection gold outline wins over last-move blue"
    );
}

#[test]
fn cursor_on_legal_destination_shows_white_corners_and_marker() {
    let mut game = Game::default();
    key(&mut game, KeyCode::Enter); // select e2
    game.cursor = Square::E4;
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (x, y) = sq(4, 4);
    assert_eq!(buffer[(x, y)].symbol(), "┌");
    assert_eq!(buffer[(x, y)].fg, Color::Rgb(0, 220, 255));
    assert_eq!(buffer[(x + 7, y + 3)].bg, Color::Rgb(60, 200, 60));
}

#[test]
fn cursor_on_capture_shows_white_corners_and_amber_outline() {
    let mut game = position("4k3/8/8/8/1b6/8/3QK3/8 w - - 0 1");
    game.cursor = Square::D2;
    key(&mut game, KeyCode::Enter);
    game.cursor = Square::B4;
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (x, y) = sq(1, 4);
    assert_eq!(buffer[(x, y)].symbol(), "┌");
    assert_eq!(
        buffer[(x, y)].fg,
        Color::Rgb(0, 220, 255),
        "cursor corner visible"
    );
    assert_eq!(
        buffer[(x + 5, y)].fg,
        Color::Rgb(230, 80, 20),
        "amber outline stays visible"
    );
}

#[test]
fn cursor_on_last_move_shows_white_corners_and_blue_outline() {
    let mut game = position("4k3/8/8/8/4P3/8/8/4K3 w - - 0 1");
    game.last_move = Some((Square::E2, Square::E4));
    game.cursor = Square::E4;
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (x, y) = sq(4, 4);
    assert_eq!(buffer[(x, y)].symbol(), "┌");
    assert_eq!(
        buffer[(x, y)].fg,
        Color::Rgb(0, 220, 255),
        "cursor corner visible"
    );
    assert_eq!(
        buffer[(x + 5, y)].fg,
        Color::Rgb(90, 130, 205),
        "blue outline stays visible"
    );
}

#[test]
fn legal_marker_takes_precedence_over_last_move() {
    // The king's legal move back to e2 must win over the e2 last-move outline.
    let mut game = position("4k3/8/8/8/4P3/8/8/4K3 w - - 0 1");
    game.last_move = Some((Square::E2, Square::E4));
    game.cursor = Square::E1;
    key(&mut game, KeyCode::Enter);
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (x, y) = sq(4, 6);
    assert_eq!(
        buffer[(x + 7, y + 3)].bg,
        Color::Rgb(60, 200, 60),
        "green marker wins"
    );
    assert_ne!(
        buffer[(x, y)].symbol(),
        "┌",
        "no blue outline on a legal marker square"
    );
}

#[test]
fn capture_outline_preserves_exact_sprite_interior() {
    let mut game = position("4k3/8/8/8/1b6/8/3QK3/8 w - - 0 1");
    game.cursor = Square::D2;
    key(&mut game, KeyCode::Enter);
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (x, y) = sq(1, 4);
    let data = sprite_pixels(shakmaty::Color::Black, Role::Bishop);
    let e2_bg = [110, 110, 110];
    let (cx, cy) = (8u16, 4u16);
    let top = painted(data, cx as u32, (cy * 2) as u32, e2_bg);
    let bottom = painted(data, cx as u32, (cy * 2 + 1) as u32, e2_bg);
    let cell = &buffer[(x + cx, y + cy)];
    assert_eq!(
        cell.fg,
        Color::Rgb(top[0], top[1], top[2]),
        "capture interior exact"
    );
    assert_eq!(
        cell.bg,
        Color::Rgb(bottom[0], bottom[1], bottom[2]),
        "capture interior exact"
    );
}

#[test]
fn header_reflects_active_mode_and_engine_failure() {
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    let game = Game::new_vs_engine(shakmaty::Color::Black);
    draw_terminal(&mut terminal, &game);
    let text = buffer_text(&terminal);
    assert!(
        text.contains("You: White vs Stockfish"),
        "engine-mode header"
    );
    let mut failed = Game::new_vs_engine(shakmaty::Color::Black);
    failed.engine_failed = true;
    draw_terminal(&mut terminal, &failed);
    let text = buffer_text(&terminal);
    assert!(
        text.contains("Local two-player"),
        "failed engine header must not claim Stockfish"
    );
    assert!(!text.contains("vs Stockfish"));
    let local = Game::default();
    draw_terminal(&mut terminal, &local);
    assert!(buffer_text(&terminal).contains("Local two-player"));
}

#[test]
fn long_engine_status_wraps_in_panel() {
    let mut game = Game::new_vs_engine(shakmaty::Color::Black);
    game.engine_status =
        "Engine error: engine not found (set STOCKFISH_PATH or place stockfish next to the binary)"
            .into();
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let text = buffer_text(&terminal);
    for expected in [
        "STOCKFISH_PATH",
        "next to the binary",
        "white to move",
        "Recent moves",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
}

#[test]
fn material_panel_shows_captures_and_white_balance() {
    let mut game = position("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1");
    move_uci(&mut game, "e4d5");
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let text = buffer_text(&terminal);
    assert!(text.contains("Captured"), "material block title");
    assert!(
        text.contains("White +1"),
        "signed balance for white capture"
    );
    assert!(
        text.contains("♟"),
        "white-captured black pawn figurine shown"
    );
    assert!(!text.contains("♙"), "no white pawn in white capture row");
}

#[test]
fn material_panel_black_advantage_and_local_mode() {
    let mut game = position("r3k3/8/8/8/8/8/P7/4K3 b - - 0 1");
    move_uci(&mut game, "a8a2");
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let text = buffer_text(&terminal);
    assert!(
        text.contains("Black +1"),
        "signed balance for black capture"
    );
    assert!(
        text.contains("♙"),
        "black-captured white pawn figurine shown"
    );
    assert!(text.contains("Local two-player"), "works in local mode");
}

#[test]
fn material_panel_equal_state_and_reset_paths() {
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    let mut game = position("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1");
    move_uci(&mut game, "e4d5");
    draw_terminal(&mut terminal, &game);
    let text = buffer_text(&terminal);
    assert!(text.contains("White +1"), "balance before reset");
    let fresh = Game::default();
    draw_terminal(&mut terminal, &fresh);
    assert!(
        buffer_text(&terminal).contains("Equal"),
        "equal state shown unambiguously"
    );
    let mut vs = Game::new_vs_engine(shakmaty::Color::Black);
    vs.engine_status = "Stockfish thinking...".into();
    draw_terminal(&mut terminal, &vs);
    let text = buffer_text(&terminal);
    assert!(text.contains("Equal"), "reset engine game is equal");
    assert!(text.contains("You: White vs Stockfish"), "engine mode");
}

#[test]
fn material_panel_fits_without_overlap_both_orientations() {
    let mut game = position("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1");
    move_uci(&mut game, "e4d5");
    for engine_side in [shakmaty::Color::Black, shakmaty::Color::White] {
        game.versus_engine = true;
        game.engine_side = engine_side;
        let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
        draw_terminal(&mut terminal, &game);
        let buffer = terminal.backend().buffer();
        let area = buffer.area;
        assert!(
            area.width >= MIN_WIDTH && area.height >= MIN_HEIGHT,
            "buffer at least minimum size"
        );
        // The material block must not collide with the recent-moves block
        // (rows 5..=17) or the footer (rows 66..=67).
        assert_eq!(
            buffer[(PANEL_X, MATERIAL_Y)].symbol(),
            "┌",
            "material top border in either orientation"
        );
        assert_eq!(
            buffer[(PANEL_X, MATERIAL_Y + MATERIAL_H - 1)].symbol(),
            "└",
            "material bottom border in either orientation"
        );
        assert_eq!(
            buffer[(PANEL_X, 4)].symbol(),
            " ",
            "row above material is blank, no overlap with recent moves"
        );
        assert_eq!(
            buffer[(PANEL_X, 65)].symbol(),
            " ",
            "row below material is blank, no overlap with footer"
        );
    }
}

#[test]
fn legal_marker_visible_on_light_and_dark_squares() {
    let mut game = position("4k3/8/8/8/8/8/8/R3K3 w - - 0 1");
    game.cursor = Square::E1;
    key(&mut game, KeyCode::Enter);
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (lx, ly) = sq(3, 7); // d1 (light square)
    assert_eq!(
        buffer[(lx + 7, ly + 3)].bg,
        Color::Rgb(60, 200, 60),
        "light square marker"
    );
    let (dx, dy) = sq(3, 6); // d2 (dark square)
    assert_eq!(
        buffer[(dx + 7, dy + 3)].bg,
        Color::Rgb(60, 200, 60),
        "dark square marker"
    );
}

#[test]
fn cursor_corners_visible_on_light_and_dark_squares() {
    let mut game = Game {
        cursor: Square::A1,
        ..Game::default()
    };
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (x, y) = sq(0, 7);
    assert_eq!(
        buffer[(x, y)].fg,
        Color::Rgb(0, 220, 255),
        "cursor corner on dark square"
    );
    game.cursor = Square::A8; // light square
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (x2, y2) = sq(0, 0);
    assert_eq!(
        buffer[(x2, y2)].fg,
        Color::Rgb(0, 220, 255),
        "cursor corner on light square"
    );
}

#[test]
fn capture_outline_works_on_both_base_colors_and_piece_colors() {
    // Dark square + black piece: queen d2 captures the bishop on b4.
    let mut game = position("4k3/8/8/8/1b6/8/3QK3/8 w - - 0 1");
    game.cursor = Square::D2;
    key(&mut game, KeyCode::Enter);
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    let (dx, dy) = sq(1, 4);
    assert_eq!(
        buffer[(dx, dy)].fg,
        Color::Rgb(230, 80, 20),
        "dark square amber outline"
    );
    let data = sprite_pixels(shakmaty::Color::Black, Role::Bishop);
    let top = painted(data, 8, 8, [110, 110, 110]);
    let bottom = painted(data, 8, 9, [110, 110, 110]);
    let cell = &buffer[(dx + 8, dy + 4)];
    assert_eq!(
        cell.fg,
        Color::Rgb(top[0], top[1], top[2]),
        "dark capture interior exact"
    );
    assert_eq!(
        cell.bg,
        Color::Rgb(bottom[0], bottom[1], bottom[2]),
        "dark capture interior exact"
    );

    // Light square + white piece: black knight d4 captures the pawn on b5.
    let mut game2 = position("4k3/8/8/1P6/3n4/8/8/4K3 b - - 0 1");
    game2.cursor = Square::D4;
    key(&mut game2, KeyCode::Enter);
    let mut terminal2 = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal2, &game2);
    let buffer2 = terminal2.backend().buffer();
    let (ex, ey) = sq(1, 3);
    assert_eq!(
        buffer2[(ex, ey)].fg,
        Color::Rgb(230, 80, 20),
        "light square amber outline"
    );
    let data2 = sprite_pixels(shakmaty::Color::White, Role::Pawn);
    let top2 = painted(data2, 8, 8, [158, 158, 158]);
    let bottom2 = painted(data2, 8, 9, [158, 158, 158]);
    let cell2 = &buffer2[(ex + 8, ey + 4)];
    assert_eq!(
        cell2.fg,
        Color::Rgb(top2[0], top2[1], top2[2]),
        "light capture interior exact"
    );
    assert_eq!(
        cell2.bg,
        Color::Rgb(bottom2[0], bottom2[1], bottom2[2]),
        "light capture interior exact"
    );
}

#[test]
fn en_passant_capture_shows_amber_outline_on_empty_square() {
    let mut game = position("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
    game.cursor = Square::E5;
    key(&mut game, KeyCode::Enter);
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    // d6 is the en passant destination (empty): amber capture outline.
    let (dx, dy) = sq(3, 2);
    assert_eq!(buffer[(dx, dy)].symbol(), "┌");
    assert_eq!(
        buffer[(dx, dy)].fg,
        Color::Rgb(230, 80, 20),
        "en passant amber outline"
    );
    // e6 is a plain empty push: green marker.
    let (ex, ey) = sq(4, 2);
    assert_eq!(
        buffer[(ex + 7, ey + 3)].bg,
        Color::Rgb(60, 200, 60),
        "empty push green marker"
    );
}

#[test]
fn draw_hint_shows_only_when_claimable() {
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    let game = Game::default();
    draw_terminal(&mut terminal, &game);
    assert!(
        !buffer_text(&terminal).contains("Draw available"),
        "no draw hint when not claimable"
    );
    let cycle = ["g1f3", "g8f6", "f3g1", "f6g8"];
    let mut game = Game::default();
    for _ in 0..2 {
        for m in cycle {
            move_uci(&mut game, m);
        }
    }
    assert!(game.claimable());
    assert!(game.ending().is_none());
    draw_terminal(&mut terminal, &game);
    assert!(buffer_text(&terminal).contains("Draw available: press d"));
    // A pending notice takes the dynamic line instead of the hint.
    game.notice = "Selected e2. Choose a legal destination.".into();
    draw_terminal(&mut terminal, &game);
    let text = buffer_text(&terminal);
    assert!(text.contains("Selected e2"));
    assert!(!text.contains("Draw available"));
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
fn black_side_rotates_board_coordinates_and_cursor_movement() {
    let mut game = Game::new_vs_engine(shakmaty::Color::White);
    assert!(game.board_flipped());
    assert_eq!(game.cursor, Square::E7);

    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let buffer = terminal.backend().buffer();
    for file in 0..8u16 {
        let col = BOARD_X + file * SQUARE_W + SQUARE_W / 2;
        let expected = char::from(b'h' - file as u8);
        assert_eq!(
            buffer[(col, BOARD_Y + BOARD_CELLS_H)].symbol(),
            expected.to_string()
        );
    }
    for row in 0..8u16 {
        assert_eq!(
            buffer[(0, BOARD_Y + row * SQUARE_H + 3)].symbol(),
            (row + 1).to_string()
        );
    }
    let (cursor_x, cursor_y) = sq(3, 6);
    assert_eq!(buffer[(cursor_x, cursor_y)].fg, Color::Rgb(0, 220, 255));

    key(&mut game, KeyCode::Up);
    assert_eq!(game.cursor, Square::E6);
    key(&mut game, KeyCode::Left);
    assert_eq!(game.cursor, Square::F6);
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
fn engine_starts_and_bestmove_enters_through_game_play() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "");
    }
    let mut engine = Engine::spawn(Path::new(&fake_engine_path())).expect("spawn");
    engine.start_search(START_FEN).expect("search start");
    let mv = wait_bestmove(&mut engine, Duration::from_secs(3)).expect("bestmove");
    assert_eq!(mv, "e2e4");

    let mut game = Game::new_vs_engine(shakmaty::Color::Black);
    let parsed = UciMove::from_ascii(mv.as_bytes())
        .expect("parses")
        .to_move(&game.position)
        .expect("legal move");
    game.play(parsed);
    assert_eq!(game.history, ["e4"]);
    assert_eq!(game.position.turn(), shakmaty::Color::Black);
    assert!(game.engine_to_move());
    assert!(!engine.is_searching());
    drop(engine);
}

#[test]
fn engine_startup_ready_timeout_is_reported() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "no-ready");
    }
    let err = Engine::spawn(Path::new(&fake_engine_path()))
        .err()
        .expect("startup error");
    assert!(err.message().contains("not ready"));
}

#[test]
fn engine_illegal_bestmove_is_rejected_by_legality() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "illegal");
    }
    let mut engine = Engine::spawn(Path::new(&fake_engine_path())).expect("spawn");
    engine.start_search(START_FEN).expect("search start");
    let mv = wait_bestmove(&mut engine, Duration::from_secs(3)).expect("bestmove");
    assert_eq!(mv, "e2e5");
    let game = Game::new_vs_engine(shakmaty::Color::Black);
    // The illegal pawn move must not convert into a playable Move.
    let parsed = UciMove::from_ascii(mv.as_bytes()).unwrap();
    assert!(parsed.to_move(&game.position).is_err());
}

#[test]
fn engine_no_bestmove_is_a_protocol_error() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "none");
    }
    let mut engine = Engine::spawn(Path::new(&fake_engine_path())).expect("spawn");
    engine.start_search(START_FEN).expect("search start");
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut got_error = false;
    while Instant::now() < deadline {
        if let Some(Err(err)) = engine.try_bestmove() {
            assert!(err.message().contains("protocol"));
            got_error = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(got_error, "expected empty bestmove protocol error");
}

#[test]
fn engine_search_stays_pending_without_response() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "timeout");
    }
    let mut engine = Engine::spawn(Path::new(&fake_engine_path())).expect("spawn");
    engine.start_search(START_FEN).expect("search start");
    std::thread::sleep(Duration::from_millis(100));
    assert!(engine.is_searching());
    assert!(engine.try_bestmove().is_none());
    assert!(engine.thinking_status().contains("Engine thinking"));
}

#[test]
fn engine_ignored_quit_is_killed_on_drop() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "ignore-quit");
    }
    let engine = Engine::spawn(Path::new(&fake_engine_path())).expect("spawn");
    drop(engine);
}

#[test]
fn engine_lookup_override_takes_precedence() {
    let root = engine_lookup_root();
    engine_lookup_temp_tree(&root, true, true);
    let resolved = crate::engine::resolve_engine_path_from(
        Some("/custom/stockfish"),
        Some(&root.join("bin")),
        &root.join("src"),
    );
    assert_eq!(
        resolved,
        Some(std::path::PathBuf::from("/custom/stockfish"))
    );
    // An override pointing at a missing path still wins (spawn reports the error).
    let resolved = crate::engine::resolve_engine_path_from(
        Some("/nonexistent/stockfish"),
        Some(&root.join("bin")),
        &root.join("src"),
    );
    assert_eq!(
        resolved,
        Some(std::path::PathBuf::from("/nonexistent/stockfish"))
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn engine_lookup_sibling_before_staged() {
    let root = engine_lookup_root();
    engine_lookup_temp_tree(&root, true, true);
    let resolved =
        crate::engine::resolve_engine_path_from(None, Some(&root.join("bin")), &root.join("src"));
    assert_eq!(resolved, Some(root.join("bin/stockfish")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn engine_lookup_staged_source_tree_when_no_sibling() {
    let root = engine_lookup_root();
    engine_lookup_temp_tree(&root, false, true);
    let resolved =
        crate::engine::resolve_engine_path_from(None, Some(&root.join("bin")), &root.join("src"));
    assert_eq!(
        resolved,
        Some(root.join("src/third_party/stockfish/bundle/stockfish-macos-universal"))
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn engine_lookup_missing_paths_return_none() {
    let root = engine_lookup_root();
    engine_lookup_temp_tree(&root, false, false);
    let resolved =
        crate::engine::resolve_engine_path_from(None, Some(&root.join("bin")), &root.join("src"));
    assert_eq!(resolved, None);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn engine_real_lookup_finds_staged_source_tree_binary() {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resolved = crate::engine::resolve_engine_path_from(None, None, &manifest);
    assert_eq!(
        resolved,
        Some(manifest.join("third_party/stockfish/bundle/stockfish-macos-universal"))
    );
}

#[test]
fn engine_turn_blocks_move_input_but_keeps_cursor_and_new_game_config() {
    let mut game = position("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 0 1");
    game.versus_engine = true;
    game.engine_side = shakmaty::Color::Black;
    assert!(game.engine_to_move());
    key(&mut game, KeyCode::Enter);
    assert_eq!(game.selected, None, "Enter must be blocked on engine turn");
    game.cursor = Square::E7;
    key(&mut game, KeyCode::Char('k'));
    assert_eq!(game.cursor, Square::E8, "cursor movement stays enabled");
    key(&mut game, KeyCode::Char('n'));
    assert!(game.versus_engine);
    assert!(game.configuring, "N opens configuration on engine turn");
}

#[test]
fn engine_switch_sides_toggles_and_restarts() {
    let mut game = Game::new_vs_engine(shakmaty::Color::Black);
    game.engine_skill = 7;
    assert!(game.versus_engine);
    assert_eq!(game.engine_side, shakmaty::Color::Black);
    game.switch_sides();
    assert_eq!(game.engine_side, shakmaty::Color::White);
    assert_eq!(game.engine_skill, 7);
    assert!(game.board_flipped());
    assert_eq!(game.cursor, Square::E7);
    assert_eq!(game.position, Chess::default());
}

fn temp_nonexecutable() -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("fake_engine_noexec_{}", std::process::id()));
    std::fs::write(&path, b"#!/bin/sh\nexit 0\n").unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o000);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

fn engine_lookup_root() -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("chess_lookup_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn engine_lookup_temp_tree(root: &std::path::Path, with_sibling: bool, with_staged: bool) {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    if with_sibling {
        std::fs::write(bin.join("stockfish"), b"x").unwrap();
    }
    let staged = root.join("src/third_party/stockfish/bundle");
    if with_staged {
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(staged.join("stockfish-macos-universal"), b"x").unwrap();
    }
}

#[test]
fn engine_missing_and_nonexecutable_are_graceful_errors() {
    let _guard = engine_lock();
    // Missing path: spawn fails.
    assert!(crate::engine::Engine::spawn(Path::new("/nonexistent/stockfish")).is_err());
    // Non-executable file: spawn is refused.
    let path = temp_nonexecutable();
    let err = crate::engine::Engine::spawn(&path)
        .err()
        .expect("spawn error");
    assert!(!err.message().is_empty());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn engine_silent_uciok_times_out() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "no-uci");
    }
    let err = Engine::spawn(Path::new(&fake_engine_path()))
        .err()
        .expect("startup error");
    assert!(
        err.message().contains("startup failed"),
        "{}",
        err.message()
    );
}

#[test]
fn engine_delayed_handshake_still_starts() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "delayed");
    }
    let engine = Engine::spawn(Path::new(&fake_engine_path())).expect("starts despite delay");
    drop(engine);
}

#[test]
fn engine_early_exit_is_reported() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "early-exit");
    }
    let err = Engine::spawn(Path::new(&fake_engine_path()))
        .err()
        .expect("startup error");
    assert!(err.message().contains("startup failed") || err.message().contains("protocol"));
}

#[test]
fn engine_delayed_bestmove_eventually_arrives() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "slow");
    }
    let mut engine = Engine::spawn(Path::new(&fake_engine_path())).expect("spawn");
    engine.start_search(START_FEN).expect("search start");
    assert!(
        engine.try_bestmove().is_none(),
        "must still be pending immediately"
    );
    let mv = wait_bestmove(&mut engine, Duration::from_secs(3)).expect("bestmove");
    assert_eq!(mv, "e2e4");
}

#[test]
fn engine_malformed_bestmove_is_rejected() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "malformed");
    }
    let mut engine = Engine::spawn(Path::new(&fake_engine_path())).expect("spawn");
    engine.start_search(START_FEN).expect("search start");
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut rejected = false;
    while Instant::now() < deadline {
        match engine.try_bestmove() {
            Some(Ok(mv)) => {
                assert!(
                    UciMove::from_ascii(mv.as_bytes()).is_err(),
                    "malformed move parsed"
                );
                rejected = true;
                break;
            }
            Some(Err(_)) => {
                rejected = true;
                break;
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    assert!(rejected, "malformed bestmove not rejected");
}

#[test]
fn special_moves_convert_through_game_play() {
    // Promotion: a7 pawn promotes to a queen.
    let mut game = position("4k3/P7/8/8/8/8/8/4K3 w - - 0 1");
    let m = UciMove::from_ascii(b"a7a8q")
        .unwrap()
        .to_move(&game.position)
        .unwrap();
    game.play(m);
    assert_eq!(
        game.position.board().piece_at(Square::A8).unwrap().role,
        Role::Queen
    );

    // Castling kingside: king e1 to g1, rook to f1.
    let mut game = position("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    let m = UciMove::from_ascii(b"e1g1")
        .unwrap()
        .to_move(&game.position)
        .unwrap();
    game.play(m);
    assert_eq!(
        game.position.board().piece_at(Square::G1).unwrap().role,
        Role::King
    );
    assert_eq!(
        game.position.board().piece_at(Square::F1).unwrap().role,
        Role::Rook
    );

    // En passant capture: e5 takes d5 en passant.
    let mut game = position("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
    let m = UciMove::from_ascii(b"e5d6")
        .unwrap()
        .to_move(&game.position)
        .unwrap();
    game.play(m);
    assert!(game.position.board().piece_at(Square::D5).is_none());
    assert_eq!(
        game.position.board().piece_at(Square::D6).unwrap().role,
        Role::Pawn
    );
}

#[test]
fn engine_quit_while_thinking_reaps_child() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "slow");
    }
    let mut engine = Engine::spawn(Path::new(&fake_engine_path())).expect("spawn");
    engine.start_search(START_FEN).expect("search start");
    std::thread::sleep(Duration::from_millis(200));
    drop(engine); // quit during thinking; sleeping fake is killed, then reaped
}

#[test]
fn engine_restart_while_thinking_starts_fresh_search() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "");
    }
    let mut engine = Engine::spawn(Path::new(&fake_engine_path())).expect("spawn");
    // Engine starts searching the initial position (would answer e2e4).
    engine.start_search(START_FEN).expect("search start");
    std::thread::sleep(Duration::from_millis(100));
    // Restart resets the game; the in-flight search must be cancelled.
    let mut game = Game::new_vs_engine(shakmaty::Color::Black);
    drive_engine(&mut engine, &mut game);
    assert!(!engine.is_searching(), "stale search must be cancelled");
    assert!(
        engine.last_fen().is_none(),
        "stale search fen must be cleared"
    );
    // Human moves first, then the engine searches the fresh position.
    let m = UciMove::from_ascii(b"d2d4")
        .unwrap()
        .to_move(&game.position)
        .unwrap();
    game.play(m);
    drive_engine(&mut engine, &mut game);
    assert_eq!(
        engine.last_fen().map(|fen| fen.contains(" b ")),
        Some(true),
        "engine must search the fresh black-to-move position"
    );
    let mv = wait_bestmove(&mut engine, Duration::from_secs(3)).expect("bestmove");
    assert_eq!(mv, "e7e5");
}

#[test]
fn engine_error_marks_game_failed_without_retry_loop() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "illegal");
    }
    let mut engine = Engine::spawn(Path::new(&fake_engine_path())).expect("spawn");
    let mut game = Game::new_vs_engine(shakmaty::Color::Black);
    let m = UciMove::from_ascii(b"e2e4")
        .unwrap()
        .to_move(&game.position)
        .unwrap();
    game.play(m);
    for _ in 0..100 {
        drive_engine(&mut engine, &mut game);
        if game.engine_failed {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(game.engine_failed, "engine failure must be latched");
    assert!(
        game.engine_status.contains("illegal move"),
        "{}",
        game.engine_status
    );
    drive_engine(&mut engine, &mut game);
    assert!(game.engine_failed, "failed engine must not be retried");
}

#[test]
fn repeated_spawn_drop_reaps_children() {
    let _guard = engine_lock();
    unsafe {
        std::env::set_var("FAKE_ENGINE_MODE", "ignore-quit");
    }
    for _ in 0..3 {
        let engine = Engine::spawn(Path::new(&fake_engine_path())).expect("spawn");
        drop(engine);
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
        "Esc cancel",
        "Enter move",
        "corners=cursor",
        "gold=selected",
        "green=legal",
        "amber=capture",
        "blue=last",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
    assert!(!text.contains("[P]"));
    assert!(!text.contains("Pawn _"));
    let buffer = terminal.backend().buffer();
    let (gx, gy) = sq(4, 4);
    assert_eq!(buffer[(gx + 7, gy + 3)].bg, Color::Rgb(60, 200, 60));
    terminal.backend_mut().resize(60, 20);
    draw_terminal(&mut terminal, &game);
    let text = buffer_text(&terminal);
    assert!(text.contains(&format!("Resize to at least {MIN_WIDTH}x{MIN_HEIGHT}")));
    assert!(text.contains("60x20"));
}

#[test]
fn new_game_configuration_selects_side_and_bounded_skill() {
    let mut game = Game::new_vs_engine(shakmaty::Color::Black);
    key(&mut game, KeyCode::Char('N'));
    assert!(game.configuring);

    key(&mut game, KeyCode::Right);
    assert_eq!(game.config_side, HumanSide::Black);
    key(&mut game, KeyCode::Right);
    assert_eq!(game.config_side, HumanSide::Random);

    for _ in 0..30 {
        key(&mut game, KeyCode::Up);
    }
    assert_eq!(game.engine_skill, MAX_SKILL);
    for _ in 0..30 {
        key(&mut game, KeyCode::Down);
    }
    assert_eq!(game.engine_skill, 0);

    key(&mut game, KeyCode::Enter);
    assert_eq!(game.take_new_game_request(), Some((HumanSide::Random, 0)));
    assert!(!game.configuring);
}

#[test]
fn new_game_configuration_renders_and_can_be_cancelled() {
    let mut game = Game::default();
    game.open_new_game_config();
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    draw_terminal(&mut terminal, &game);
    let text = buffer_text(&terminal);
    for expected in [
        "New game",
        "Human side",
        "White",
        "Stockfish skill",
        "Enter: start game",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
    assert!(!text.contains("Recent moves"));
    key(&mut game, KeyCode::Right);
    key(&mut game, KeyCode::Down);
    key(&mut game, KeyCode::Esc);
    assert!(!game.configuring);
    assert_eq!(game.config_side, HumanSide::White);
    assert_eq!(game.engine_skill, MAX_SKILL);
    assert_eq!(game.take_new_game_request(), None);
}

#[test]
fn random_side_resolution_covers_both_colours() {
    assert_eq!(
        crate::resolve_human_side(HumanSide::Random, true),
        shakmaty::Color::White
    );
    assert_eq!(
        crate::resolve_human_side(HumanSide::Random, false),
        shakmaty::Color::Black
    );
    assert_eq!(
        crate::resolve_human_side(HumanSide::White, false),
        shakmaty::Color::White
    );
    assert_eq!(
        crate::resolve_human_side(HumanSide::Black, true),
        shakmaty::Color::Black
    );
}

#[test]
fn engine_accepts_skill_configuration_before_search() {
    let _guard = engine_lock();
    unsafe { std::env::remove_var("FAKE_ENGINE_MODE") };
    let mut engine = Engine::spawn(Path::new(&fake_engine_path())).expect("spawn");
    engine.configure_skill(7).expect("configure skill");
    engine.start_search(START_FEN).expect("search start");
    assert!(wait_bestmove(&mut engine, Duration::from_secs(2)).is_some());
}
