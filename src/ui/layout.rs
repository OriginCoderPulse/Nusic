use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub const SPECTRUM_INNER_ROWS: u16 = 3;
const PANEL_ROW_GAP: u16 = 1;
const PLAYER_TOP_PAD: u16 = 1;
const PLAYER_CONTROLS_ROWS: u16 = 1;
const PLAYER_PROGRESS_ROWS: u16 = 1;

pub fn split_main(area: Rect) -> (Rect, Rect) {
    let bottom = bottom_panel_height();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(bottom)])
        .split(area);

    (chunks[0], chunks[1])
}

pub fn bottom_panel_height() -> u16 {
    2 + PLAYER_TOP_PAD
        + SPECTRUM_INNER_ROWS
        + PANEL_ROW_GAP
        + PLAYER_CONTROLS_ROWS
        + PANEL_ROW_GAP
        + PLAYER_PROGRESS_ROWS
}

pub fn split_top(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(3, 10), Constraint::Ratio(7, 10)])
        .split(area);
    (chunks[0], chunks[1])
}

pub fn split_left(area: Rect) -> (Rect, Rect, Rect) {
    let header_h = 3u16;
    let info_h = info_height(area.height, header_h);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_h),
            Constraint::Length(info_h),
            Constraint::Min(5),
        ])
        .split(area);

    (chunks[0], chunks[1], chunks[2])
}

fn info_height(left_h: u16, header_h: u16) -> u16 {
    let body = left_h.saturating_sub(header_h);
    if body <= 9 {
        return 5;
    }
    (((body as f32) * 0.22).round() as u16).clamp(5, 7)
}
