mod layout;
mod theme;
mod widgets;

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

use layout::{split_left, split_main, split_top};
use theme::Theme;
use widgets::{
    app_header, lyrics_panel, player_controls, song_info, spectrum_bar, status_message, track_list,
};

pub fn run(mut app: App) -> anyhow::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(
        io::stdout(),
    ))?;

    let tick = Duration::from_millis(120);
    let frame = Duration::from_millis(16);
    let mut last_tick = Instant::now();
    let mut last_frame = Instant::now();

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_frame);
        last_frame = now;

        terminal.draw(|f| draw(f, &mut app, dt))?;

        if last_tick.elapsed() >= tick {
            app.on_tick();
            last_tick = Instant::now();
        }

        let timeout = frame.saturating_sub(last_frame.elapsed().min(frame));
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if handle_key(&mut app, key) {
                    break;
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;

    app.shutdown();

    Ok(())
}

fn draw(frame: &mut Frame, app: &mut App, dt: Duration) {
    let theme = Theme::default();
    let area = frame.area();

    frame.render_widget(Clear, area);

    let (top, spectrum, controls) = split_main(area);
    let (left, right) = split_top(top);
    let (header, info, list) = split_left(left);

    app_header(frame, header, &theme);
    song_info(frame, info, app, &theme);
    track_list(frame, list, app, &theme);
    lyrics_panel(frame, right, app, &theme);
    spectrum_bar(frame, spectrum, app, &theme, dt);
    player_controls(frame, controls, app, &theme);

    if let Some(msg) = app.status_message.clone() {
        status_message(frame, area, &msg, &theme);
    }

    if app.search_mode {
        draw_search(frame, area, app, &theme);
    }
}

fn draw_search(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let popup = centered_rect(60, 3, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(theme.accent)
            .title(Line::from(Span::styled(" Search ", theme.subtitle))),
        popup,
    );
    let inner = popup.inner(Margin::new(2, 1));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("/", theme.muted),
            Span::raw(&app.search_query),
            Span::styled("▌", theme.accent),
        ])),
        inner,
    );
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height),
            Constraint::Fill(1),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    if app.search_mode {
        return handle_search_key(app, key);
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
            return true;
        }
        KeyCode::Char(' ') => app.toggle_playback(),
        KeyCode::Char('n') | KeyCode::Char(']') => {
            app.next_track();
        }
        KeyCode::Char('p') | KeyCode::Char('[') => app.prev_track(),
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return true;
        }
        KeyCode::Char('s') => app.toggle_shuffle(),
        KeyCode::Char('r') => app.cycle_repeat(),
        KeyCode::Char('o') => app.open_music_dir(),
        KeyCode::Char('/') => {
            app.search_mode = true;
            app.search_query.clear();
        }
        KeyCode::Char('+') | KeyCode::Char('=') => app.adjust_volume(0.05),
        KeyCode::Char('-') | KeyCode::Char('_') => app.adjust_volume(-0.05),
        KeyCode::Char('j') | KeyCode::Down => app.move_selection(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_selection(-1),
        KeyCode::Char('h') | KeyCode::Left => app.move_selection(-1),
        KeyCode::Char('l') | KeyCode::Right => app.move_selection(1),
        KeyCode::PageUp => app.move_selection(-10),
        KeyCode::PageDown => app.move_selection(10),
        KeyCode::Home => app.select_first(),
        KeyCode::End => app.select_last(),
        KeyCode::Enter => app.play_selected(),
        _ => {}
    }

    false
}

fn handle_search_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.search_mode = false;
            app.search_query.clear();
            app.apply_filter();
        }
        KeyCode::Enter => {
            app.search_mode = false;
            app.apply_filter();
            if !app.filtered_indices.is_empty() {
                app.play_selected();
            }
        }
        KeyCode::Backspace => {
            app.search_query.pop();
            app.apply_filter();
        }
        KeyCode::Char(c) => {
            app.search_query.push(c);
            app.apply_filter();
        }
        _ => {}
    }
    false
}
