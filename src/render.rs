use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
#[cfg(test)]
use shakmaty::{Color as Side, Role};
use shakmaty::{Move, Position, Square};

use crate::game::{Game, destination};
use crate::sprites::{SPRITE_SIZE, sprite_pixels};

pub(crate) const MIN_WIDTH: u16 = 170;
pub(crate) const MIN_HEIGHT: u16 = 68;
// Each square is 16 columns wide and 8 rows tall of terminal cells: a 16x16
// sprite occupies 16 columns (one sprite pixel per cell width) and 8 rows
// (two sprite pixel rows per half-block cell).
pub(crate) const SQUARE_W: u16 = 16;
pub(crate) const SQUARE_H: u16 = 8;
#[cfg(test)]
pub(crate) const BOARD_CELLS_W: u16 = 128;
pub(crate) const BOARD_CELLS_H: u16 = 64;
pub(crate) const BOARD_X: u16 = 2;
pub(crate) const BOARD_Y: u16 = 1;
pub(crate) const PANEL_X: u16 = 134;
pub(crate) const PANEL_WIDTH: u16 = 34;

// Square decorations. All are high-contrast against the mid-tone square
// colours, the marker colour and the piece artwork.
const SELECTED_OUTLINE: [u8; 3] = [255, 210, 40];
const CURSOR_OUTLINE: [u8; 3] = [0, 220, 255];
const CAPTURE_OUTLINE: [u8; 3] = [230, 80, 20];
const LEGAL_MARKER: [u8; 3] = [60, 200, 60];
const LAST_MOVE_OUTLINE: [u8; 3] = [90, 130, 205];

// Center of a 16x8 square for the empty-legal marker: a 4x4 centred block that
// covers <=25% of the square and keeps the base colour visible around it.
const MARKER_CX0: u16 = 6;
const MARKER_CX1: u16 = 9;
const MARKER_CY0: u16 = 2;
const MARKER_CY1: u16 = 5;

#[cfg(test)]
pub(crate) fn sprite_bytes(side: Side, role: Role) -> &'static [u8] {
    match (side, role) {
        (Side::White, Role::King) => include_bytes!("../assets/sprites/white_king.png"),
        (Side::White, Role::Queen) => include_bytes!("../assets/sprites/white_queen.png"),
        (Side::White, Role::Rook) => include_bytes!("../assets/sprites/white_rook.png"),
        (Side::White, Role::Bishop) => include_bytes!("../assets/sprites/white_bishop.png"),
        (Side::White, Role::Knight) => include_bytes!("../assets/sprites/white_knight.png"),
        (Side::White, Role::Pawn) => include_bytes!("../assets/sprites/white_pawn.png"),
        (Side::Black, Role::King) => include_bytes!("../assets/sprites/black_king.png"),
        (Side::Black, Role::Queen) => include_bytes!("../assets/sprites/black_queen.png"),
        (Side::Black, Role::Rook) => include_bytes!("../assets/sprites/black_rook.png"),
        (Side::Black, Role::Bishop) => include_bytes!("../assets/sprites/black_bishop.png"),
        (Side::Black, Role::Knight) => include_bytes!("../assets/sprites/black_knight.png"),
        (Side::Black, Role::Pawn) => include_bytes!("../assets/sprites/black_pawn.png"),
    }
}

#[cfg(test)]
fn role_index(role: Role) -> usize {
    match role {
        Role::King => 0,
        Role::Queen => 1,
        Role::Rook => 2,
        Role::Bishop => 3,
        Role::Knight => 4,
        Role::Pawn => 5,
    }
}

#[cfg(test)]
fn side_index(side: Side) -> usize {
    match side {
        Side::White => 0,
        Side::Black => 1,
    }
}

#[cfg(test)]
pub(crate) fn sprite_index(side: Side, role: Role) -> usize {
    side_index(side) * 6 + role_index(role)
}

fn to_rgb(color: [u8; 3]) -> Color {
    Color::Rgb(color[0], color[1], color[2])
}

// A sprite pixel is painted as-is; a transparent pixel keeps the square colour.
// The static pixel buffer holds 16x16 RGBA bytes in row-major order.
fn pixel_color(data: &[u8; SPRITE_SIZE * SPRITE_SIZE * 4], x: u32, y: u32, bg: [u8; 3]) -> [u8; 3] {
    let i = ((y * SPRITE_SIZE as u32 + x) * 4) as usize;
    if data[i + 3] == 0 {
        bg
    } else {
        [data[i], data[i + 1], data[i + 2]]
    }
}

// One half-block cell carries two sprite pixel rows: the foreground is the top
// pixel and the background the bottom pixel. Two equal halves render as a
// plain space with that colour.
fn cell_span(top: [u8; 3], bottom: [u8; 3]) -> Span<'static> {
    let (ch, fg, bg) = if top == bottom {
        (' ', top, top)
    } else {
        ('▀', top, bottom)
    };
    Span::styled(
        ch.to_string(),
        Style::default().fg(to_rgb(fg)).bg(to_rgb(bg)),
    )
}

// The box-drawing character for a square perimeter cell. The outline line is
// drawn in `color` over the square background, so it stays high-contrast on
// both base colours and on every piece.
fn outline_span(cx: u16, cy: u16, color: [u8; 3], bg: [u8; 3]) -> Span<'static> {
    let left = cx == 0;
    let right = cx == SQUARE_W - 1;
    let top = cy == 0;
    let bottom = cy == SQUARE_H - 1;
    let ch = if top && left {
        '┌'
    } else if top && right {
        '┐'
    } else if bottom && left {
        '└'
    } else if bottom && right {
        '┘'
    } else if top || bottom {
        '─'
    } else {
        '│'
    };
    Span::styled(
        ch.to_string(),
        Style::default().fg(to_rgb(color)).bg(to_rgb(bg)),
    )
}

// Mid-tone square colors keep both armies readable.
pub(crate) fn base_bg(row: u16, file: u16) -> [u8; 3] {
    if (row + file).is_multiple_of(2) {
        [158, 158, 158]
    } else {
        [110, 110, 110]
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Decor {
    Selected,
    Capture,
    Legal,
    LastMove,
    None,
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn marker_span(color: [u8; 3]) -> Span<'static> {
    Span::styled(" ", Style::default().fg(to_rgb(color)).bg(to_rgb(color)))
}

// Build one square as 8 lines of 16 cells. Decoration precedence is: the
// selected square gets a full gold outline, a capturable destination gets a
// thin amber outline (the piece stays exact inside), an empty legal
// destination gets a small centred green marker, and a last-move square gets a
// subtle blue outline. A cursor without a selection keeps four white corner
// brackets drawn on top.
fn square_lines(
    game: &Game,
    legal: &[Move],
    square: Square,
    row: u16,
    file: u16,
) -> Vec<Line<'static>> {
    let bg = base_bg(row, file);
    let selected = game.selected == Some(square);
    let legal_dest = game.selected.is_some()
        && legal
            .iter()
            .any(|m| m.from() == game.selected && destination(*m) == square);
    // A capture destination is any legal move into this square that captures
    // (including en passant, whose destination square is empty).
    let capture_dest = game.selected.is_some()
        && legal.iter().any(|m| {
            m.from() == game.selected && destination(*m) == square && m.capture().is_some()
        });
    let last_move = game
        .last_move
        .is_some_and(|(a, b)| a == square || b == square);
    let decor = if selected {
        Decor::Selected
    } else if capture_dest {
        Decor::Capture
    } else if legal_dest {
        Decor::Legal
    } else if last_move {
        Decor::LastMove
    } else {
        Decor::None
    };
    let cursor = game.cursor == square && !selected;
    let data = game
        .position
        .board()
        .piece_at(square)
        .map(|piece| sprite_pixels(piece.color, piece.role));
    (0..SQUARE_H)
        .map(|cy| {
            let spans: Vec<Span<'static>> = (0..SQUARE_W)
                .map(|cx| {
                    let on_perimeter =
                        cx == 0 || cx == SQUARE_W - 1 || cy == 0 || cy == SQUARE_H - 1;
                    let on_corner =
                        (cx == 0 || cx == SQUARE_W - 1) && (cy == 0 || cy == SQUARE_H - 1);
                    let in_marker = (MARKER_CX0..=MARKER_CX1).contains(&cx)
                        && (MARKER_CY0..=MARKER_CY1).contains(&cy);
                    let span = if decor == Decor::Selected && on_perimeter {
                        outline_span(cx, cy, SELECTED_OUTLINE, bg)
                    } else if decor == Decor::Capture && on_perimeter {
                        outline_span(cx, cy, CAPTURE_OUTLINE, bg)
                    } else if decor == Decor::Legal && in_marker {
                        marker_span(LEGAL_MARKER)
                    } else if decor == Decor::LastMove && on_perimeter {
                        outline_span(cx, cy, LAST_MOVE_OUTLINE, bg)
                    } else {
                        match data {
                            Some(pixels) => cell_span(
                                pixel_color(pixels, cx as u32, (cy * 2) as u32, bg),
                                pixel_color(pixels, cx as u32, (cy * 2 + 1) as u32, bg),
                            ),
                            None => cell_span(bg, bg),
                        }
                    };
                    if cursor && on_corner {
                        outline_span(cx, cy, CURSOR_OUTLINE, bg)
                    } else {
                        span
                    }
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

fn draw_board(frame: &mut Frame, game: &Game, legal: &[Move]) {
    for row in 0..8u16 {
        frame.render_widget(
            Paragraph::new((8 - row).to_string()),
            Rect::new(0, BOARD_Y + row * SQUARE_H + 3, 1, 1),
        );
        for file in 0..8u16 {
            let square = Square::new(u32::from((7 - row) * 8 + file));
            let lines = square_lines(game, legal, square, row, file);
            frame.render_widget(
                Paragraph::new(lines),
                Rect::new(
                    BOARD_X + file * SQUARE_W,
                    BOARD_Y + row * SQUARE_H,
                    SQUARE_W,
                    SQUARE_H,
                ),
            );
        }
    }
    for file in 0..8u16 {
        let letter = char::from(b'a' + file as u8);
        frame.render_widget(
            Paragraph::new(letter.to_string()),
            Rect::new(
                BOARD_X + file * SQUARE_W + SQUARE_W / 2,
                BOARD_Y + BOARD_CELLS_H,
                1,
                1,
            ),
        );
    }
}

fn draw_panel(frame: &mut Frame, game: &Game) {
    let status = game.ending().unwrap_or_else(|| {
        format!(
            "{} to move{}",
            game.position.turn(),
            if game.position.is_check() {
                " - CHECK!"
            } else {
                ""
            }
        )
    });
    frame.render_widget(
        Paragraph::new(status).wrap(Wrap { trim: true }),
        Rect::new(PANEL_X, 2, PANEL_WIDTH, 2),
    );
    let history: Vec<Line> = game
        .history
        .chunks(2)
        .enumerate()
        .map(|(i, pair)| {
            Line::from(format!(
                "{:>3}. {:<9} {}",
                i + 1,
                pair[0],
                pair.get(1).map_or("", String::as_str)
            ))
        })
        .collect();
    let skip = history.len().saturating_sub(11);
    frame.render_widget(
        Paragraph::new(history.into_iter().skip(skip).collect::<Vec<_>>()).block(
            Block::default()
                .title(" Recent moves ")
                .borders(Borders::ALL),
        ),
        Rect::new(PANEL_X, 5, PANEL_WIDTH, 13),
    );
    if !game.engine_status.is_empty() {
        frame.render_widget(
            Paragraph::new(game.engine_status.as_str()).wrap(Wrap { trim: true }),
            Rect::new(PANEL_X, 19, PANEL_WIDTH, 4),
        );
    }
}

fn draw_new_game_config(frame: &mut Frame, game: &Game) {
    let area = frame.area();
    let width = 72;
    let height = 12;
    let popup = Rect::new(
        area.width.saturating_sub(width) / 2,
        area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("Human side   < "),
            Span::styled(
                game.config_side.label(),
                Style::default().fg(Color::Rgb(0, 220, 255)),
            ),
            Span::raw(" >"),
        ]),
        Line::from(vec![
            Span::raw("Stockfish Elo  "),
            Span::styled(
                game.engine_elo.to_string(),
                Style::default().fg(Color::Rgb(255, 210, 40)),
            ),
        ]),
        Line::from(""),
        Line::from("Left/right: side    Up/down: Elo"),
        Line::from("Enter: start game    Esc: cancel"),
    ];
    frame.render_widget(
        Paragraph::new(text)
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().title(" New game ").borders(Borders::ALL)),
        popup,
    );
}

pub(crate) fn draw(frame: &mut Frame, game: &Game) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        frame.render_widget(
            Paragraph::new(format!(
                "Resize to at least {MIN_WIDTH}x{MIN_HEIGHT} (now {}x{}).\nq / Ctrl-C: quit",
                area.width, area.height
            )),
            area,
        );
        return;
    }
    if game.configuring {
        draw_new_game_config(frame, game);
        return;
    }
    let full = area.width.saturating_sub(2);
    let header = if game.versus_engine && !game.engine_failed {
        let human = capitalize(&game.engine_side.other().to_string());
        format!("CHESS  |  You: {human} vs Stockfish")
    } else {
        "CHESS  |  Local two-player".into()
    };
    frame.render_widget(Paragraph::new(header), Rect::new(1, 0, full, 1));
    let legal = if game.ending().is_none() {
        game.position.legal_moves()
    } else {
        Default::default()
    };
    draw_board(frame, game, &legal);
    draw_panel(frame, game);
    // Footer is at most two lines: one compact controls+legend line and a
    // dynamic line. The dynamic line shows the draw-availability hint only
    // when a draw claim is live and no other feedback is pending.
    frame.render_widget(
        Paragraph::new(
            "Arrows/hjkl cursor  Enter move  Esc cancel  N configure  s sides  d draw  q quit | corners=cursor gold=selected green=legal amber=capture blue=last",
        ),
        Rect::new(1, 66, full, 1),
    );
    let dynamic = if game.ending().is_none() && game.claimable() && game.notice.is_empty() {
        "Draw available: press d"
    } else {
        game.notice.as_str()
    };
    frame.render_widget(Paragraph::new(dynamic), Rect::new(1, 67, full, 1));
}
