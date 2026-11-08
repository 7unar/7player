//! Terminal UI rendering.

pub mod theme;

use crate::app::{App, Focus, HomeTab, LoginTab, Screen, Splash, ToastKind};
use crate::keymap::Action;
use crate::netease::model::{Playlist, Track};
use crate::ui::theme::{theme_at, Theme};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, LineGauge, List, ListItem, ListState, Padding, Paragraph,
    Row, Table, TableState, Wrap,
};
use ratatui::Frame;
use std::cell::RefCell;
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitAction {
    Nav(Screen),
    HomeTab(HomeTab),
    SelectRow(usize),
    FocusSearch,
    PlayPause,
    Seek,
    Mute,
    Like,
    OpenLyrics,
    ThemeRow(usize),
    LoginTab(LoginTab),
    LoginField(usize),
    QrRefresh,
    LoginSubmit,
    ToggleSidebar,
    CloseOverlay,
    Noop,
    PlayerArea,
    LyricsArea,
}

#[derive(Debug, Clone, Copy)]
pub struct HitRegion {
    pub rect: Rect,
    pub action: HitAction,
}

thread_local! {
    static HITS: RefCell<Vec<HitRegion>> = const { RefCell::new(Vec::new()) };
}

fn hit(rect: Rect, action: HitAction) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    HITS.with(|hits| hits.borrow_mut().push(HitRegion { rect, action }));
}

pub fn take_hits() -> Vec<HitRegion> {
    HITS.with(|hits| std::mem::take(&mut *hits.borrow_mut()))
}

fn clear_hits() {
    HITS.with(|hits| hits.borrow_mut().clear());
}

pub fn draw(frame: &mut Frame, app: &App) {
    clear_hits();
    let area = frame.area();
    if let Some(splash) = &app.splash {
        draw_splash(frame, area, splash);
        return;
    }
    let theme = app.theme;
    frame.render_widget(Block::default().style(theme.base()), area);

    if app.screen == Screen::Lyrics {
        render_lyrics_full(frame, area, app);
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(8),
        Constraint::Length(4),
        Constraint::Length(1),
    ])
    .split(area);
    let header = layout[0];
    let body = layout[1];
    let player = layout[2];
    let footer = layout[3];

    render_header(frame, header, app);

    let sidebar_width = if app.sidebar_visible { 24 } else { 0 };
    let show_side_lyrics = area.width >= 112 && app.player.track.is_some();
    let constraints = if show_side_lyrics {
        vec![
            Constraint::Length(sidebar_width),
            Constraint::Min(38),
            Constraint::Length(36),
        ]
    } else {
        vec![
            Constraint::Length(sidebar_width),
            Constraint::Min(38),
            Constraint::Length(0),
        ]
    };
    let cols = Layout::horizontal(constraints).split(body);
    if app.sidebar_visible {
        render_sidebar(frame, cols[0], app);
    }
    render_main(frame, cols[1], app);
    if show_side_lyrics {
        render_lyrics_panel(frame, cols[2], app);
    }

    render_player(frame, player, app);
    render_footer(frame, footer, app);

    if app.help {
        render_help(frame, area, app);
    } else if app.theme_picker.is_some() {
        render_theme_picker(frame, area, app);
    } else if app.login.is_some() {
        render_login(frame, area, app);
    }
}

// ----------------------------------------------------------------------
// Splash / intro
// ----------------------------------------------------------------------

const SIGNATURE_LINES: &[&str] = &[
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⣀⣀⣀⣀⣀⣀⣀⣤⡄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣿⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠉⠉⠉⠉⠉⠉⣽⠟⠁⠀⠀⠀⠀⠀⣀⣠⡀⠀⠀⠀⣸⡟⠀⠀⠀⠀⠀⠀⢀⣤⣤⣄⠀⠀⠀⠀⠀⡀⠀⠀⠀⠀⣀⠀⠀⠀⢀⣴⣶⡄⠀⠀⠀⠀⣀⠀⠀⢀⣀⡀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⢠⣾⠋⠀⠀⠀⢸⡷⢠⡾⠋⠙⣿⠀⢀⣴⣿⡇⠀⠀⠀⠀⠀⣴⠟⠁⢸⡟⠀⠀⠀⣠⣾⠇⠀⠀⠀⢸⣿⠀⠀⢰⡟⢁⣼⠃⠀⠀⠀⣸⡿⢀⣴⠟⠙⠛⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⢠⡿⠁⠀⠀⠀⠀⢸⣷⠏⠀⠀⣼⣇⣴⠟⠁⣿⠀⠀⠀⠀⣠⣾⠋⠀⣠⠿⣧⢀⣠⡾⢻⣿⠀⠀⠀⣠⣾⣿⠀⠀⣿⡷⠛⠁⠀⠀⣠⠞⢹⣧⡾⠁⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⢠⣿⠁⠀⠀⠀⠀⠀⣼⡏⢀⣠⣾⠟⠋⠀⠀⠀⠻⣧⣤⣤⠾⠛⣿⣤⠾⠋⠀⠙⠛⠉⠀⠘⢿⣦⡶⠞⠋⢸⡏⠀⠀⢻⣧⣤⣤⡴⠞⠁⠀⢸⣿⠁⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠈⠋⠀⠀⠀⠀⠀⠀⣿⡇⠈⠉⠀⠀⠀⠀⠀⠀⠀⠈⠉⠀⠀⠀⠀⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣾⠇⠀⠀⠀⠈⠉⠁⠀⠀⠀⠀⠈⠉⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠰⣷⣤⣀⣤⡾⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠉⠉⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⠄⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⠔⠁⠀⠀⠀",
    "⠀⠀⠀⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠞⠛⠛⠃⠀⠀⠀⠀⠀",
];

const SIGNATURE_WIDTH_DOTS: usize = 132;
const SIGNATURE_HEIGHT_DOTS: usize = 44;
const SIGNATURE_WIDTH_CELLS: usize = SIGNATURE_WIDTH_DOTS / 2;
const SIGNATURE_HEIGHT_CELLS: usize = SIGNATURE_HEIGHT_DOTS / 4;
const UNDERLINE_Y: usize = 40;

const DOT_BITS: [(usize, usize, u16); 8] = [
    (0, 0, 0x01),
    (0, 1, 0x02),
    (0, 2, 0x04),
    (1, 0, 0x08),
    (1, 1, 0x10),
    (1, 2, 0x20),
    (0, 3, 0x40),
    (1, 3, 0x80),
];

fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    (
        (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t) as u8,
        (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t) as u8,
        (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t) as u8,
    )
}

fn rgb(color: (u8, u8, u8)) -> Color {
    Color::Rgb(color.0, color.1, color.2)
}

fn signature_dots() -> &'static [Vec<bool>] {
    static DOTS: std::sync::OnceLock<Vec<Vec<bool>>> = std::sync::OnceLock::new();
    DOTS.get_or_init(|| {
        let mut grid = vec![vec![false; SIGNATURE_WIDTH_DOTS]; SIGNATURE_HEIGHT_DOTS];
        for (row, line) in SIGNATURE_LINES.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                let bits = (ch as u32).saturating_sub(0x2800) as u16;
                for (dx, dy, bit) in DOT_BITS {
                    if bits & bit == 0 {
                        continue;
                    }
                    let x = col * 2 + dx;
                    let y = row * 4 + dy;
                    if x < SIGNATURE_WIDTH_DOTS && y < SIGNATURE_HEIGHT_DOTS {
                        grid[y][x] = true;
                    }
                }
            }
        }
        grid
    })
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn ink_color(cell_x: usize, pen: f32, shimmer: f32, fade: f32) -> Color {
    let t = cell_x as f32 / (SIGNATURE_WIDTH_CELLS - 1) as f32;
    let mut color = if t < 0.55 {
        lerp_rgb((96, 124, 255), (156, 134, 255), t / 0.55)
    } else {
        lerp_rgb((156, 134, 255), (205, 160, 255), (t - 0.55) / 0.45)
    };
    if (cell_x as f32 - shimmer).abs() < 4.5 {
        let strength = 1.0 - (cell_x as f32 - shimmer).abs() / 4.5;
        color = lerp_rgb(color, (224, 234, 255), strength * 0.45);
    }
    if (cell_x as f32 - pen).abs() < 2.5 {
        color = lerp_rgb(color, (235, 244, 255), 0.9);
    }
    color = lerp_rgb(color, (8, 8, 14), fade);
    rgb(color)
}

fn draw_splash(frame: &mut Frame, area: Rect, splash: &Splash) {
    let elapsed = splash.elapsed().as_secs_f32();
    let duration = splash.duration.as_secs_f32().max(0.1);
    let bg = (8, 8, 14);
    frame
        .buffer_mut()
        .set_style(area, Style::default().bg(rgb(bg)));

    if area.width < 72 || area.height < 20 {
        let lines = vec![
            Line::from(Span::styled(
                "7player",
                Style::default()
                    .fg(rgb((170, 156, 255)))
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
            Line::from(Span::styled(
                "NETEASE CLOUD MUSIC",
                Style::default().fg(rgb((96, 96, 132))),
            ))
            .alignment(Alignment::Center),
            Line::from(""),
            Line::from(Span::styled(
                "按任意键进入",
                Style::default().fg(rgb((130, 120, 190))),
            ))
            .alignment(Alignment::Center),
        ];
        let inner = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(4),
            Constraint::Fill(1),
        ])
        .split(area)[1];
        frame.render_widget(Paragraph::new(lines).style(Style::default()), inner);
        return;
    }

    let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(SIGNATURE_HEIGHT_CELLS as u16),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(area);

    let text_reveal = ease_out_cubic(elapsed / 1.15) * SIGNATURE_WIDTH_DOTS as f32;
    let line_reveal = ((elapsed - 1.15) / 0.55).clamp(0.0, 1.0) * SIGNATURE_WIDTH_DOTS as f32;
    let fade = ((elapsed - (duration - 0.32)) / 0.32).clamp(0.0, 1.0);
    let shimmer = if elapsed > 1.45 {
        ((elapsed - 1.45) * 34.0) % 100.0 - 15.0
    } else {
        -100.0
    };
    let pen = if text_reveal < SIGNATURE_WIDTH_DOTS as f32 {
        text_reveal / 2.0
    } else if elapsed > 1.15 && line_reveal < SIGNATURE_WIDTH_DOTS as f32 {
        line_reveal / 2.0
    } else {
        -10.0
    };

    let dots = signature_dots();
    let lines: Vec<Line> = (0..SIGNATURE_HEIGHT_CELLS)
        .map(|cell_y| {
            let mut spans = Vec::with_capacity(SIGNATURE_WIDTH_CELLS);
            for cell_x in 0..SIGNATURE_WIDTH_CELLS {
                let mut bits = 0u16;
                for (dx, dy, bit) in DOT_BITS {
                    let x = cell_x * 2 + dx;
                    let y = cell_y * 4 + dy;
                    if !dots[y][x] {
                        continue;
                    }
                    let visible = if y >= UNDERLINE_Y {
                        x as f32 <= line_reveal
                    } else {
                        x as f32 <= text_reveal
                    };
                    if visible {
                        bits |= bit;
                    }
                }
                if bits == 0 {
                    spans.push(Span::raw(" "));
                    continue;
                }
                let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default()
                        .fg(ink_color(cell_x, pen, shimmer, fade))
                        .add_modifier(Modifier::BOLD),
                ));
            }
            Line::from(spans).alignment(Alignment::Center)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).style(Style::default()), layout[1]);

    let fade_in = ((elapsed - 1.05) / 0.55).clamp(0.0, 1.0);
    let subtitle_color = lerp_rgb(bg, (126, 124, 168), fade_in);
    frame.render_widget(
        Paragraph::new(
            Line::from(Span::styled(
                "N E T E A S E   C L O U D   M U S I C",
                Style::default()
                    .fg(rgb(subtitle_color))
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
        )
        .style(Style::default()),
        layout[3],
    );
    frame.render_widget(
        Paragraph::new(
            Line::from(Span::styled(
                "网易云音乐 · 终端播放器",
                Style::default().fg(rgb(lerp_rgb(bg, (88, 84, 122), fade_in))),
            ))
            .alignment(Alignment::Center),
        )
        .style(Style::default()),
        layout[4],
    );

    let hint = if elapsed > 1.55 {
        let pulse = 0.55 + 0.45 * ((elapsed * 3.0).sin() * 0.5 + 0.5);
        let color = lerp_rgb((78, 74, 108), (178, 166, 255), pulse);
        Line::from(Span::styled(
            "按任意键进入   ·   PRESS ANY KEY",
            Style::default().fg(rgb(color)).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center)
    } else {
        Line::from(Span::styled(
            "·   ·   ·",
            Style::default().fg(rgb((64, 60, 88))),
        ))
        .alignment(Alignment::Center)
    };
    frame.render_widget(Paragraph::new(hint).style(Style::default()), layout[6]);

    if area.height > 4 {
        let version_area = Rect::new(
            area.x.saturating_add(area.width.saturating_sub(18)),
            area.y.saturating_add(area.height.saturating_sub(1)),
            17,
            1,
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("7player  {}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(rgb((54, 52, 74))),
            )))
            .alignment(Alignment::Right)
            .style(Style::default().bg(rgb(bg))),
            version_area,
        );
    }
}
// ----------------------------------------------------------------------
// Chrome
// ----------------------------------------------------------------------

fn spinner(app: &App) -> &'static str {
    SPINNER[(app.tick as usize / 12) % SPINNER.len()]
}

fn busy(app: &App) -> bool {
    app.bootstrapping
        || app.home.loading
        || app.search.loading
        || app.player.loading
        || app
            .detail
            .as_ref()
            .map(|detail| detail.loading)
            .unwrap_or(false)
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);
    let top = rows[0];
    let title = match app.screen {
        Screen::Detail => app
            .detail
            .as_ref()
            .map(|detail| detail.playlist.name.clone())
            .unwrap_or_else(|| "歌单详情".into()),
        Screen::Lyrics => "歌词".into(),
        other => other.title().to_string(),
    };
    let left = Line::from(vec![
        Span::styled(
            " ▤ ",
            Style::default()
                .fg(if app.sidebar_visible {
                    theme.muted
                } else {
                    theme.accent
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "♪ ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "7",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("player", Style::default().fg(theme.accent_alt)),
        Span::styled("  ·  ", theme.muted()),
        Span::styled(title, Style::default().fg(theme.text)),
    ]);
    frame.render_widget(Paragraph::new(left).style(theme.base()), top);
    hit(
        Rect::new(top.x.saturating_add(1), top.y, 3, 1),
        HitAction::ToggleSidebar,
    );

    let mut right = Vec::new();
    if busy(app) {
        right.push(Span::styled(
            format!("{} ", spinner(app)),
            Style::default().fg(theme.accent),
        ));
        right.push(Span::styled("同步中", theme.muted()));
        right.push(Span::styled("   ", theme.muted()));
    }
    if let Some(user) = &app.user {
        right.push(Span::styled(
            format!("● {}", truncate(&user.nickname, 18)),
            Style::default().fg(theme.green),
        ));
    } else {
        right.push(Span::styled("○ 未登录", theme.muted()));
    }
    let backend = if app.snapshot.backend.contains("静音") {
        "静音"
    } else {
        "音频"
    };
    right.push(Span::styled(
        format!("  ·  {backend}  ·  {}", app.theme.label),
        theme.muted(),
    ));
    frame.render_widget(
        Paragraph::new(Line::from(right))
            .alignment(Alignment::Right)
            .style(theme.base()),
        top,
    );

    let rule = "─".repeat(area.width as usize);
    frame.render_widget(
        Paragraph::new(rule).style(Style::default().fg(theme.border).bg(theme.bg)),
        rows[1],
    );
}

fn render_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.surface).fg(theme.text));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(9),
        Constraint::Min(0),
        Constraint::Length(8),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " 浏览",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        )))
        .style(Style::default().bg(theme.surface)),
        chunks[0],
    );

    let active = app.active_nav();
    let items: Vec<ListItem> = Screen::NAV
        .iter()
        .map(|screen| {
            let selected = *screen == active;
            let title_style = if selected {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };
            let desc_style = if selected {
                Style::default().fg(theme.text)
            } else {
                Style::default().fg(theme.muted)
            };
            let desc = match screen {
                Screen::Home => "推荐 · 榜单",
                Screen::Search => "搜索网易云",
                Screen::Library => "我的歌单",
                Screen::Queue => "播放队列",
                _ => "",
            };
            let padding = Span::styled(
                " ".repeat(26),
                if selected {
                    theme.selected()
                } else {
                    Style::default().bg(theme.surface)
                },
            );
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(format!("  {}  ", screen.icon()), title_style),
                    Span::styled(screen.title(), title_style),
                    padding.clone(),
                ]),
                Line::from(vec![
                    Span::styled(format!("      {desc}"), desc_style),
                    padding,
                ]),
            ])
        })
        .collect();
    let list = List::new(items)
        .highlight_style(theme.selected())
        .highlight_symbol("")
        .style(Style::default().bg(theme.surface));
    let index = Screen::NAV
        .iter()
        .position(|screen| *screen == active)
        .unwrap_or(0);
    let mut state = ListState::default().with_selected(Some(index));
    frame.render_stateful_widget(list, chunks[1], &mut state);
    for (i, screen) in Screen::NAV.iter().enumerate() {
        hit(
            Rect::new(
                chunks[1].x,
                chunks[1].y.saturating_add(i as u16 * 2),
                chunks[1].width,
                2,
            ),
            HitAction::Nav(*screen),
        );
    }

    let mut footer = vec![
        Line::from(Span::styled("  ───────────────", theme.muted())),
        Line::from(vec![
            Span::styled(
                "  ◉ ",
                Style::default().fg(if app.user.is_some() {
                    theme.green
                } else {
                    theme.muted
                }),
            ),
            Span::styled(
                app.user
                    .as_ref()
                    .map(|user| truncate(&user.nickname, 12))
                    .unwrap_or_else(|| "未登录".into()),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("  主题  ", theme.muted()),
            Span::styled(
                truncate(app.theme.label, 13),
                Style::default().fg(theme.accent_alt),
            ),
        ]),
        Line::from(vec![
            Span::styled("  音质  ", theme.muted()),
            Span::styled(
                crate::config::quality_label(&app.cfg.quality),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("  音量  ", theme.muted()),
            Span::styled(
                format!("{}%", app.cfg.volume),
                Style::default().fg(if app.cfg.volume == 0 {
                    theme.red
                } else {
                    theme.green
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("  输出  ", theme.muted()),
            Span::styled(
                if app.snapshot.backend.contains("静音") {
                    "静音模拟"
                } else {
                    "系统音频"
                },
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(Span::styled("  L 登录  ·  ? 帮助", theme.muted())),
        Line::from(Span::styled(format!("  v{VERSION}"), theme.muted())),
    ];
    footer.truncate(8);
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().bg(theme.surface)),
        chunks[3],
    );
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let hint = match (&app.toast, app.screen, app.focus) {
        (Some(toast), _, _) => {
            let (icon, color) = match toast.kind {
                ToastKind::Info => ("◈", theme.accent),
                ToastKind::Success => ("✓", theme.green),
                ToastKind::Error => ("!", theme.red),
            };
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{icon} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(truncate(&toast.text, 90), Style::default().fg(theme.text)),
            ])
        }
        (None, Screen::Search, Focus::Input) => raw_key_hint(
            theme,
            &[("Enter", "搜索"), ("Esc", "返回列表"), ("Ctrl+U", "清空")],
        ),
        (None, Screen::Detail, _) => key_hint(
            app,
            &[
                (Action::Activate, "播放"),
                (Action::Enqueue, "加入队列"),
                (Action::EnqueueAll, "全部加入"),
                (Action::Back, "返回"),
            ],
        ),
        (None, Screen::Queue, _) => key_hint(
            app,
            &[
                (Action::Activate, "播放"),
                (Action::RemoveQueue, "移出队列"),
                (Action::Back, "返回"),
            ],
        ),
        (None, Screen::Home, _) => key_hint(
            app,
            &[
                (Action::PrevTab, "切换栏目"),
                (Action::Activate, "打开/播放"),
                (Action::Enqueue, "加入队列"),
                (Action::Search, "搜索"),
                (Action::Theme, "主题"),
                (Action::Help, "帮助"),
            ],
        ),
        _ => key_hint(
            app,
            &[
                (Action::PlayPause, "播放/暂停"),
                (Action::Next, "切歌"),
                (Action::SeekBack, "快退"),
                (Action::SeekForward, "快进"),
                (Action::VolumeUp, "音量"),
                (Action::Theme, "主题"),
                (Action::Help, "帮助"),
            ],
        ),
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().bg(theme.bg).fg(theme.muted)),
        area,
    );
}

fn key_hint(app: &App, keys: &[(Action, &str)]) -> Line<'static> {
    let theme = app.theme;
    let mut spans = vec![Span::raw("  ")];
    let mut first = true;
    for (action, desc) in keys {
        let label = if *action == Action::PrevTab {
            let prev = app
                .keymap
                .primary_label(Action::PrevTab)
                .unwrap_or_default();
            let next = app
                .keymap
                .primary_label(Action::NextTab)
                .unwrap_or_default();
            if prev.is_empty() && next.is_empty() {
                None
            } else if prev == "[" && next == "]" {
                Some("[/]".to_string())
            } else {
                Some(format!("{prev}/{next}"))
            }
        } else {
            app.keymap.primary_label(*action)
        };
        let Some(label) = label else {
            continue;
        };
        if !first {
            spans.push(Span::raw("   "));
        }
        first = false;
        let label = pretty_key(&label);
        let label = if label.chars().count() > 1 && !label.starts_with('[') {
            format!("[{label}]")
        } else {
            label
        };
        spans.push(Span::styled(
            label,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {desc}"),
            Style::default().fg(theme.muted),
        ));
    }
    Line::from(spans)
}

fn pretty_key(label: &str) -> String {
    match label {
        "enter" => "Enter".into(),
        "space" => "Space".into(),
        "esc" => "Esc".into(),
        "tab" => "Tab".into(),
        "backtab" => "Shift+Tab".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "pgup" => "PgUp".into(),
        "pgdown" => "PgDn".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        _ => label.to_string(),
    }
}

fn raw_key_hint(theme: Theme, keys: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![Span::raw("  ")];
    for (i, (key, desc)) in keys.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        let label = if (key.starts_with('[') && key.ends_with(']')) || key.chars().count() == 1 {
            (*key).to_string()
        } else {
            format!("[{key}]")
        };
        spans.push(Span::styled(
            label,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {desc}"),
            Style::default().fg(theme.muted),
        ));
    }
    Line::from(spans)
}

// Main views
// ----------------------------------------------------------------------

fn panel_block(title: &str, theme: Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(theme.base())
        .padding(Padding::new(1, 1, 0, 0))
        .title(Line::from(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )))
}

fn render_main(frame: &mut Frame, area: Rect, app: &App) {
    match app.screen {
        Screen::Home => render_home(frame, area, app),
        Screen::Search => render_search(frame, area, app),
        Screen::Library => render_library(frame, area, app),
        Screen::Detail => render_detail(frame, area, app),
        Screen::Queue => render_queue(frame, area, app),
        Screen::Lyrics => {}
    }
}

fn render_home(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let block = panel_block("发现", theme).title_bottom(Line::from(Span::styled(
        " [/] 切换栏目 ",
        Style::default().fg(theme.muted),
    )));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let chunks = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(inner);

    let mut tabs = vec![Span::raw(" ")];
    for (i, tab) in HomeTab::ALL.iter().enumerate() {
        if i > 0 {
            tabs.push(Span::styled("   ", theme.muted()));
        }
        let active = *tab == app.home_tab;
        tabs.push(Span::styled(
            format!(" {} ", tab.title()),
            if active {
                Style::default()
                    .fg(theme.accent)
                    .bg(theme.surface_alt)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            },
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(tabs)).style(theme.base()),
        chunks[0],
    );
    let mut tab_x = chunks[0].x.saturating_add(1);
    for tab in HomeTab::ALL {
        let label = format!(" {} ", tab.title());
        let width = UnicodeWidthStr::width(label.as_str()) as u16;
        hit(
            Rect::new(tab_x, chunks[0].y, width, 1),
            HitAction::HomeTab(tab),
        );
        tab_x = tab_x.saturating_add(width).saturating_add(3);
    }

    match app.home_tab {
        HomeTab::ForYou => {
            if app.home.daily.is_empty() {
                if app.home.loading {
                    loading_state(frame, chunks[1], app, "正在加载每日推荐");
                } else {
                    empty_state(
                        frame,
                        chunks[1],
                        app,
                        "每日推荐需要登录",
                        "按 L 扫码登录，解锁每日推荐、私人歌单与红心收藏",
                    );
                }
            } else {
                render_track_table(frame, chunks[1], app, &app.home.daily, app.home.selected);
            }
        }
        HomeTab::Charts => {
            if app.home.toplists.is_empty() {
                if app.home.loading {
                    loading_state(frame, chunks[1], app, "正在加载排行榜");
                } else {
                    empty_state(frame, chunks[1], app, "暂无排行榜", "按 R 刷新");
                }
            } else {
                render_playlist_table(
                    frame,
                    chunks[1],
                    app,
                    &app.home.toplists,
                    app.home.selected,
                    true,
                );
            }
        }
        HomeTab::Mixes => {
            if app.home.recommended.is_empty() {
                if app.home.loading {
                    loading_state(frame, chunks[1], app, "正在加载推荐歌单");
                } else {
                    empty_state(
                        frame,
                        chunks[1],
                        app,
                        "登录后查看推荐歌单",
                        "按 L 扫码登录，或在「排行榜」中浏览公开榜单",
                    );
                }
            } else {
                render_playlist_table(
                    frame,
                    chunks[1],
                    app,
                    &app.home.recommended,
                    app.home.selected,
                    false,
                );
            }
        }
    }
}

fn render_search(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let focused = app.focus == Focus::Input;
    let border = if focused { theme.accent } else { theme.border };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .style(theme.base())
        .padding(Padding::new(1, 1, 0, 0))
        .title(Line::from(vec![
            Span::styled(
                " 搜索 ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("网易云音乐", theme.muted()),
        ]));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);

    let mut spans = vec![Span::styled(" ⌕  ", Style::default().fg(theme.accent))];
    if app.search.input.is_empty() {
        spans.push(Span::styled(
            "输入歌名、歌手或专辑，回车搜索",
            Style::default().fg(theme.muted),
        ));
    } else {
        spans.push(Span::styled(
            app.search.input.clone(),
            Style::default().fg(theme.text),
        ));
    }
    if focused {
        spans.push(Span::styled(
            "▏",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme.base()),
        chunks[0],
    );
    hit(chunks[0], HitAction::FocusSearch);

    let status = if app.search.loading {
        Line::from(vec![
            Span::styled(
                format!(" {} ", spinner(app)),
                Style::default().fg(theme.accent),
            ),
            Span::styled("正在搜索…", theme.muted()),
        ])
    } else if let Some(error) = &app.search.error {
        Line::from(Span::styled(
            format!(" 搜索失败：{error}"),
            Style::default().fg(theme.red),
        ))
    } else if !app.search.results.is_empty() {
        Line::from(vec![
            Span::styled(
                format!(" 找到 {} 首", app.search.total),
                Style::default().fg(theme.green),
            ),
            Span::styled("  ·  Enter 播放整组结果  ·  a 加入队列", theme.muted()),
        ])
    } else {
        Line::from(Span::styled(" 输入关键词开始搜索", theme.muted()))
    };
    frame.render_widget(Paragraph::new(status).style(theme.base()), chunks[1]);

    if app.search.loading {
        loading_state(frame, chunks[2], app, "正在搜索网易云曲库");
    } else if app.search.results.is_empty() {
        empty_state(
            frame,
            chunks[2],
            app,
            "搜索你喜欢的音乐",
            "支持歌名、歌手、专辑；回车搜索，Esc 退出输入",
        );
    } else {
        render_track_table(
            frame,
            chunks[2],
            app,
            &app.search.results,
            app.search.selected,
        );
    }
}

fn render_library(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let block = panel_block("我的歌单", theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if app.library.playlists.is_empty() {
        if app.user.is_some() {
            empty_state(
                frame,
                inner,
                app,
                "还没有歌单",
                "在网易云音乐 App 中创建或收藏歌单后，按 R 刷新",
            );
        } else {
            empty_state(
                frame,
                inner,
                app,
                "登录后查看你的歌单",
                "按 L 打开登录面板，扫码即可同步歌单与红心收藏",
            );
        }
    } else {
        render_playlist_table(
            frame,
            inner,
            app,
            &app.library.playlists,
            app.library.selected,
            false,
        );
    }
}

fn render_detail(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let Some(detail) = &app.detail else {
        empty_state(frame, area, app, "没有歌单", "返回后重新选择");
        return;
    };
    let block = panel_block(
        &format!("歌单 · {}", truncate(&detail.playlist.name, 40)),
        theme,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let chunks = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(inner);
    let meta = Line::from(vec![
        Span::styled(
            format!(" {} ", detail.playlist.creator),
            Style::default().fg(theme.accent_alt),
        ),
        Span::styled("· ", theme.muted()),
        Span::styled(
            format!(
                "{} 首",
                detail
                    .tracks
                    .len()
                    .max(detail.playlist.track_count as usize)
            ),
            Style::default().fg(theme.text),
        ),
        Span::styled(" · ", theme.muted()),
        Span::styled(truncate(&detail.playlist.description, 60), theme.muted()),
    ]);
    frame.render_widget(Paragraph::new(meta).style(theme.base()), chunks[0]);

    if detail.loading {
        loading_state(frame, chunks[1], app, "正在加载歌单");
    } else if detail.tracks.is_empty() {
        empty_state(frame, chunks[1], app, "歌单为空", "换一个歌单试试");
    } else {
        render_track_table(frame, chunks[1], app, &detail.tracks, detail.selected);
    }
}

fn render_queue(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let block = panel_block("播放队列", theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner);
    let current = app
        .queue
        .current
        .map(|idx| format!("{}/{}", idx + 1, app.queue.tracks.len()))
        .unwrap_or_else(|| "0/0".into());
    let meta = Line::from(vec![
        Span::styled(
            format!(" {} 首", app.queue.tracks.len()),
            Style::default().fg(theme.text),
        ),
        Span::styled("  ·  当前 ", theme.muted()),
        Span::styled(current, Style::default().fg(theme.accent)),
        Span::styled(
            format!(
                "  ·  {}  ·  {}",
                if app.shuffle {
                    "随机播放"
                } else {
                    "顺序播放"
                },
                app.repeat.label()
            ),
            theme.muted(),
        ),
    ]);
    frame.render_widget(Paragraph::new(meta).style(theme.base()), chunks[0]);
    if app.queue.tracks.is_empty() {
        empty_state(
            frame,
            chunks[1],
            app,
            "播放队列为空",
            "在歌曲列表按 Enter 播放，或按 a 加入队列",
        );
    } else {
        render_track_table(frame, chunks[1], app, &app.queue.tracks, app.queue.selected);
    }
}

// ----------------------------------------------------------------------
// Tables
// ----------------------------------------------------------------------

fn render_track_table(frame: &mut Frame, area: Rect, app: &App, tracks: &[Track], selected: usize) {
    let theme = app.theme;
    let show_album = area.width >= 76;
    let widths = if show_album {
        vec![
            Constraint::Length(4),
            Constraint::Percentage(40),
            Constraint::Percentage(26),
            Constraint::Percentage(26),
            Constraint::Length(6),
            Constraint::Min(0),
        ]
    } else {
        vec![
            Constraint::Length(4),
            Constraint::Percentage(55),
            Constraint::Percentage(36),
            Constraint::Length(6),
            Constraint::Min(0),
        ]
    };
    let mut header = vec!["#", "歌曲", "歌手"];
    if show_album {
        header.push("专辑");
    }
    header.push("时长");
    header.push("");
    let header = Row::new(
        header
            .into_iter()
            .map(|text| {
                let style = Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::BOLD);
                if text == "时长" {
                    ratatui::widgets::Cell::from(Line::from(text).alignment(Alignment::Right))
                        .style(style)
                } else {
                    ratatui::widgets::Cell::from(text).style(style)
                }
            })
            .collect::<Vec<_>>(),
    )
    .height(1)
    .bottom_margin(1);

    let current_id = app.player.track.as_ref().map(|track| track.id);
    let rows: Vec<Row> = tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let playing = current_id == Some(track.id);
            let marker = if playing { "▶" } else { "" };
            let index_style = if playing {
                Style::default().fg(theme.green)
            } else {
                Style::default().fg(theme.muted)
            };
            let mut title = vec![Span::styled(
                track.name.clone(),
                Style::default().fg(theme.text),
            )];
            if track.is_vip() {
                title.push(Span::styled(
                    " VIP",
                    Style::default()
                        .fg(theme.yellow)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            if app.is_liked(track.id) {
                title.push(Span::styled(" ♥", Style::default().fg(theme.red)));
            }
            let mut cells = vec![
                ratatui::widgets::Cell::from(Line::from(vec![Span::styled(
                    format!("{marker}{:>2}", i + 1),
                    if playing { index_style } else { theme.muted() },
                )])),
                ratatui::widgets::Cell::from(Line::from(title)),
                ratatui::widgets::Cell::from(Span::styled(
                    truncate(&track.artist_text(), 30),
                    Style::default().fg(theme.muted),
                )),
            ];
            if show_album {
                cells.push(ratatui::widgets::Cell::from(Span::styled(
                    truncate(&track.album, 32),
                    Style::default().fg(theme.muted),
                )));
            }
            cells.push(ratatui::widgets::Cell::from(
                Line::from(track.duration_text()).alignment(Alignment::Right),
            ));
            cells.push(ratatui::widgets::Cell::from(""));
            Row::new(cells).style(theme.base())
        })
        .collect();

    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(1)
        .row_highlight_style(theme.selected())
        .style(theme.base());
    let mut state =
        TableState::default().with_selected(Some(selected.min(tracks.len().saturating_sub(1))));
    frame.render_stateful_widget(table, area, &mut state);
    let offset = state.offset();
    let data_top = area.y.saturating_add(2);
    let visible = area.height.saturating_sub(2) as usize;
    for row in 0..visible {
        let index = offset + row;
        if index >= tracks.len() {
            break;
        }
        hit(
            Rect::new(area.x, data_top.saturating_add(row as u16), area.width, 1),
            HitAction::SelectRow(index),
        );
    }
}

fn render_playlist_table(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    playlists: &[Playlist],
    selected: usize,
    charts: bool,
) {
    let theme = app.theme;
    let widths = if charts {
        vec![
            Constraint::Length(3),
            Constraint::Min(24),
            Constraint::Length(16),
            Constraint::Length(6),
            Constraint::Min(0),
        ]
    } else {
        vec![
            Constraint::Length(3),
            Constraint::Min(24),
            Constraint::Length(18),
            Constraint::Length(6),
            Constraint::Min(0),
        ]
    };
    let header = Row::new(
        if charts {
            vec!["#", "榜单", "更新", "曲目", ""]
        } else {
            vec!["#", "歌单", "创建者", "曲目", ""]
        }
        .into_iter()
        .map(|text| {
            let style = Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD);
            if text == "曲目" {
                ratatui::widgets::Cell::from(Line::from(text).alignment(Alignment::Right))
                    .style(style)
            } else {
                ratatui::widgets::Cell::from(text).style(style)
            }
        })
        .collect::<Vec<_>>(),
    )
    .bottom_margin(1);

    let rows: Vec<Row> = playlists
        .iter()
        .enumerate()
        .map(|(i, playlist)| {
            Row::new(vec![
                ratatui::widgets::Cell::from(Span::styled(
                    format!("{:>2}", i + 1),
                    Style::default().fg(theme.muted),
                )),
                ratatui::widgets::Cell::from(Line::from(vec![Span::styled(
                    truncate(&playlist.name, 40),
                    Style::default().fg(theme.text),
                )])),
                ratatui::widgets::Cell::from(Span::styled(
                    truncate(
                        if charts {
                            &playlist.update_frequency
                        } else {
                            &playlist.creator
                        },
                        18,
                    ),
                    Style::default().fg(theme.muted),
                )),
                ratatui::widgets::Cell::from(
                    Line::from(playlist.track_count.to_string()).alignment(Alignment::Right),
                ),
                ratatui::widgets::Cell::from(""),
            ])
            .style(theme.base())
        })
        .collect();

    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(1)
        .row_highlight_style(theme.selected())
        .style(theme.base());
    let mut state =
        TableState::default().with_selected(Some(selected.min(playlists.len().saturating_sub(1))));
    frame.render_stateful_widget(table, area, &mut state);
    let offset = state.offset();
    let data_top = area.y.saturating_add(2);
    let visible = area.height.saturating_sub(2) as usize;
    for row in 0..visible {
        let index = offset + row;
        if index >= playlists.len() {
            break;
        }
        hit(
            Rect::new(area.x, data_top.saturating_add(row as u16), area.width, 1),
            HitAction::SelectRow(index),
        );
    }
}

// ----------------------------------------------------------------------
// Lyrics
// ----------------------------------------------------------------------

fn lyric_window<'a>(app: &App, height: usize) -> Vec<Line<'a>> {
    let theme = app.theme;
    let lyrics = &app.lyrics.lyrics;
    if lyrics.lines.is_empty() {
        if !lyrics.untimed.is_empty() {
            return lyrics
                .untimed
                .iter()
                .take(height)
                .map(|line| {
                    Line::from(Span::styled(line.clone(), Style::default().fg(theme.muted)))
                        .alignment(Alignment::Center)
                })
                .collect();
        }
        return Vec::new();
    }
    let active = app.lyrics.active.unwrap_or(0) as i32 + app.lyrics.offset;
    let total = lyrics.lines.len() as i32;
    let half = height as i32 / 2;
    let start = (active - half).clamp(0, total.saturating_sub(height as i32));
    let mut out = Vec::new();
    for idx in start..total.min(start + height as i32) {
        let idx = idx as usize;
        let line = &lyrics.lines[idx];
        let is_active = app.lyrics.active == Some(idx) && app.lyrics.offset == 0;
        let distance = (idx as i32 - active).abs();
        let (fg, modifier) = if is_active {
            (theme.accent, Modifier::BOLD)
        } else if distance <= 1 {
            (theme.text, Modifier::empty())
        } else {
            (theme.muted, Modifier::empty())
        };
        out.push(
            Line::from(Span::styled(
                line.text.clone(),
                Style::default().fg(fg).add_modifier(modifier),
            ))
            .alignment(Alignment::Center),
        );
        if is_active {
            if let Some(translation) = &line.translation {
                out.push(
                    Line::from(Span::styled(
                        translation.clone(),
                        Style::default().fg(theme.muted),
                    ))
                    .alignment(Alignment::Center),
                );
            }
        }
    }
    out
}

fn render_lyrics_panel(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let block = panel_block("歌词", theme).title_bottom(Line::from(Span::styled(
        " y 全屏 ",
        Style::default().fg(theme.muted),
    )));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    hit(inner, HitAction::LyricsArea);
    if app.lyrics.loading {
        loading_state(frame, inner, app, "正在加载歌词");
        return;
    }
    let height = inner.height as usize;
    let lines = lyric_window(app, height);
    if lines.is_empty() {
        empty_state(
            frame,
            inner,
            app,
            "暂无歌词",
            app.lyrics
                .error
                .as_deref()
                .unwrap_or("这首歌没有可用的歌词"),
        );
    } else {
        frame.render_widget(
            Paragraph::new(lines)
                .style(theme.base())
                .wrap(Wrap { trim: true }),
            inner,
        );
    }
}

fn render_lyrics_full(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    frame.render_widget(Block::default().style(theme.base()), area);

    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(area);
    let top = rows[0];
    let center = rows[1];
    let bottom = rows[2];

    let track = app.player.track.as_ref();
    let playing = match app.snapshot.status {
        crate::audio::AudioStatus::Playing => "▶",
        crate::audio::AudioStatus::Paused => "⏸",
        _ => "■",
    };
    let title = track
        .map(|track| format!("{}  —  {}", track.name, track.artist_text()))
        .unwrap_or_else(|| "未在播放".into());
    let title_line = Line::from(vec![
        Span::styled(
            format!(" {playing}  "),
            Style::default()
                .fg(
                    if app.snapshot.status == crate::audio::AudioStatus::Playing {
                        theme.green
                    } else {
                        theme.muted
                    },
                )
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate(&title, 78),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(title_line).style(theme.base()),
        Rect::new(
            top.x.saturating_add(2),
            top.y,
            top.width.saturating_sub(4),
            1,
        ),
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "纯享歌词 ",
            Style::default().fg(theme.muted),
        )))
        .alignment(Alignment::Right)
        .style(theme.base()),
        Rect::new(
            top.x.saturating_add(2),
            top.y,
            top.width.saturating_sub(4),
            1,
        ),
    );

    let album = track.map(|track| track.album.as_str()).unwrap_or("");
    let meta = Line::from(vec![
        Span::styled(
            format!(" {}", truncate(album, 44)),
            Style::default().fg(theme.muted),
        ),
        Span::styled("  ·  ", theme.muted()),
        Span::styled(app.quality_text(), Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(
        Paragraph::new(meta).style(theme.base()),
        Rect::new(
            top.x.saturating_add(2),
            top.y.saturating_add(1),
            top.width.saturating_sub(4),
            1,
        ),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format_time(app.snapshot.position),
                Style::default().fg(theme.accent),
            ),
            Span::styled(" / ", theme.muted()),
            Span::styled(
                format_time(app.snapshot.duration),
                Style::default().fg(theme.muted),
            ),
        ]))
        .alignment(Alignment::Right)
        .style(theme.base()),
        Rect::new(
            top.x.saturating_add(2),
            top.y.saturating_add(1),
            top.width.saturating_sub(4),
            1,
        ),
    );
    frame.render_widget(
        Paragraph::new("─".repeat(area.width.saturating_sub(4) as usize))
            .style(Style::default().fg(theme.border).bg(theme.bg)),
        Rect::new(
            top.x.saturating_add(2),
            top.y.saturating_add(2),
            top.width.saturating_sub(4),
            1,
        ),
    );

    let lyric_width = area.width.saturating_sub(12).clamp(24, 92);
    let columns = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(lyric_width),
        Constraint::Fill(1),
    ])
    .split(center);
    let height = columns[1].height as usize;
    let synced = !app.lyrics.loading && !app.lyrics.lyrics.lines.is_empty();
    let active = app.lyrics.active.unwrap_or(0);
    let top_pad = if synced {
        (height / 2).saturating_sub(active)
    } else {
        0
    };
    let window_height = height.saturating_sub(top_pad);
    let mut lines = if app.lyrics.loading {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("{}  正在加载歌词…", spinner(app)),
                Style::default().fg(theme.muted),
            ))
            .alignment(Alignment::Center),
        ]
    } else if synced {
        lyric_window(app, window_height)
    } else {
        Vec::new()
    };
    if lines.is_empty() {
        lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                app.lyrics
                    .error
                    .clone()
                    .unwrap_or_else(|| "暂无歌词".into()),
                Style::default().fg(theme.muted),
            ))
            .alignment(Alignment::Center),
            Line::from(""),
        ];
    }
    let pre_pad = if synced {
        top_pad
    } else {
        height.saturating_sub(lines.len()) / 2
    };
    let mut out = Vec::with_capacity(lines.len() + pre_pad);
    for _ in 0..pre_pad {
        out.push(Line::from(""));
    }
    out.extend(lines);
    frame.render_widget(
        Paragraph::new(out)
            .style(theme.base())
            .alignment(Alignment::Center),
        columns[1],
    );

    let gauge_width = area.width.saturating_sub(24).clamp(20, 64);
    let gauge_columns = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(gauge_width),
        Constraint::Fill(1),
    ])
    .split(Rect::new(bottom.x, bottom.y, bottom.width, 1));
    let ratio = if app.snapshot.duration.is_zero() {
        0.0
    } else {
        (app.snapshot.position.as_secs_f64() / app.snapshot.duration.as_secs_f64()).clamp(0.0, 1.0)
    };
    frame.render_widget(
        LineGauge::default()
            .ratio(ratio)
            .filled_style(Style::default().fg(theme.accent))
            .unfilled_style(Style::default().fg(theme.border))
            .label(""),
        gauge_columns[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "y / Esc 退出   ·   j / k 滚动   ·   g 回到当前   ·   Space 暂停",
            Style::default().fg(theme.muted),
        )))
        .alignment(Alignment::Center)
        .style(theme.base()),
        Rect::new(bottom.x, bottom.y.saturating_add(1), bottom.width, 1),
    );
}

fn render_player(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(theme.base())
        .padding(Padding::new(1, 1, 0, 0))
        .title(Line::from(vec![
            Span::styled(
                " 正在播放 ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            if app.player.loading {
                Span::styled(
                    format!("{} ", spinner(app)),
                    Style::default().fg(theme.yellow),
                )
            } else {
                Span::raw("")
            },
        ]))
        .title_top(
            Line::from(Span::styled(
                format!(" {} ", app.quality_text()),
                Style::default().fg(theme.muted),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    hit(
        Rect::new(area.x, area.y, area.width, area.height),
        HitAction::PlayerArea,
    );
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);

    let track = app.player.track.as_ref();
    if let Some(track) = track {
        let status_icon = if app.player.loading {
            "◌"
        } else if app.snapshot.status == crate::audio::AudioStatus::Paused {
            "⏸"
        } else if app.snapshot.status == crate::audio::AudioStatus::Playing {
            "▶"
        } else {
            "■"
        };
        let icon_color = match app.snapshot.status {
            crate::audio::AudioStatus::Playing => theme.green,
            crate::audio::AudioStatus::Paused => theme.yellow,
            _ => theme.muted,
        };
        let mut left = vec![
            Span::styled(
                format!(" {status_icon} "),
                Style::default().fg(icon_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                truncate(&track.name, 46),
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  —  ", theme.muted()),
            Span::styled(
                truncate(&track.artist_text(), 30),
                Style::default().fg(theme.muted),
            ),
        ];
        if app.player.trial {
            left.push(Span::styled(
                "  试听",
                Style::default()
                    .fg(theme.yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if app.is_liked(track.id) {
            left.push(Span::styled("  ♥", Style::default().fg(theme.red)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(left)).style(theme.base()),
            rows[0],
        );

        let right = Line::from(vec![
            Span::styled(
                format!("{} ", app.repeat.icon()),
                Style::default().fg(if app.repeat == crate::app::Repeat::Off {
                    theme.muted
                } else {
                    theme.accent
                }),
            ),
            Span::styled(
                if app.shuffle {
                    "⇄ 随机 "
                } else {
                    "⇄ 顺序 "
                },
                Style::default().fg(if app.shuffle {
                    theme.accent
                } else {
                    theme.muted
                }),
            ),
            Span::styled("·  ", theme.muted()),
            Span::styled(
                format!("音量 {}%", app.cfg.volume),
                Style::default().fg(if app.cfg.volume == 0 {
                    theme.red
                } else {
                    theme.muted
                }),
            ),
            Span::raw(" "),
        ])
        .right_aligned();
        frame.render_widget(Paragraph::new(right).style(theme.base()), rows[0]);

        let progress = Layout::horizontal([
            Constraint::Length(7),
            Constraint::Min(12),
            Constraint::Length(7),
            Constraint::Length(10),
        ])
        .split(rows[1]);
        frame.render_widget(
            Paragraph::new(Span::styled(
                format_time(app.snapshot.position),
                Style::default().fg(theme.muted),
            ))
            .style(theme.base()),
            progress[0],
        );
        let ratio = if app.snapshot.duration.is_zero() {
            0.0
        } else {
            (app.snapshot.position.as_secs_f64() / app.snapshot.duration.as_secs_f64())
                .clamp(0.0, 1.0)
        };
        let gauge = LineGauge::default()
            .ratio(ratio)
            .filled_style(Style::default().fg(theme.accent))
            .unfilled_style(Style::default().fg(theme.muted))
            .label("");
        frame.render_widget(gauge, progress[1]);
        frame.render_widget(
            Paragraph::new(Span::styled(
                format_time(app.snapshot.duration),
                Style::default().fg(theme.muted),
            ))
            .style(theme.base()),
            progress[2],
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                if app.snapshot.backend.contains("静音") {
                    "静音模拟".to_string()
                } else {
                    "系统音频".to_string()
                },
                Style::default().fg(theme.muted),
            ))
            .alignment(Alignment::Right)
            .style(theme.base()),
            progress[3],
        );
        hit(Rect::new(inner.x, rows[0].y, 5, 1), HitAction::PlayPause);
        hit(
            Rect::new(
                inner.x.saturating_add(5),
                rows[0].y,
                inner.width.saturating_sub(55),
                1,
            ),
            HitAction::OpenLyrics,
        );
        hit(progress[1], HitAction::Seek);
        hit(
            Rect::new(
                inner.x.saturating_add(inner.width.saturating_sub(10)),
                rows[0].y,
                10,
                1,
            ),
            HitAction::Mute,
        );
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " 未在播放  ·  从搜索或歌单中选择一首歌",
                theme.muted(),
            )))
            .style(theme.base()),
            rows[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " Space 播放/暂停   n/p 切歌   t 主题   ? 帮助",
                Style::default().fg(theme.border),
            )))
            .style(theme.base()),
            rows[1],
        );
    }
}

// ----------------------------------------------------------------------
// States / overlays
// ----------------------------------------------------------------------

fn loading_state(frame: &mut Frame, area: Rect, app: &App, label: &str) {
    let theme = app.theme;
    let line = Line::from(vec![
        Span::styled(
            format!("{} ", spinner(app)),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_string(), theme.muted()),
    ]);
    let rows = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(line)
            .alignment(Alignment::Center)
            .style(theme.base()),
        rows[1],
    );
}

fn empty_state(frame: &mut Frame, area: Rect, app: &App, title: &str, subtitle: &str) {
    let theme = app.theme;
    let rows = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .split(area);
    let lines = vec![
        Line::from(Span::styled(
            "♪",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(Span::styled(
            title.to_string(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(Span::styled(
            subtitle.to_string(),
            Style::default().fg(theme.muted),
        ))
        .alignment(Alignment::Center),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(theme.base())
            .wrap(Wrap { trim: true }),
        rows[1],
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2)).max(20);
    let height = height.min(area.height.saturating_sub(2)).max(8);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn overlay_backdrop(frame: &mut Frame, area: Rect, theme: Theme) {
    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(Style::default().bg(theme.bg)), area);
}

fn render_help(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    overlay_backdrop(frame, area, theme);
    hit(area, HitAction::CloseOverlay);
    let popup = if area.width < 100 || area.height < 30 {
        Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height.saturating_sub(1).max(12),
        }
    } else {
        centered(area, 98, 24)
    };
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .padding(Padding::new(2, 2, 1, 1))
        .title(Line::from(vec![
            Span::styled(
                " 快捷键 ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Keyboard shortcuts", theme.muted()),
        ]))
        .title_bottom(Line::from(Span::styled(
            " Esc / ? 关闭   ·   默认键位，可在 config.toml [keys] 覆盖 ",
            Style::default().fg(theme.muted),
        )));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let left: [(&str, &[(&str, &str)]); 2] = [
        (
            "播放",
            &[
                ("Space", "暂停 / 继续"),
                ("n / p", "下一首 / 上一首"),
                ("← / →", "快退 / 快进 5 秒"),
                ("+ / -", "音量 ±5%"),
                ("m", "静音"),
                ("s / r", "随机 / 循环模式"),
                ("f / y", "队列 / 纯享歌词"),
            ],
        ),
        (
            "浏览",
            &[
                ("1 - 4", "切换页面"),
                ("Tab", "循环切换页面"),
                ("j / k", "上下移动"),
                ("h / l", "上一页 / 下一页"),
                ("gg / G", "跳到首 / 尾"),
                ("Ctrl+d/u", "半页下 / 上"),
                ("Enter", "播放 / 打开歌单"),
                ("Esc", "返回上一层"),
            ],
        ),
    ];
    let right: [(&str, &[(&str, &str)]); 2] = [
        (
            "功能",
            &[
                ("/", "搜索"),
                ("a / A", "加入队列 / 全部"),
                ("z", "喜欢当前歌曲"),
                ("t", "切换主题"),
                ("b", "侧边栏开关"),
                ("L", "登录网易云"),
                ("Ctrl+L", "退出登录"),
                ("q", "退出播放器"),
            ],
        ),
        (
            "提示",
            &[
                ("搜索框", "按键作为文字输入"),
                ("Esc", "返回列表再用快捷键"),
                ("Cookie", "保存在 ~/.config/7player"),
                ("按键", "config.toml [keys] 可覆盖"),
            ],
        ),
    ];

    let cols =
        Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)]).split(inner);
    frame.render_widget(
        Paragraph::new(help_column(theme, &left)).style(Style::default().bg(theme.surface)),
        cols[0],
    );
    frame.render_widget(
        Paragraph::new(help_column(theme, &right)).style(Style::default().bg(theme.surface)),
        cols[1],
    );
}

fn help_column(theme: Theme, sections: &[(&str, &[(&str, &str)])]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (title, rows) in sections {
        lines.push(Line::from(Span::styled(
            format!("  {title}"),
            Style::default()
                .fg(theme.accent_alt)
                .add_modifier(Modifier::BOLD),
        )));
        for (key, desc) in rows.iter() {
            lines.push(Line::from(vec![
                Span::styled("    ", theme.muted()),
                Span::styled(
                    format!("{key:<10}"),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled((*desc).to_string(), Style::default().fg(theme.text)),
            ]));
        }
        lines.push(Line::from(""));
    }
    lines
}

fn theme_description(name: &str) -> &'static str {
    match name {
        "tokyo-night" => "深夜蓝紫，默认",
        "catppuccin-mocha" => "柔和低饱和",
        "dracula" => "暗夜紫粉",
        "nord" => "极地冷色",
        "gruvbox-dark" => "复古暖色",
        "solarized-dark" => "经典低对比",
        "rose-pine" => "月下玫瑰",
        "one-dark" => "Atom 经典",
        "catppuccin-latte" => "明亮柔和",
        "solarized-light" => "纸上日光",
        _ => "",
    }
}

fn render_theme_picker(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    hit(area, HitAction::CloseOverlay);
    let Some(picker) = &app.theme_picker else {
        return;
    };
    let popup = centered(area, 64, 14);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .padding(Padding::new(2, 2, 1, 1))
        .title(Line::from(vec![
            Span::styled(
                " 主题 ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Themes", theme.muted()),
        ]))
        .title_bottom(Line::from(Span::styled(
            " Enter 应用 · Esc 取消 · j/k 预览 ",
            Style::default().fg(theme.muted),
        )));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines = Vec::new();
    for (index, name) in crate::ui::theme::THEME_NAMES.iter().enumerate() {
        let candidate = theme_at(index);
        let selected = index == picker.index;
        let current = app.cfg.theme == *name;
        let prefix = if selected { "▸ " } else { "  " };
        let mut spans = vec![
            Span::styled(
                prefix.to_string(),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<20}", candidate.label),
                if selected {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text)
                },
            ),
        ];
        for color in candidate.swatches() {
            spans.push(Span::styled("██", Style::default().fg(color)));
        }
        spans.push(Span::styled(
            format!("  {}", theme_description(name)),
            Style::default().fg(theme.muted),
        ));
        if current {
            spans.push(Span::styled("  ●", Style::default().fg(theme.green)));
        }
        lines.push(Line::from(spans));
        if (index as u16) < inner.height {
            hit(
                Rect::new(
                    inner.x,
                    inner.y.saturating_add(index as u16),
                    inner.width,
                    1,
                ),
                HitAction::ThemeRow(index),
            );
        }
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.surface)),
        inner,
    );
}

fn render_login_compact(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let Some(login) = &app.login else {
        return;
    };
    overlay_backdrop(frame, area, theme);
    hit(area, HitAction::Noop);
    let popup = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(1).max(10),
    };
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .padding(Padding::new(2, 2, 1, 1))
        .title(Line::from(Span::styled(
            " 登录网易云音乐 ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )))
        .title_bottom(Line::from(Span::styled(
            " Esc 关闭 · Tab 切换 ",
            Style::default().fg(theme.muted),
        )));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);

    let mut tabs = vec![Span::raw(" ")];
    for (index, (tab, label)) in [
        (LoginTab::Qr, "二维码登录"),
        (LoginTab::Phone, "手机号登录"),
    ]
    .into_iter()
    .enumerate()
    {
        if index > 0 {
            tabs.push(Span::styled("    ", theme.muted()));
        }
        let active = login.tab == tab;
        tabs.push(Span::styled(
            format!(" {label} "),
            if active {
                Style::default()
                    .fg(theme.accent)
                    .bg(theme.surface_alt)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            },
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(tabs)), chunks[0]);
    let mut tab_x = chunks[0].x.saturating_add(1);
    for (tab, label) in [
        (LoginTab::Qr, "二维码登录"),
        (LoginTab::Phone, "手机号登录"),
    ] {
        let width = UnicodeWidthStr::width(format!(" {label} ").as_str()) as u16;
        hit(
            Rect::new(tab_x, chunks[0].y, width, 1),
            HitAction::LoginTab(tab),
        );
        tab_x = tab_x.saturating_add(width).saturating_add(4);
    }

    match login.tab {
        LoginTab::Qr => {
            let lines = vec![
                Line::from(Span::styled(
                    "终端窗口过小，二维码无法完整显示",
                    Style::default()
                        .fg(theme.yellow)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "请将窗口调整到至少 100×30，或按 Tab 改用手机号登录。",
                    Style::default().fg(theme.text),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("当前状态：{}", login.qr_status),
                    Style::default().fg(theme.accent),
                )),
                Line::from(Span::styled(
                    "登录状态只保存在本机 Cookie（0600）。",
                    theme.muted(),
                )),
            ];
            frame.render_widget(
                Paragraph::new(lines)
                    .style(Style::default().bg(theme.surface))
                    .wrap(Wrap { trim: true }),
                chunks[1],
            );
        }
        LoginTab::Phone => {
            let rows = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(chunks[1]);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    "手机号 + 密码登录（扫码更稳定）",
                    theme.muted(),
                ))
                .style(Style::default().bg(theme.surface)),
                rows[0],
            );
            render_field(
                frame,
                rows[1],
                app,
                "手机号",
                &login.phone,
                login.field == 0,
                false,
            );
            render_field(
                frame,
                rows[2],
                app,
                "密码",
                &login.password,
                login.field == 1,
                true,
            );
            hit(rows[1], HitAction::LoginField(0));
            hit(rows[2], HitAction::LoginField(1));
            let status = if login.submitting {
                Line::from(vec![
                    Span::styled(
                        format!("{} ", spinner(app)),
                        Style::default().fg(theme.accent),
                    ),
                    Span::styled("正在登录…", theme.muted()),
                ])
            } else if let Some(error) = &login.error {
                Line::from(Span::styled(
                    format!("登录失败：{error}"),
                    Style::default().fg(theme.red),
                ))
            } else {
                Line::from(Span::styled("Tab 切换输入框 · Enter 登录", theme.muted()))
            };
            frame.render_widget(
                Paragraph::new(status).style(Style::default().bg(theme.surface)),
                rows[3],
            );
            hit(rows[3], HitAction::LoginSubmit);
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "提示：调整窗口大小后重新按 L 可显示完整二维码。",
            theme.muted(),
        )))
        .style(Style::default().bg(theme.surface)),
        chunks[2],
    );
}

fn render_login(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let Some(login) = &app.login else {
        return;
    };
    if area.height < 30 || area.width < 90 {
        render_login_compact(frame, area, app);
        return;
    }
    overlay_backdrop(frame, area, theme);
    hit(area, HitAction::Noop);
    let popup = centered(area, 78, 30);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .padding(Padding::new(2, 2, 1, 1))
        .title(Line::from(vec![
            Span::styled(
                " 登录网易云音乐 ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("NetEase Cloud Music", theme.muted()),
        ]))
        .title_bottom(Line::from(Span::styled(
            " Esc 关闭 · Tab 切换方式 ",
            Style::default().fg(theme.muted),
        )));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);

    let mut tabs = vec![Span::raw(" ")];
    for (index, (tab, label)) in [
        (LoginTab::Qr, "二维码登录"),
        (LoginTab::Phone, "手机号登录"),
    ]
    .into_iter()
    .enumerate()
    {
        if index > 0 {
            tabs.push(Span::styled("    ", theme.muted()));
        }
        let active = login.tab == tab;
        tabs.push(Span::styled(
            format!(" {label} "),
            if active {
                Style::default()
                    .fg(theme.accent)
                    .bg(theme.surface_alt)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            },
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(tabs)), chunks[0]);
    let mut tab_x = chunks[0].x.saturating_add(1);
    for (tab, label) in [
        (LoginTab::Qr, "二维码登录"),
        (LoginTab::Phone, "手机号登录"),
    ] {
        let width = UnicodeWidthStr::width(format!(" {label} ").as_str()) as u16;
        hit(
            Rect::new(tab_x, chunks[0].y, width, 1),
            HitAction::LoginTab(tab),
        );
        tab_x = tab_x.saturating_add(width).saturating_add(4);
    }

    match login.tab {
        LoginTab::Qr => {
            let cols =
                Layout::horizontal([Constraint::Length(46), Constraint::Min(0)]).split(chunks[1]);
            let qr_area = Rect {
                x: cols[0].x,
                y: cols[0].y,
                width: cols[0].width,
                height: cols[0].height,
            };
            if login.qr_loading {
                loading_state(frame, qr_area, app, "正在生成二维码");
            } else if login.qr_lines.is_empty() {
                empty_state(
                    frame,
                    qr_area,
                    app,
                    "二维码加载失败",
                    login.error.as_deref().unwrap_or("按 r 重试"),
                );
            } else {
                let height = login.qr_lines.len() as u16;
                let qr_rect = Rect {
                    x: qr_area.x,
                    y: qr_area.y + qr_area.height.saturating_sub(height) / 2,
                    width: qr_area.width,
                    height: height.min(qr_area.height),
                };
                let lines: Vec<Line> = login
                    .qr_lines
                    .iter()
                    .map(|line| Line::from(Span::raw(line.clone())))
                    .collect();
                frame.render_widget(
                    Paragraph::new(lines).style(
                        Style::default()
                            .fg(Color::Rgb(0x10, 0x10, 0x18))
                            .bg(Color::Rgb(0xf4, 0xf4, 0xf8)),
                    ),
                    qr_rect,
                );
            }

            let mut info = vec![
                Line::from(Span::styled(
                    "使用网易云音乐 App 扫码",
                    Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    login.qr_status.clone(),
                    Style::default().fg(theme.accent),
                )),
                Line::from(""),
                Line::from(Span::styled("· 扫码后在手机上点击确认", theme.muted())),
                Line::from(Span::styled("· 登录状态仅保存在本机", theme.muted())),
                Line::from(Span::styled("· Cookie 文件权限为 0600", theme.muted())),
                Line::from(""),
            ];
            if let Some(error) = &login.error {
                info.push(Line::from(Span::styled(
                    truncate(error, 34),
                    Style::default().fg(theme.red),
                )));
            }
            info.push(Line::from(Span::styled(
                "r 刷新二维码",
                Style::default().fg(theme.accent_alt),
            )));
            frame.render_widget(
                Paragraph::new(info)
                    .style(Style::default().bg(theme.surface))
                    .wrap(Wrap { trim: true }),
                cols[1],
            );
            if cols[1].height > 9 {
                hit(
                    Rect::new(cols[1].x, cols[1].y + 8, cols[1].width, 1),
                    HitAction::QrRefresh,
                );
                hit(
                    Rect::new(cols[1].x, cols[1].y + 9, cols[1].width, 1),
                    HitAction::QrRefresh,
                );
            }
        }
        LoginTab::Phone => {
            let rows = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(chunks[1]);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    "使用手机号 + 密码登录（部分账号会要求验证码，建议优先扫码）",
                    Style::default().fg(theme.muted),
                ))
                .style(Style::default().bg(theme.surface)),
                rows[0],
            );
            render_field(
                frame,
                rows[1],
                app,
                "手机号",
                &login.phone,
                login.field == 0,
                false,
            );
            render_field(
                frame,
                rows[2],
                app,
                "密码",
                &login.password,
                login.field == 1,
                true,
            );
            hit(rows[1], HitAction::LoginField(0));
            hit(rows[2], HitAction::LoginField(1));
            let status = if login.submitting {
                Line::from(vec![
                    Span::styled(
                        format!("{} ", spinner(app)),
                        Style::default().fg(theme.accent),
                    ),
                    Span::styled("正在登录…", theme.muted()),
                ])
            } else if let Some(error) = &login.error {
                Line::from(Span::styled(
                    format!("登录失败：{error}"),
                    Style::default().fg(theme.red),
                ))
            } else {
                Line::from(Span::styled(
                    "Tab 切换输入框 · Enter 登录 · Esc 关闭",
                    theme.muted(),
                ))
            };
            frame.render_widget(
                Paragraph::new(status).style(Style::default().bg(theme.surface)),
                rows[3],
            );
            hit(rows[3], HitAction::LoginSubmit);
        }
    }

    let footer = Line::from(vec![
        Span::styled(" 提示：", Style::default().fg(theme.accent_alt)),
        Span::styled(
            "扫码登录最稳定；登录后可同步歌单、每日推荐与红心。",
            theme.muted(),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().bg(theme.surface)),
        chunks[2],
    );
}

fn render_field(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    label: &str,
    value: &str,
    focused: bool,
    secret: bool,
) {
    let theme = app.theme;
    let border = if focused { theme.accent } else { theme.border };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(theme.bg))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            format!(" {label} "),
            Style::default().fg(if focused { theme.accent } else { theme.muted }),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let shown = if secret {
        "•".repeat(value.chars().count())
    } else {
        value.to_string()
    };
    let mut spans = vec![Span::styled(
        if shown.is_empty() {
            if secret {
                "输入密码".to_string()
            } else {
                "输入手机号".to_string()
            }
        } else {
            shown
        },
        Style::default().fg(if value.is_empty() {
            theme.muted
        } else {
            theme.text
        }),
    )];
    if focused {
        spans.push(Span::styled(
            "▏",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.bg)),
        inner,
    );
}

// ----------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------

fn format_time(duration: Duration) -> String {
    let secs = duration.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

fn truncate(input: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    if max_width == 0 {
        return String::new();
    }
    let mut width = 0;
    let mut out = String::new();
    for ch in input.chars() {
        let w = ch.width().unwrap_or(0);
        if width + w > max_width {
            break;
        }
        out.push(ch);
        width += w;
    }
    if width < input.chars().map(|c| c.width().unwrap_or(0)).sum() && width >= 1 {
        out.pop();
        out.push('…');
    }
    out
}
