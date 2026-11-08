//! 7player — a NetEase Cloud Music player for the terminal.

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use seven_player::app::App;
use seven_player::config;
use seven_player::netease::NeteaseClient;
use seven_player::ui;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::time::{interval, MissedTickBehavior};

#[derive(Debug, Parser)]
#[command(
    name = "7player",
    version,
    about = "7player — 网易云音乐 TUI 播放器 / NetEase Cloud Music for the terminal",
    long_about = None
)]
struct Cli {
    /// 启动时使用的主题（tokyo-night, catppuccin-mocha, dracula, nord,
    /// gruvbox-dark, solarized-dark, rose-pine, one-dark,
    /// catppuccin-latte, solarized-light）
    #[arg(long, value_name = "NAME")]
    theme: Option<String>,

    /// 不打开音频设备，使用静音模拟（适合无头/CI/WSL 无声卡环境）
    #[arg(long)]
    no_audio: bool,

    /// 跳过启动时的 7player ASCII 入场动画
    #[arg(long)]
    no_splash: bool,

    /// 运行联网自检并退出（匿名会话、排行榜、搜索、音源、歌词）
    #[arg(long)]
    check: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    config::migrate_legacy();
    if cli.check {
        return run_check().await;
    }

    let (tx, rx) = unbounded_channel();
    let mut app = App::new(cli.theme, cli.no_audio, cli.no_splash, tx)?;
    let result = run_tui(&mut app, rx).await;
    app.shutdown();
    result
}

async fn run_tui(
    app: &mut App,
    mut rx: UnboundedReceiver<seven_player::worker::Msg>,
) -> Result<()> {
    enable_raw_mode().context("无法进入终端 raw 模式（请确认在真正的 TTY 中运行）")?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        crossterm::cursor::Hide
    )
    .context("无法进入备用屏幕")?;
    let guard = TerminalGuard;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).context("初始化终端失败")?;
    terminal.clear().ok();

    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            crossterm::cursor::Show
        );
        old_hook(info);
    }));

    let mut events = EventStream::new();
    let mut tick = interval(Duration::from_millis(16));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;
        app.set_hits(ui::take_hits());

        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => app.on_key(key),
                    Some(Ok(Event::Mouse(mouse))) => app.on_mouse(mouse),
                    Some(Ok(Event::Resize(_, _))) => {}
                    Some(Ok(_)) => {}
                    Some(Err(_)) => {}
                    None => app.should_quit = true,
                }
            }
            Some(message) = rx.recv() => app.on_msg(message),
            _ = tick.tick() => app.on_tick(),
        }

        if app.should_quit {
            break;
        }
    }

    drop(terminal);
    drop(guard);
    Ok(())
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            crossterm::cursor::Show
        );
    }
}

/// Headless connectivity / feature check. Useful in CI and when debugging login
/// or API changes without entering the TUI.
async fn run_check() -> Result<()> {
    println!("7player {} · 联网自检", env!("CARGO_PKG_VERSION"));
    println!("配置目录: {}", config::config_dir().display());
    println!("缓存目录: {}", config::audio_cache_dir().display());
    let client = NeteaseClient::new(config::cookie_path())?;

    if !client.has_music_u() {
        print!("匿名会话注册… ");
        match client.register_anonymous().await {
            Ok(()) => println!("ok"),
            Err(err) => println!("跳过（{err}）"),
        }
    }

    match client.login_status().await {
        Ok(Some(user)) => println!("登录状态: {} (uid={})", user.nickname, user.uid),
        Ok(None) => println!("登录状态: 未登录（可使用 TUI 中的 L 扫码登录）"),
        Err(err) => println!("登录状态: 查询失败（{err}）"),
    }

    let charts = client.toplists().await.context("排行榜接口失败")?;
    println!("排行榜: {} 个", charts.len());
    for chart in charts.iter().take(3) {
        println!("  · {} ({})", chart.name, chart.update_frequency);
    }

    let keyword = "周杰伦";
    let page = client
        .search_songs(keyword, 0, 10)
        .await
        .with_context(|| format!("搜索「{keyword}」失败"))?;
    println!(
        "搜索「{keyword}」: 共 {} 首，返回 {} 首",
        page.total,
        page.tracks.len()
    );
    for track in page.tracks.iter().take(5) {
        println!(
            "  · {} - {} [{}]{}",
            track.name,
            track.artist_text(),
            track.duration_text(),
            if track.is_vip() { " VIP" } else { "" }
        );
    }

    if let Some(track) = page.tracks.first() {
        print!("音源「{}」… ", track.name);
        match client.song_url(track.id, "standard").await {
            Ok(url) => println!("ok ({} kbps, {}, {})", url.br / 1000, url.level, url.kind),
            Err(err) => println!("失败（{err}）"),
        }
        print!("歌词… ");
        match client.lyric(track.id).await {
            Ok(lyrics) => println!(
                "ok ({} 行, 翻译: {})",
                lyrics.lines.len(),
                if lyrics.has_translation { "有" } else { "无" }
            ),
            Err(err) => println!("失败（{err}）"),
        }
    }

    if let Some(chart) = charts.first() {
        print!("歌单「{}」… ", chart.name);
        match client.playlist_detail(chart.id).await {
            Ok(detail) => println!("ok ({} 首)", detail.tracks.len()),
            Err(err) => println!("失败（{err}）"),
        }
    }

    println!("自检完成");
    Ok(())
}
