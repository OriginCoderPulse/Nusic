use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub const CONTROLS_MIN: u16 = 5;
pub const SPECTRUM_HEIGHT: u16 = 3;

pub fn split_main(area: Rect) -> (Rect, Rect, Rect) {
    let controls = controls_height(area.height);
    let bottom = controls.saturating_add(SPECTRUM_HEIGHT);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(bottom)])
        .split(area);

    let (spectrum, controls) = split_bottom(chunks[1]);
    (chunks[0], spectrum, controls)
}

fn split_bottom(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(SPECTRUM_HEIGHT),
            Constraint::Length(area.height.saturating_sub(SPECTRUM_HEIGHT).max(CONTROLS_MIN)),
        ])
        .split(area);
    (chunks[0], chunks[1])
}

fn controls_height(total: u16) -> u16 {
    if total <= CONTROLS_MIN + SPECTRUM_HEIGHT + 4 {
        return CONTROLS_MIN.max(3);
    }
    (((total as f32) * 0.11).round() as u16).clamp(5, 7)
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
