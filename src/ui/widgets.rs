use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use unicode_width::UnicodeWidthStr;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::library::{active_index, segment_fraction, MISSING};
use crate::player::{PlaybackState, RepeatMode};
use crate::ui::Theme;

const SPECTRUM_LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const SPECTRUM_IDLE_STATIC: f32 = 1.0;
const SPECTRUM_MAX_LEVEL: usize = 7;
/// Exponential smoothing — higher = snappier, lower = silkier.
const SPECTRUM_SMOOTH_RATE: f32 = 14.0;
const SPECTRUM_ATTACK_RATE: f32 = 18.0;
const SPECTRUM_RELEASE_RATE: f32 = 10.0;
const MAX_FIELD_CHARS: usize = 15;
const SELECT_MARKER: &str = "›";
const PLAY_ICON: &str = "♫";
const ROW_PAD: u16 = 1;

/// Lazygit-style panel: independent rounded border, transparent fill.
pub fn panel_block<'a>(title: &'a str, theme: &Theme) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.border)
        .title(Line::from(Span::styled(format!(" {title} "), theme.subtitle)))
        .padding(Padding::new(ROW_PAD, ROW_PAD, 0, 0))
}

pub fn app_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let panel_title = if app.quit_marked {
        "Nusic · Pin"
    } else {
        "Nusic"
    };
    let block = panel_block(panel_title, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("♫ ", theme.accent),
            Span::styled("Nusic", theme.title),
            Span::styled("  ·  ", theme.muted),
            Span::styled("local music player", theme.subtitle),
        ]))
        .alignment(Alignment::Center),
        inner,
    );
}

fn player_panel_title(app: &App) -> Option<String> {
    if !matches!(
        app.playback.state,
        PlaybackState::Playing | PlaybackState::Paused
    ) {
        return None;
    }

    app.playback
        .current_path
        .as_ref()
        .and_then(|path| {
            app.queue
                .tracks()
                .iter()
                .find(|t| t.path.as_path() == path.as_path())
                .map(|t| t.title.clone())
        })
        .or_else(|| app.queue.current_track().map(|t| t.title.clone()))
}

pub fn song_info(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let block = panel_block("Track Info", theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let track = app
        .playback
        .current_path
        .as_ref()
        .and_then(|path| app.queue.tracks().iter().find(|t| t.path.as_path() == path.as_path()))
        .or_else(|| app.queue.current_track())
        .or_else(|| {
            app.filtered_indices
                .get(app.list_selection)
                .and_then(|&i| app.queue.tracks().get(i))
        });

    let (title, artist, album, duration): (String, String, String, String) =
        if let Some(t) = track {
            (
                t.title.clone(),
                t.artist.clone(),
                t.album.clone(),
                t.duration_display(),
            )
        } else {
            (
                "No tracks".into(),
                MISSING.into(),
                truncate_chars(&app.load_path.display().to_string(), MAX_FIELD_CHARS),
                MISSING.into(),
            )
        };

    let lines = vec![
        info_row("Title", &title, inner.width, theme.muted, theme.title.add_modifier(Modifier::BOLD)),
        info_row("Artist", &artist, inner.width, theme.muted, theme.text),
        info_row("Album", &album, inner.width, theme.muted, theme.subtitle),
        info_row("Length", &duration, inner.width, theme.muted, theme.text),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

pub fn track_list(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let title = library_panel_title(app);
    let block = panel_block(&title, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let viewport = inner.height as usize;
    app.list_viewport = viewport.max(1);
    app.ensure_list_visible(viewport);

    if app.filtered_indices.is_empty() {
        render_centered_lines(
            frame,
            inner,
            vec![
                Line::from(Span::styled("No audio files found", theme.muted)),
                Line::from(Span::styled(
                    "Supported: mp3, flac, ogg, wav, m4a, aac (not DRM .m4p)",
                    theme.muted,
                )),
                Line::from(Span::styled("Press o to open music folder", theme.muted)),
            ],
        );
        return;
    }

    let mut lines = Vec::new();
    let visible = app
        .filtered_indices
        .iter()
        .enumerate()
        .skip(app.list_scroll)
        .take(viewport);

    for (vis_idx, &track_idx) in visible {
        let track = &app.queue.tracks()[track_idx];
        let selected = vis_idx == app.list_selection;
        let playing = app.playback.current_path.as_deref() == Some(track.path.as_path())
            && app.playback.state != PlaybackState::Stopped;

        lines.push(track_row(
            track.title.as_str(),
            inner.width,
            selected,
            playing,
            theme,
        ));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn track_row(
    title: &str,
    width: u16,
    selected: bool,
    playing: bool,
    theme: &Theme,
) -> Line<'static> {
    const MARKER_GAP: usize = 1;
    const PLAY_GAP: usize = 1;

    let w = width as usize;
    if w == 0 {
        return Line::from("");
    }

    let marker = if selected { SELECT_MARKER } else { " " };
    let marker_style = if selected {
        theme.selected
    } else {
        theme.muted
    };
    let row_style = if selected {
        theme.selected
    } else {
        theme.text
    };

    let left_w = marker.width() + MARKER_GAP;
    let play_w = if playing { PLAY_ICON.width() } else { 1 };
    let right_w = PLAY_GAP + play_w;
    let title_budget = w.saturating_sub(left_w + right_w);
    let title = truncate_display(title, title_budget.min(MAX_FIELD_CHARS * 2));

    let used = left_w + title.width() + right_w;
    let pad = w.saturating_sub(used);

    Line::from(vec![
        Span::styled(marker.to_string(), marker_style),
        Span::raw(" ".repeat(MARKER_GAP)),
        Span::styled(title, row_style.add_modifier(Modifier::BOLD)),
        Span::raw(" ".repeat(pad)),
        Span::raw(" ".repeat(PLAY_GAP)),
        Span::styled(
            if playing { PLAY_ICON } else { " " }.to_string(),
            if playing {
                theme.playing
            } else {
                theme.muted
            },
        ),
    ])
}

fn library_panel_title(app: &App) -> String {
    let count = app.filtered_indices.len();
    let mut title = format!("Library ({count})");
    if app.queue.shuffle_enabled() && app.queue.repeat_mode() != RepeatMode::One {
        title.push_str(" · Shuffle");
    }
    match app.queue.repeat_mode() {
        RepeatMode::All => title.push_str(" · Repeat All"),
        RepeatMode::One => title.push_str(" · Repeat One"),
        RepeatMode::Off => {}
    }
    title
}

fn truncate_display(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = ch.to_string().width();
        if cw == 0 {
            continue;
        }
        if w + cw > max_width {
            if max_width >= 1 && (w + 1 <= max_width || out.is_empty()) {
                out.push('…');
            }
            break;
        }
        out.push(ch);
        w += cw;
    }
    out
}

fn info_row(
    label: &str,
    value: &str,
    width: u16,
    label_style: Style,
    value_style: Style,
) -> Line<'static> {
    let w = width as usize;
    if w == 0 {
        return Line::from("");
    }

    let label = format!("{label:<8}");
    let label_w = label.width();
    let value_budget = w.saturating_sub(label_w);
    let value = truncate_display(value, value_budget);
    let value_w = value.width();
    let gap = w.saturating_sub(label_w + value_w);

    Line::from(vec![
        Span::styled(label, label_style),
        Span::raw(" ".repeat(gap)),
        Span::styled(value, value_style),
    ])
}

pub fn lyrics_panel(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if let Some(lines) = &app.lyrics {
        let position = app.playback_position();
        render_lyrics(frame, area, lines, position, theme);
    } else {
        let block = panel_block("Lyrics", theme);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        render_centered_message(frame, inner, "No lyrics", theme.muted);
    }
}

pub fn player_panel(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme, dt: Duration) {
    use crate::ui::layout::SPECTRUM_INNER_ROWS;

    let title_w = area.width.saturating_sub(4) as usize;
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.border)
        .padding(Padding::new(ROW_PAD, ROW_PAD, 0, 0));

    if let Some(title) = player_panel_title(app) {
        block = block
            .title(Line::from(Span::styled(
                truncate_display(&title, title_w),
                theme.title,
            )))
            .title_alignment(Alignment::Center);
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(SPECTRUM_INNER_ROWS),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    render_spectrum_bars(frame, rows[1], app, theme, dt);

    let play_icon = match app.playback.state {
        PlaybackState::Playing => "⏸",
        PlaybackState::Paused | PlaybackState::Stopped => "▶",
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("⏮", theme.muted),
            Span::raw("  "),
            Span::styled(play_icon, theme.accent),
            Span::raw("  "),
            Span::styled("⏭", theme.muted),
        ]))
        .alignment(Alignment::Center),
        rows[3],
    );

    render_progress_line(
        frame,
        rows[5],
        app.playback_position(),
        app.playback.duration,
        theme,
    );
}

pub fn advance_spectrum(app: &mut App, dt: Duration) {
    let dt = dt.as_secs_f32().min(0.05);
    let bar_count = app.spectrum_bars.len();
    if bar_count == 0 {
        return;
    }

    if app.spectrum_targets.len() != bar_count {
        app.spectrum_targets.resize(bar_count, 0.0);
        reseed_spectrum_targets(app);
    }

    let playing = app.playback.state == PlaybackState::Playing;
    app.spectrum_reseed_acc += dt;
    let interval = if playing {
        0.14 + spectrum_hash(app.spectrum_seed, 0, 1) * 0.22
    } else {
        0.55 + spectrum_hash(app.spectrum_seed, 0, 2) * 0.85
    };
    if app.spectrum_reseed_acc >= interval {
        app.spectrum_reseed_acc = 0.0;
        reseed_spectrum_targets(app);
    }

    for (i, bar) in app.spectrum_bars.iter_mut().enumerate() {
        let target = app.spectrum_targets.get(i).copied().unwrap_or(0.0);
        let rate = if playing && target > *bar {
            SPECTRUM_ATTACK_RATE
        } else if playing {
            SPECTRUM_RELEASE_RATE
        } else {
            SPECTRUM_SMOOTH_RATE * 0.6
        };
        let alpha = 1.0 - (-dt * rate).exp();
        *bar += (target - *bar) * alpha;
    }
}

fn reseed_spectrum_targets(app: &mut App) {
    app.spectrum_seed = app.spectrum_seed.wrapping_add(1);
    let playing = app.playback.state == PlaybackState::Playing;
    for (i, target) in app.spectrum_targets.iter_mut().enumerate() {
        *target = spectrum_random_level(i, app.spectrum_seed, playing);
    }
}

fn spectrum_hash(seed: u64, bar: usize, salt: u32) -> f32 {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    bar.hash(&mut hasher);
    salt.hash(&mut hasher);
    hasher.finish() as f32 / u64::MAX as f32
}

fn spectrum_random_level(bar: usize, seed: u64, playing: bool) -> f32 {
    let raw = spectrum_hash(seed, bar, 0);
    let contrast = if raw < 0.38 {
        raw * 0.22
    } else {
        0.08 + (raw - 0.38) * 1.45
    };
    let scale = if playing { 1.0 } else { 0.35 };
    contrast.clamp(0.0, 1.0) * SPECTRUM_MAX_LEVEL as f32 * scale
}

fn pad_center(text: &str, width: u16, style: Style) -> Line<'static> {
    let w = width as usize;
    if w == 0 {
        return Line::from("");
    }
    let text = truncate_display(text, w);
    let tw = text.width();
    if tw >= w {
        return Line::from(Span::styled(text, style));
    }
    let pad = w - tw;
    let left = pad / 2;
    Line::from(vec![
        Span::raw(" ".repeat(left)),
        Span::styled(text, style),
        Span::raw(" ".repeat(pad - left)),
    ])
}

fn render_lyrics(
    frame: &mut Frame,
    area: Rect,
    lines: &[crate::library::LrcLine],
    position: std::time::Duration,
    theme: &Theme,
) {
    let block = panel_block("Lyrics", theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let viewport = inner.height as usize;
    if viewport == 0 || inner.width == 0 {
        return;
    }

    let active = active_index(lines, position).unwrap_or(0);
    let frac = segment_fraction(lines, position, active);
    let center_row = viewport as f32 / 2.0;
    // Float anchor: current line starts at center, drifts up as frac→1.
    let base = active as f32 - center_row + frac;

    let mut rendered = Vec::with_capacity(viewport);
    let mut prev_idx: Option<usize> = None;

    for row in 0..viewport {
        let line_f = base + row as f32;
        if line_f < 0.0 {
            rendered.push(Line::from(""));
            continue;
        }
        let i = line_f.floor() as usize;
        if i >= lines.len() {
            rendered.push(Line::from(""));
            continue;
        }
        if prev_idx == Some(i) {
            rendered.push(Line::from(""));
            continue;
        }
        prev_idx = Some(i);

        let style = if i == active {
            theme.playing.add_modifier(Modifier::BOLD)
        } else if i == active + 1 {
            theme.subtitle
        } else {
            theme.muted
        };
        rendered.push(pad_center(&lines[i].text, inner.width, style));
    }

    frame.render_widget(
        Paragraph::new(if rendered.iter().all(|l| l.spans.is_empty()) {
            vec![Line::from(Span::styled("…", theme.muted))]
        } else {
            rendered
        }),
        inner,
    );
}

fn render_centered_lines(frame: &mut Frame, area: Rect, lines: Vec<Line>) {
    if area.height == 0 || lines.is_empty() {
        return;
    }
    let line_count = lines.len() as u16;
    let top_pad = area.height.saturating_sub(line_count) / 2;
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(top_pad),
            Constraint::Length(line_count),
            Constraint::Min(0),
        ])
        .split(area)[1];
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        center,
    );
}

fn render_centered_message(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    render_centered_lines(
        frame,
        area,
        vec![Line::from(Span::styled(text, style))],
    );
}

fn render_spectrum_bars(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme, dt: Duration) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let bar_count = (area.width as usize).div_ceil(2).max(1);
    app.spectrum_bars
        .resize(bar_count, SPECTRUM_IDLE_STATIC);
    advance_spectrum(app, dt);

    let playing = app.playback.state == PlaybackState::Playing;
    let rows = area.height.max(1);

    let (display_min, display_max) = if playing && !app.spectrum_bars.is_empty() {
        let min = app
            .spectrum_bars
            .iter()
            .copied()
            .fold(f32::INFINITY, f32::min);
        let max = app
            .spectrum_bars
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        (min, max.max(min + 1.2))
    } else {
        (0.0, SPECTRUM_MAX_LEVEL as f32)
    };
    let display_range = (display_max - display_min).max(0.8);

    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(1); rows as usize])
        .split(area);

    for (row, row_area) in row_areas.iter().enumerate() {
        let row_from_bottom = rows as usize - 1 - row;
        let row_bottom = row_from_bottom as f32 / rows as f32;
        let row_top = (row_from_bottom + 1) as f32 / rows as f32;
        let mut spans = Vec::with_capacity(bar_count * 2);
        for i in 0..bar_count {
            let level = app.spectrum_bars[i].clamp(0.0, SPECTRUM_MAX_LEVEL as f32);
            let normalized = if playing {
                ((level - display_min) / display_range).clamp(0.0, 1.0).powf(0.9)
            } else {
                level / SPECTRUM_MAX_LEVEL as f32
            };
            let ch = if normalized >= row_top {
                SPECTRUM_LEVELS[SPECTRUM_MAX_LEVEL]
            } else if normalized > row_bottom {
                let frac = (normalized - row_bottom) / (row_top - row_bottom);
                let idx = (frac * SPECTRUM_MAX_LEVEL as f32)
                    .round()
                    .clamp(1.0, SPECTRUM_MAX_LEVEL as f32) as usize;
                SPECTRUM_LEVELS[idx]
            } else if row_from_bottom == 0 && normalized > 0.0 {
                SPECTRUM_LEVELS[0]
            } else {
                ' '
            };
            spans.push(Span::styled(
                ch.to_string(),
                spectrum_bar_style(i, normalized, playing, theme),
            ));
            if i + 1 < bar_count {
                spans.push(Span::raw(" "));
            }
        }

        frame.render_widget(
            Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
            *row_area,
        );
    }
}

fn spectrum_bar_style(i: usize, normalized: f32, playing: bool, theme: &Theme) -> Style {
    if !playing {
        return theme.muted;
    }

    let palette = [
        theme.muted,
        theme.subtitle,
        theme.accent,
        theme.playing,
        theme.selected,
        theme.progress_fill,
        theme.title,
    ];
    let jitter = spectrum_hash(i as u64, (normalized * 1000.0) as usize, 3);
    let idx = ((normalized * 0.65 + jitter * 0.35) * palette.len() as f32) as usize;
    palette[idx.min(palette.len() - 1)]
}

fn render_progress_line(
    frame: &mut Frame,
    area: Rect,
    position: std::time::Duration,
    duration: std::time::Duration,
    theme: &Theme,
) {
    let max_w = area.width.max(1) as usize;
    let bar_w = (max_w * 65 / 100).clamp(28, 48).min(max_w);
    let ratio = if duration.is_zero() {
        0.0
    } else {
        (position.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0)
    };
    let exact = ratio * bar_w as f64;

    let mut spans = Vec::with_capacity(bar_w);
    for i in 0..bar_w {
        let start = i as f64;
        let end = (i + 1) as f64;
        let (ch, style) = if exact >= end {
            ('━', theme.progress_fill)
        } else if exact <= start {
            ('─', theme.progress_empty)
        } else {
            let frac = exact - start;
            let ch = if frac >= 0.75 {
                '━'
            } else if frac >= 0.5 {
                '▬'
            } else if frac >= 0.25 {
                '╌'
            } else {
                '─'
            };
            (ch, theme.progress_fill)
        };
        spans.push(Span::styled(ch.to_string(), style));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        area,
    );
}

pub fn status_message(frame: &mut Frame, area: Rect, message: &str, theme: &Theme) {
    let popup = Rect {
        x: area.x + 2,
        y: area.y + area.height.saturating_sub(4),
        width: area.width.saturating_sub(4),
        height: 3,
    };
    frame.render_widget(
        Paragraph::new(message)
            .style(theme.error)
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(theme.error)
                    .title(Line::from(Span::styled(" Error ", theme.error))),
            ),
        popup,
    );
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else if max == 0 {
        String::new()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}
