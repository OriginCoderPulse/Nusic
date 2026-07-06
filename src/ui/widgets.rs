use std::time::Duration;

use unicode_width::UnicodeWidthStr;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::library::{active_index, segment_fraction};
use crate::player::PlaybackState;
use crate::ui::Theme;

const SPECTRUM_LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// ~4.8s low → high → low, then static gray bars.
const SPECTRUM_IDLE_ANIM_SECS: f32 = 4.8;
const SPECTRUM_IDLE_STATIC: f32 = 1.0;
const SPECTRUM_MAX_LEVEL: usize = 7;
/// Exponential smoothing — higher = snappier, lower = silkier.
const SPECTRUM_SMOOTH_RATE: f32 = 16.0;
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

pub fn app_header(frame: &mut Frame, area: Rect, theme: &Theme) {
    let block = panel_block("Nusic", theme);
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

pub fn song_info(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let block = panel_block("Track Info", theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let track = app
        .filtered_indices
        .get(app.list_selection)
        .and_then(|&i| app.queue.tracks().get(i))
        .or_else(|| app.queue.current_track());

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
                "—".into(),
                truncate_chars(&app.load_path.display().to_string(), MAX_FIELD_CHARS),
                "—".into(),
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
    let count = app.filtered_indices.len();
    let title = format!("Library ({count})");
    let block = panel_block(&title, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let viewport = inner.height as usize;
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

pub fn spectrum_bar(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme, dt: Duration) {
    render_spectrum(frame, area, app, theme, dt);
}

pub fn advance_spectrum(app: &mut App, dt: Duration) {
    let dt = dt.as_secs_f32().min(0.05);

    if app.playback.state == PlaybackState::Playing {
        app.spectrum_idle_secs = SPECTRUM_IDLE_ANIM_SECS + 1.0;
    } else {
        app.spectrum_idle_secs += dt;
    }

    let bar_count = app.spectrum_bars.len();
    if bar_count == 0 {
        return;
    }

    let t = app.spectrum_started.elapsed().as_secs_f32();
    let alpha = 1.0 - (-dt * SPECTRUM_SMOOTH_RATE).exp();
    let playing = app.playback.state == PlaybackState::Playing;

    for (i, bar) in app.spectrum_bars.iter_mut().enumerate() {
        let target = if playing {
            spectrum_play_target(i, t)
        } else {
            spectrum_idle_target(i, app.spectrum_idle_secs)
        };
        *bar += (target - *bar) * alpha;
    }
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

fn render_spectrum(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme, dt: Duration) {
    let block = panel_block("Spectrum", theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let bar_count = (inner.width as usize).div_ceil(2).max(1);
    app.spectrum_bars
        .resize(bar_count, SPECTRUM_IDLE_STATIC);
    advance_spectrum(app, dt);

    let playing = app.playback.state == PlaybackState::Playing;
    let t = app.spectrum_started.elapsed().as_secs_f32();

    let mut spans = Vec::with_capacity(bar_count * 2);
    for i in 0..bar_count {
        let level = app.spectrum_bars[i].round().clamp(0.0, SPECTRUM_MAX_LEVEL as f32) as usize;
        spans.push(Span::styled(
            SPECTRUM_LEVELS[level].to_string(),
            spectrum_bar_style(i, t, playing, theme),
        ));
        if i + 1 < bar_count {
            spans.push(Span::raw(" "));
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        inner,
    );
}

fn spectrum_play_target(i: usize, t: f32) -> f32 {
    let fi = i as f32;
    let n1 = (t * 2.47 + fi * 0.83).sin();
    let n2 = (t * 3.91 + fi * 1.29).sin();
    let n3 = (t * 5.17 + fi * 0.37).sin();
    let gate = (t * 1.13 + fi * 2.03).sin() * 0.5 + 0.5;
    let raw = if gate > 0.58 {
        (n1 * 0.45 + n2 * 0.35 + n3 * 0.55 + 1.15) / 2.4
    } else if gate > 0.32 {
        (n1 * 0.35 + n2 * 0.25 + 0.55) / 1.4
    } else {
        (n1 * 0.15 + 0.35) / 1.2
    };
    raw.clamp(0.0, 1.0) * SPECTRUM_MAX_LEVEL as f32
}

fn spectrum_idle_target(i: usize, idle_secs: f32) -> f32 {
    if idle_secs >= SPECTRUM_IDLE_ANIM_SECS {
        return SPECTRUM_IDLE_STATIC;
    }
    let phase = idle_secs / SPECTRUM_IDLE_ANIM_SECS * std::f32::consts::PI;
    let envelope = phase.sin();
    let ripple = (i as f32 * 0.55 + idle_secs * 2.5).sin() * 0.12;
    (envelope + ripple).clamp(0.0, 1.0) * SPECTRUM_MAX_LEVEL as f32
}

fn spectrum_bar_style(i: usize, t: f32, playing: bool, theme: &Theme) -> Style {
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
    let fi = i as f32;
    let v = (t * 1.7 + fi * 0.93).sin() * 0.45 + (t * 0.9 + fi * 1.41).cos() * 0.55;
    let idx = ((v + 1.0) * 0.5 * palette.len() as f32) as usize;
    palette[idx.min(palette.len() - 1)]
}

pub fn player_controls(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let block = panel_block("Player", theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let title = app
        .playback
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
        .unwrap_or_else(|| "No track playing".to_string());

    let play_icon = match app.playback.state {
        PlaybackState::Playing => "⏸",
        PlaybackState::Paused | PlaybackState::Stopped => "▶",
    };

    let rows = if inner.height >= 3 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(inner)
    };

    let (title_area, progress_area, controls_area) = if inner.height >= 3 {
        (Some(rows[0]), rows[1], rows[2])
    } else {
        (None, rows[0], rows[1])
    };

    if let Some(title_area) = title_area {
        frame.render_widget(
            Paragraph::new(truncate_display(&title, title_area.width as usize))
                .style(theme.title)
                .alignment(Alignment::Center),
            title_area,
        );
    }

    render_progress_line(
        frame,
        progress_area,
        app.playback_position(),
        app.playback.duration,
        theme,
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ⏮ ", theme.muted),
            Span::styled(play_icon, theme.accent),
            Span::styled(" ⏭ ", theme.muted),
        ]))
        .alignment(Alignment::Center),
        controls_area,
    );
}

fn render_progress_line(
    frame: &mut Frame,
    area: Rect,
    position: std::time::Duration,
    duration: std::time::Duration,
    theme: &Theme,
) {
    let bar_w = area.width.max(1) as usize;
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
