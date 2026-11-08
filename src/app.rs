//! Application state and event handling.

use crate::audio::{AudioCmd, AudioEvent, AudioHandle, AudioStatus, PlaybackSnapshot};
use crate::config::{self, quality_label, Config};
use crate::keymap::{Action, KeyChord, KeyMatch, Keymap};
use crate::netease::lrc::Lyrics;
use crate::netease::model::{Playlist, QrState, Track, User};
use crate::netease::NeteaseClient;
use crate::ui::theme::{theme_at, theme_by_name, theme_index, Theme, THEME_NAMES};
use crate::ui::{HitAction, HitRegion};
use crate::worker::{self, Bootstrap, Msg};
use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use qrcode::{render::unicode::Dense1x2, EcLevel, QrCode};
use rand::Rng;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Search,
    Library,
    Queue,
    Detail,
    Lyrics,
}

impl Screen {
    pub const NAV: [Screen; 4] = [Screen::Home, Screen::Search, Screen::Library, Screen::Queue];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Home => "发现",
            Screen::Search => "搜索",
            Screen::Library => "歌单",
            Screen::Queue => "队列",
            Screen::Detail => "歌单详情",
            Screen::Lyrics => "歌词",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Screen::Home => "♪",
            Screen::Search => "/",
            Screen::Library => "♫",
            Screen::Queue => "≡",
            Screen::Detail | Screen::Lyrics => "·",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeTab {
    ForYou,
    Charts,
    Mixes,
}

impl HomeTab {
    pub const ALL: [HomeTab; 3] = [HomeTab::ForYou, HomeTab::Charts, HomeTab::Mixes];

    pub fn title(self) -> &'static str {
        match self {
            HomeTab::ForYou => "为你推荐",
            HomeTab::Charts => "排行榜",
            HomeTab::Mixes => "推荐歌单",
        }
    }

    pub fn next(self) -> Self {
        match self {
            HomeTab::ForYou => HomeTab::Charts,
            HomeTab::Charts => HomeTab::Mixes,
            HomeTab::Mixes => HomeTab::ForYou,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            HomeTab::ForYou => HomeTab::Mixes,
            HomeTab::Charts => HomeTab::ForYou,
            HomeTab::Mixes => HomeTab::Charts,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Off,
    All,
    One,
}

impl Repeat {
    pub fn next(self) -> Self {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Repeat::Off => "不循环",
            Repeat::All => "列表循环",
            Repeat::One => "单曲循环",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Repeat::Off => "→",
            Repeat::All => "↻",
            Repeat::One => "↻¹",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub at: Instant,
}

#[derive(Debug, Default)]
pub struct HomeState {
    pub toplists: Vec<Playlist>,
    pub recommended: Vec<Playlist>,
    pub daily: Vec<Track>,
    pub selected: usize,
    pub loading: bool,
}

#[derive(Debug, Default)]
pub struct SearchState {
    pub input: String,
    pub results: Vec<Track>,
    pub total: u64,
    pub selected: usize,
    pub loading: bool,
    pub seq: u64,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct LibraryState {
    pub playlists: Vec<Playlist>,
    pub selected: usize,
}

#[derive(Debug)]
pub struct DetailState {
    pub id: u64,
    pub playlist: Playlist,
    pub tracks: Vec<Track>,
    pub selected: usize,
    pub loading: bool,
    pub from: Screen,
}

impl DetailState {
    fn placeholder(playlist: Playlist, from: Screen) -> Self {
        Self {
            id: playlist.id,
            playlist,
            tracks: Vec::new(),
            selected: 0,
            loading: true,
            from,
        }
    }
}

#[derive(Debug, Default)]
pub struct QueueState {
    pub tracks: Vec<Track>,
    pub current: Option<usize>,
    pub selected: usize,
    pub history: Vec<usize>,
}

impl QueueState {
    pub fn current_track(&self) -> Option<&Track> {
        self.current.and_then(|index| self.tracks.get(index))
    }

    pub fn set(&mut self, tracks: Vec<Track>, index: usize) {
        self.history.clear();
        self.tracks = tracks;
        self.current = if self.tracks.is_empty() {
            None
        } else {
            Some(index.min(self.tracks.len() - 1))
        };
        self.selected = self.current.unwrap_or(0);
    }

    pub fn add(&mut self, track: Track) {
        self.tracks.push(track);
        if self.current.is_none() {
            self.selected = self.tracks.len().saturating_sub(1);
        }
    }

    pub fn add_many(&mut self, tracks: impl IntoIterator<Item = Track>) -> usize {
        let before = self.tracks.len();
        self.tracks.extend(tracks);
        self.tracks.len() - before
    }

    pub fn remove(&mut self, index: usize) -> Option<Track> {
        if index >= self.tracks.len() {
            return None;
        }
        let removed = self.tracks.remove(index);
        match self.current {
            Some(current) if current == index => {
                self.current = if self.tracks.is_empty() {
                    None
                } else {
                    Some(index.min(self.tracks.len() - 1))
                };
            }
            Some(current) if current > index => self.current = Some(current - 1),
            _ => {}
        }
        self.selected = self.selected.min(self.tracks.len().saturating_sub(1));
        Some(removed)
    }

    pub fn next_index(&mut self, shuffle: bool) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        let current = self.current.unwrap_or(0);
        if shuffle && self.tracks.len() > 1 {
            let mut rng = rand::thread_rng();
            let mut next = rng.gen_range(0..self.tracks.len());
            if next == current {
                next = (next + 1) % self.tracks.len();
            }
            self.history.push(current);
            Some(next)
        } else if current + 1 < self.tracks.len() {
            Some(current + 1)
        } else {
            None
        }
    }

    pub fn previous_index(&mut self) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        if let Some(previous) = self.history.pop() {
            return Some(previous);
        }
        let current = self.current.unwrap_or(0);
        if current > 0 {
            Some(current - 1)
        } else {
            None
        }
    }
}

#[derive(Debug, Default)]
pub struct LyricState {
    pub lyrics: Lyrics,
    pub active: Option<usize>,
    pub loading: bool,
    pub track_id: Option<u64>,
    pub offset: i32,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct PlayerState {
    pub track: Option<Track>,
    pub loading: bool,
    pub trial: bool,
    pub quality: String,
    pub format: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginTab {
    Qr,
    Phone,
}

#[derive(Debug)]
pub struct LoginState {
    pub seq: u64,
    pub tab: LoginTab,
    pub qr_url: Option<String>,
    pub qr_key: Option<String>,
    pub qr_lines: Vec<String>,
    pub qr_status: String,
    pub qr_loading: bool,
    pub qr_polling: bool,
    pub qr_expired: bool,
    pub phone: String,
    pub password: String,
    pub field: usize,
    pub submitting: bool,
    pub error: Option<String>,
    pub last_poll: Instant,
}

impl LoginState {
    fn new(seq: u64) -> Self {
        Self {
            seq,
            tab: LoginTab::Qr,
            qr_url: None,
            qr_key: None,
            qr_lines: Vec::new(),
            qr_status: "正在获取二维码…".into(),
            qr_loading: true,
            qr_polling: false,
            qr_expired: false,
            phone: String::new(),
            password: String::new(),
            field: 0,
            submitting: false,
            error: None,
            last_poll: Instant::now(),
        }
    }
}

#[derive(Debug)]
pub struct ThemePicker {
    pub index: usize,
    pub original: Theme,
}

#[derive(Debug, Clone, Copy)]
pub struct Splash {
    pub started: Instant,
    pub duration: Duration,
}

impl Splash {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            duration: Duration::from_millis(2800),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed().min(self.duration)
    }

    pub fn finished(&self) -> bool {
        self.started.elapsed() >= self.duration
    }
}

pub struct App {
    pub cfg: Config,
    pub theme: Theme,
    pub client: Arc<NeteaseClient>,
    pub audio: AudioHandle,
    pub net_tx: UnboundedSender<Msg>,
    pub cache_dir: PathBuf,

    pub screen: Screen,
    pub home_tab: HomeTab,
    pub focus: Focus,
    pub home: HomeState,
    pub search: SearchState,
    pub library: LibraryState,
    pub detail: Option<DetailState>,
    pub queue: QueueState,
    pub lyrics: LyricState,
    pub player: PlayerState,
    pub snapshot: PlaybackSnapshot,

    pub liked: HashSet<u64>,
    pub user: Option<User>,
    pub repeat: Repeat,
    pub shuffle: bool,
    pub volume_before_mute: u8,

    pub login: Option<LoginState>,
    pub theme_picker: Option<ThemePicker>,
    pub help: bool,
    pub toast: Option<Toast>,
    pub hits: Vec<HitRegion>,
    pub keymap: Keymap,
    pub key_seq: Vec<KeyChord>,
    pub key_seq_at: Option<Instant>,
    pub splash: Option<Splash>,
    pub sidebar_visible: bool,

    pub should_quit: bool,
    pub bootstrapping: bool,
    pub tick: u64,
    pub started: Instant,
    pub lyrics_from: Screen,

    search_seq: u64,
    detail_seq: u64,
    play_seq: u64,
    login_seq: u64,
    bootstrap_seq: u64,
}

impl App {
    pub fn new(
        theme_override: Option<String>,
        no_audio: bool,
        no_splash: bool,
        net_tx: UnboundedSender<Msg>,
    ) -> Result<Self> {
        let mut cfg = Config::load();
        if let Some(theme) = theme_override {
            cfg.theme = theme;
        }
        let theme = theme_by_name(&cfg.theme);
        cfg.theme = theme.name.to_string();
        let client =
            Arc::new(NeteaseClient::new(config::cookie_path()).context("初始化网易云客户端失败")?);
        let volume = cfg.volume.min(100) as f32 / 100.0;
        let audio = AudioHandle::spawn(no_audio, volume);
        let snapshot = audio.snapshot();
        let quality = config::normalize_quality(&cfg.quality)
            .unwrap_or(config::DEFAULT_QUALITY)
            .to_string();
        cfg.quality = quality.clone();
        let sidebar_visible = cfg.sidebar;
        let (keymap, key_warnings) = Keymap::from_config(&cfg.keys);
        let has_key_warnings = !key_warnings.is_empty();
        let startup_toast = if has_key_warnings {
            format!("按键配置: {}", key_warnings.join("；"))
        } else {
            "正在连接网易云音乐…".to_string()
        };
        let cache_dir = config::audio_cache_dir();
        let app = Self {
            cfg,
            theme,
            client: client.clone(),
            audio,
            net_tx: net_tx.clone(),
            cache_dir,
            screen: Screen::Home,
            home_tab: HomeTab::ForYou,
            focus: Focus::List,
            home: HomeState {
                loading: true,
                ..Default::default()
            },
            search: SearchState::default(),
            library: LibraryState::default(),
            detail: None,
            queue: QueueState::default(),
            lyrics: LyricState::default(),
            player: PlayerState {
                quality,
                ..Default::default()
            },
            snapshot,
            liked: HashSet::new(),
            user: None,
            repeat: Repeat::Off,
            shuffle: false,
            volume_before_mute: 70,
            login: None,
            theme_picker: None,
            help: false,
            toast: Some(Toast {
                text: startup_toast,
                kind: if has_key_warnings {
                    ToastKind::Error
                } else {
                    ToastKind::Info
                },
                at: Instant::now(),
            }),
            hits: Vec::new(),
            keymap,
            key_seq: Vec::new(),
            key_seq_at: None,
            splash: if no_splash { None } else { Some(Splash::new()) },
            sidebar_visible,
            should_quit: false,
            bootstrapping: true,
            tick: 0,
            started: Instant::now(),
            lyrics_from: Screen::Home,
            search_seq: 0,
            detail_seq: 0,
            play_seq: 0,
            login_seq: 0,
            bootstrap_seq: 1,
        };
        worker::spawn_bootstrap(app.net_tx.clone(), client, app.bootstrap_seq);
        Ok(app)
    }

    // ------------------------------------------------------------------
    // Input
    // ------------------------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        if self.splash.is_some() {
            self.splash = None;
            return;
        }
        if self.help {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
            ) {
                self.help = false;
            }
            return;
        }
        if self.theme_picker.is_some() {
            self.on_theme_key(key);
            return;
        }
        if self.login.is_some() {
            self.on_login_key(key);
            return;
        }
        if self.screen == Screen::Search && self.focus == Focus::Input {
            self.on_search_input(key);
            return;
        }
        self.on_normal_key(key);
    }

    fn on_search_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.focus = Focus::List,
            KeyCode::Enter => {
                self.focus = Focus::List;
                self.run_search();
            }
            KeyCode::Backspace => {
                self.search.input.pop();
            }
            KeyCode::Delete => self.search.input.clear(),
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.input.clear();
            }
            KeyCode::Tab => self.focus = Focus::List,
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && self.search.input.chars().count() < 80 =>
            {
                self.search.input.push(c);
            }
            _ => {}
        }
    }

    fn on_normal_key(&mut self, key: KeyEvent) {
        let now = Instant::now();
        if let Some(started) = self.key_seq_at {
            if now.duration_since(started) > Duration::from_millis(700) {
                self.key_seq.clear();
                self.key_seq_at = None;
            }
        }
        let chord = KeyChord::from_event(&key);
        match self.keymap.resolve(&self.key_seq, chord.clone()) {
            KeyMatch::Action(action) => {
                self.key_seq.clear();
                self.key_seq_at = None;
                self.perform(action);
            }
            KeyMatch::Pending => {
                self.key_seq.push(chord);
                self.key_seq_at = Some(now);
            }
            KeyMatch::None => {
                self.key_seq.clear();
                self.key_seq_at = None;
            }
        }
    }

    fn perform(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Help => self.help = true,
            Action::Theme => self.open_theme_picker(),
            Action::Login => {
                if let Some(user) = &self.user {
                    self.toast(
                        format!("已登录：{}（Ctrl+L 退出登录）", user.nickname),
                        ToastKind::Info,
                    );
                } else {
                    self.open_login();
                }
            }
            Action::Logout => self.logout(),
            Action::PlayPause => self.toggle_pause(),
            Action::Next => self.next_track(false),
            Action::Prev => self.previous_track(),
            Action::NavPrev => self.cycle_nav(false),
            Action::NavNext => self.cycle_nav(true),
            Action::Sidebar => self.toggle_sidebar(),
            Action::Queue => self.go_to(Screen::Queue),
            Action::Lyrics => {
                if self.screen == Screen::Lyrics {
                    self.go_back();
                } else {
                    self.open_lyrics();
                }
            }
            Action::Search => self.open_search(),
            Action::Shuffle => {
                self.shuffle = !self.shuffle;
                if self.shuffle {
                    self.queue.history.clear();
                }
                self.toast(
                    if self.shuffle {
                        "随机播放：开"
                    } else {
                        "随机播放：关"
                    },
                    ToastKind::Info,
                );
            }
            Action::Repeat => {
                self.repeat = self.repeat.next();
                self.toast(
                    format!("播放模式：{}", self.repeat.label()),
                    ToastKind::Info,
                );
            }
            Action::Mute => self.toggle_mute(),
            Action::Like => self.toggle_like(),
            Action::Enqueue => self.enqueue_selected(false),
            Action::EnqueueAll => self.enqueue_selected(true),
            Action::Refresh => self.refresh_current(),
            Action::MoveUp => self.move_selection(-1),
            Action::MoveDown => self.move_selection(1),
            Action::PageUp | Action::HalfPageUp => self.move_selection(-10),
            Action::PageDown | Action::HalfPageDown => self.move_selection(10),
            Action::FastPageUp => self.move_selection(-20),
            Action::FastPageDown => self.move_selection(20),
            Action::Top => self.select_first(),
            Action::Bottom => self.select_last(),
            Action::PrevTab => self.switch_home_tab(false),
            Action::NextTab => self.switch_home_tab(true),
            Action::ScreenHome => self.go_to(Screen::Home),
            Action::ScreenSearch => self.go_to(Screen::Search),
            Action::ScreenLibrary => self.go_to(Screen::Library),
            Action::ScreenQueue => self.go_to(Screen::Queue),
            Action::Activate => self.activate(),
            Action::Back => self.go_back(),
            Action::SeekBack => self.seek_by(-5),
            Action::SeekForward => self.seek_by(5),
            Action::RemoveQueue => {
                if self.screen == Screen::Queue {
                    self.remove_selected_from_queue();
                }
            }
            Action::VolumeUp => self.adjust_volume(5),
            Action::VolumeDown => self.adjust_volume(-5),
        }
    }

    // Mouse
    // ------------------------------------------------------------------

    pub fn set_hits(&mut self, hits: Vec<HitRegion>) {
        self.hits = hits;
    }

    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        self.key_seq.clear();
        self.key_seq_at = None;
        if self.splash.is_some() {
            self.splash = None;
            return;
        }
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.on_click(mouse.column, mouse.row),
            MouseEventKind::ScrollDown => self.on_scroll(mouse.column, mouse.row, 3),
            MouseEventKind::ScrollUp => self.on_scroll(mouse.column, mouse.row, -3),
            _ => {}
        }
    }

    fn hit_at(&self, x: u16, y: u16) -> Option<HitRegion> {
        self.hits.iter().rev().copied().find(|hit| {
            let rect = hit.rect;
            x >= rect.x
                && x < rect.x.saturating_add(rect.width)
                && y >= rect.y
                && y < rect.y.saturating_add(rect.height)
        })
    }

    fn on_click(&mut self, x: u16, y: u16) {
        let Some(hit) = self.hit_at(x, y) else {
            return;
        };
        match hit.action {
            HitAction::Noop | HitAction::PlayerArea => {}
            HitAction::ToggleSidebar => self.toggle_sidebar(),
            HitAction::CloseOverlay => self.close_overlay(),
            HitAction::Nav(screen) => {
                if screen == Screen::Search {
                    self.screen = Screen::Search;
                    self.focus = Focus::Input;
                } else {
                    self.go_to(screen);
                }
            }
            HitAction::HomeTab(tab) => {
                self.screen = Screen::Home;
                self.home_tab = tab;
                self.home.selected = 0;
                self.focus = Focus::List;
            }
            HitAction::SelectRow(index) => {
                self.set_selection(index);
                self.activate();
            }
            HitAction::FocusSearch => self.open_search(),
            HitAction::PlayPause => self.toggle_pause(),
            HitAction::Seek => {
                if self.snapshot.duration.is_zero() {
                    return;
                }
                let ratio = ((x.saturating_sub(hit.rect.x)) as f64 / hit.rect.width.max(1) as f64)
                    .clamp(0.0, 1.0);
                self.audio
                    .send(AudioCmd::Seek(self.snapshot.duration.mul_f64(ratio)));
            }
            HitAction::Mute => self.toggle_mute(),
            HitAction::Like => self.toggle_like(),
            HitAction::OpenLyrics => self.open_lyrics(),
            HitAction::LyricsArea => {
                if self.screen != Screen::Lyrics {
                    self.open_lyrics();
                }
            }
            HitAction::ThemeRow(index) => self.apply_theme_row(index),
            HitAction::LoginTab(tab) => {
                if let Some(mut login) = self.login.take() {
                    login.tab = tab;
                    login.error = None;
                    self.login = Some(login);
                }
            }
            HitAction::LoginField(field) => {
                if let Some(login) = self.login.as_mut() {
                    login.field = field.min(1);
                }
            }
            HitAction::QrRefresh => {
                if let Some(mut login) = self.login.take() {
                    if login.tab == LoginTab::Qr {
                        self.start_qr(&mut login);
                    }
                    self.login = Some(login);
                }
            }
            HitAction::LoginSubmit => {
                if let Some(mut login) = self.login.take() {
                    if login.tab == LoginTab::Phone {
                        self.submit_phone(&mut login);
                    }
                    self.login = Some(login);
                }
            }
        }
    }

    fn on_scroll(&mut self, x: u16, y: u16, delta: i32) {
        if self.help || self.theme_picker.is_some() || self.login.is_some() {
            return;
        }
        let over = |action: HitAction| {
            self.hits.iter().rev().any(|hit| {
                hit.action == action
                    && x >= hit.rect.x
                    && x < hit.rect.x.saturating_add(hit.rect.width)
                    && y >= hit.rect.y
                    && y < hit.rect.y.saturating_add(hit.rect.height)
            })
        };
        if over(HitAction::PlayerArea) {
            self.adjust_volume(if delta > 0 { 2 } else { -2 });
            return;
        }
        if self.screen == Screen::Lyrics || over(HitAction::LyricsArea) {
            let len = self.lyrics.lyrics.lines.len() as i32;
            self.lyrics.offset = (self.lyrics.offset + delta).clamp(-len, len);
            return;
        }
        self.move_selection(delta as i64);
    }

    fn close_overlay(&mut self) {
        if self.help {
            self.help = false;
        } else if let Some(picker) = self.theme_picker.take() {
            self.theme = picker.original;
            self.toast("已取消主题切换", ToastKind::Info);
        } else if self.login.is_some() {
            self.login = None;
        }
    }

    fn apply_theme_row(&mut self, index: usize) {
        let theme = theme_at(index);
        self.theme = theme;
        self.cfg.theme = theme.name.to_string();
        let _ = self.cfg.save();
        self.theme_picker = None;
        self.toast(format!("主题已切换：{}", theme.label), ToastKind::Success);
    }

    fn on_theme_key(&mut self, key: KeyEvent) {
        let Some(mut picker) = self.theme_picker.take() else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.theme = picker.original;
                self.toast("已取消主题切换", ToastKind::Info);
            }
            KeyCode::Enter => {
                self.cfg.theme = self.theme.name.to_string();
                if let Err(err) = self.cfg.save() {
                    self.toast(format!("保存配置失败: {err}"), ToastKind::Error);
                } else {
                    self.toast(
                        format!("主题已切换：{}", self.theme.label),
                        ToastKind::Success,
                    );
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                picker.index = (picker.index + 1) % THEME_NAMES.len();
                self.theme = theme_at(picker.index);
                self.theme_picker = Some(picker);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                picker.index = (picker.index + THEME_NAMES.len() - 1) % THEME_NAMES.len();
                self.theme = theme_at(picker.index);
                self.theme_picker = Some(picker);
            }
            KeyCode::Char('t') | KeyCode::Char('q') => {
                self.theme = picker.original;
            }
            _ => self.theme_picker = Some(picker),
        }
    }

    fn on_login_key(&mut self, key: KeyEvent) {
        let Some(mut login) = self.login.take() else {
            return;
        };
        let mut closed = false;

        if key.code == KeyCode::Esc {
            closed = true;
        } else if key.code == KeyCode::Tab {
            if login.tab == LoginTab::Qr {
                login.tab = LoginTab::Phone;
            } else {
                login.field = (login.field + 1) % 2;
            }
            login.error = None;
        } else if key.code == KeyCode::BackTab {
            if login.tab == LoginTab::Qr {
                login.tab = LoginTab::Phone;
            } else if login.field == 1 {
                login.field = 0;
            } else {
                login.tab = LoginTab::Qr;
            }
            login.error = None;
        } else {
            match login.tab {
                LoginTab::Qr => match key.code {
                    KeyCode::Char('r') | KeyCode::Enter => {
                        self.start_qr(&mut login);
                    }
                    _ => {}
                },
                LoginTab::Phone => match key.code {
                    KeyCode::Char(c)
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        if login.field == 0 {
                            if c.is_ascii_digit() && login.phone.len() < 11 {
                                login.phone.push(c);
                            }
                        } else if login.password.len() < 64 {
                            login.password.push(c);
                        }
                    }
                    KeyCode::Backspace => {
                        if login.field == 0 {
                            login.phone.pop();
                        } else {
                            login.password.pop();
                        }
                    }
                    KeyCode::Enter => {
                        self.submit_phone(&mut login);
                    }
                    _ => {}
                },
            }
        }

        if !closed {
            self.login = Some(login);
        }
    }

    fn start_qr(&mut self, login: &mut LoginState) {
        self.login_seq += 1;
        login.seq = self.login_seq;
        login.qr_url = None;
        login.qr_key = None;
        login.qr_lines.clear();
        login.qr_loading = true;
        login.qr_polling = false;
        login.qr_expired = false;
        login.qr_status = "正在获取二维码…".into();
        login.error = None;
        login.last_poll = Instant::now();
        worker::spawn_qr_create(self.net_tx.clone(), self.client.clone(), login.seq);
    }

    fn submit_phone(&mut self, login: &mut LoginState) {
        if login.phone.len() < 6 {
            login.error = Some("请输入手机号".into());
            return;
        }
        if login.password.is_empty() {
            login.error = Some("请输入密码".into());
            return;
        }
        login.submitting = true;
        login.error = None;
        worker::spawn_login_phone(
            self.net_tx.clone(),
            self.client.clone(),
            login.seq,
            login.phone.clone(),
            login.password.clone(),
        );
    }

    fn open_login(&mut self) {
        let mut state = LoginState::new(0);
        self.start_qr(&mut state);
        self.login = Some(state);
    }

    pub fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        if self.splash.as_ref().is_some_and(Splash::finished) {
            self.splash = None;
        }
        self.snapshot = self.audio.snapshot();
        while let Ok(event) = self.audio.events_mut().try_recv() {
            self.on_audio_event(event);
        }
        if let Some(toast) = &self.toast {
            if toast.at.elapsed() > Duration::from_secs(7) {
                self.toast = None;
            }
        }
        if self.lyrics.track_id.is_some() {
            self.lyrics.active = self.lyrics.lyrics.active_index(self.snapshot.position);
        }
        self.poll_qr();
    }

    fn poll_qr(&mut self) {
        let now = Instant::now();
        let Some(login) = self.login.as_ref() else {
            return;
        };
        if login.tab != LoginTab::Qr
            || login.qr_polling
            || login.qr_expired
            || login.qr_key.is_none()
        {
            return;
        }
        if now.duration_since(login.last_poll) < Duration::from_millis(1800) {
            return;
        }
        let seq = login.seq;
        let key = login.qr_key.clone().unwrap();
        if let Some(login) = self.login.as_mut() {
            login.qr_polling = true;
            login.last_poll = now;
        }
        worker::spawn_qr_check(self.net_tx.clone(), self.client.clone(), seq, key);
    }

    fn on_audio_event(&mut self, event: AudioEvent) {
        match event {
            AudioEvent::Backend(message) => self.toast(message, ToastKind::Info),
            AudioEvent::Error(message) => {
                self.player.loading = false;
                self.player.error = Some(message.clone());
                self.toast(message, ToastKind::Error);
            }
            AudioEvent::Ended => {
                if self.player.loading {
                    return;
                }
                if self.repeat == Repeat::One {
                    if let Some(index) = self.queue.current {
                        self.play_index(index);
                    }
                } else {
                    self.next_track(true);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Network messages
    // ------------------------------------------------------------------

    pub fn on_msg(&mut self, msg: Msg) {
        match msg {
            Msg::Bootstrap { seq, result } => {
                if seq == self.bootstrap_seq {
                    self.on_bootstrap(result);
                }
            }
            Msg::Search { seq, result } => {
                if seq != self.search.seq {
                    return;
                }
                self.search.loading = false;
                match result {
                    Ok(page) => {
                        self.search.total = page.total;
                        self.search.results = page.tracks;
                        self.search.selected = 0;
                        self.search.error = None;
                        if self.search.results.is_empty() {
                            self.toast("没有找到歌曲", ToastKind::Info);
                        }
                    }
                    Err(err) => {
                        self.search.error = Some(err.clone());
                        self.toast(format!("搜索失败: {err}"), ToastKind::Error);
                    }
                }
            }
            Msg::Playlist { seq, id, result } => {
                if seq != self.detail_seq {
                    return;
                }
                if let Some(detail) = self.detail.as_mut() {
                    if detail.id != id {
                        return;
                    }
                    detail.loading = false;
                    match result {
                        Ok(mut loaded) => {
                            loaded.playlist.name = if loaded.playlist.name.is_empty() {
                                detail.playlist.name.clone()
                            } else {
                                loaded.playlist.name
                            };
                            detail.playlist = loaded.playlist;
                            detail.tracks = loaded.tracks;
                            detail.selected = 0;
                            if detail.tracks.is_empty() {
                                self.toast("这个歌单是空的", ToastKind::Info);
                            }
                        }
                        Err(err) => {
                            self.toast(format!("歌单加载失败: {err}"), ToastKind::Error);
                        }
                    }
                }
            }
            Msg::PlayReady {
                seq,
                track,
                path,
                url,
            } => {
                if seq != self.play_seq {
                    return;
                }
                self.player.loading = false;
                self.player.trial = url.trial;
                self.player.format = url.kind.clone();
                self.player.quality = url.level.clone();
                if self.player.track.as_ref().map(|t| t.id) != Some(track.id) {
                    return;
                }
                if url.trial {
                    self.toast("该歌曲仅提供试听片段", ToastKind::Info);
                }
                self.audio.send(AudioCmd::Load {
                    path,
                    duration: track.duration,
                });
            }
            Msg::PlayFailed { seq, track, error } => {
                if seq != self.play_seq {
                    return;
                }
                self.player.loading = false;
                self.player.error = Some(error.clone());
                self.audio.send(AudioCmd::Stop);
                self.toast(
                    format!("{} 播放失败：{error}", track.name),
                    ToastKind::Error,
                );
            }
            Msg::Lyric { track_id, result } => {
                if self.player.track.as_ref().map(|t| t.id) != Some(track_id) {
                    return;
                }
                self.lyrics.loading = false;
                match result {
                    Ok(lyrics) => {
                        let empty = lyrics.is_empty();
                        self.lyrics.lyrics = lyrics;
                        self.lyrics.track_id = Some(track_id);
                        self.lyrics.active = None;
                        self.lyrics.offset = 0;
                        self.lyrics.error = None;
                        if empty {
                            self.lyrics.error = Some("暂无歌词".into());
                        }
                    }
                    Err(err) => {
                        self.lyrics.error = Some(err);
                    }
                }
            }
            Msg::QrCreated { seq, result } => self.on_qr_created(seq, result),
            Msg::QrStatus { seq, key, result } => self.on_qr_status(seq, &key, result),
            Msg::LoginDone { seq, result } => self.on_login_done(seq, result),
            Msg::LogoutDone(result) => match result {
                Ok(()) => {
                    self.user = None;
                    self.liked.clear();
                    self.home.daily.clear();
                    self.library.playlists.clear();
                    self.screen = Screen::Home;
                    self.toast("已退出登录", ToastKind::Success);
                    self.refresh_bootstrap();
                }
                Err(err) => self.toast(format!("退出登录失败: {err}"), ToastKind::Error),
            },
            Msg::LikeDone { id, like, result } => {
                if let Err(err) = result {
                    if like {
                        self.liked.remove(&id);
                    } else {
                        self.liked.insert(id);
                    }
                    self.toast(format!("操作失败: {err}"), ToastKind::Error);
                }
            }
        }
    }

    fn on_bootstrap(&mut self, result: Result<Bootstrap, String>) {
        self.bootstrapping = false;
        self.home.loading = false;
        match result {
            Ok(data) => {
                if self
                    .toast
                    .as_ref()
                    .is_some_and(|toast| toast.text.contains("正在连接网易云"))
                {
                    self.toast = None;
                }
                let logged_in = data.user.is_some();
                self.user = data.user;
                self.home.toplists = data.toplists;
                self.home.recommended = data.recommended;
                self.home.daily = data.daily;
                self.library.playlists = data.playlists;
                self.liked = data.liked;
                if !logged_in {
                    self.home_tab = if self.home_tab == HomeTab::ForYou {
                        HomeTab::Charts
                    } else {
                        self.home_tab
                    };
                }
                if let Some(warning) = data.warnings.first() {
                    self.toast(warning.clone(), ToastKind::Error);
                }
            }
            Err(err) => {
                self.toast(format!("连接网易云失败: {err}"), ToastKind::Error);
            }
        }
    }

    fn on_qr_created(&mut self, seq: u64, result: Result<(String, String), String>) {
        let Some(login) = self.login.as_mut() else {
            return;
        };
        if login.seq != seq {
            return;
        }
        login.qr_loading = false;
        match result {
            Ok((url, key)) => {
                login.qr_lines = make_qr_lines(&url);
                login.qr_url = Some(url);
                login.qr_key = Some(key);
                login.qr_status = "打开网易云音乐 App，扫码登录".into();
                login.qr_expired = false;
                login.last_poll = Instant::now();
            }
            Err(err) => {
                login.qr_status = "二维码获取失败".into();
                login.error = Some(err);
            }
        }
    }

    fn on_qr_status(&mut self, seq: u64, key: &str, result: Result<(QrState, String), String>) {
        let mut authorized = false;
        if let Some(login) = self.login.as_mut() {
            if login.seq != seq || login.qr_key.as_deref() != Some(key) {
                return;
            }
            login.qr_polling = false;
            match result {
                Ok((state, _message)) => match state {
                    QrState::Waiting => {
                        login.qr_status = "等待扫码…".into();
                    }
                    QrState::Scanned => {
                        login.qr_status = "已扫码，请在手机上确认".into();
                    }
                    QrState::Expired => {
                        login.qr_expired = true;
                        login.qr_status = "二维码已过期，按 r 刷新".into();
                    }
                    QrState::Authorized => authorized = true,
                },
                Err(err) => {
                    login.qr_status = format!("扫码状态查询失败：{err}");
                    login.last_poll = Instant::now();
                }
            }
        }
        if authorized {
            self.finish_login();
        }
    }

    fn finish_login(&mut self) {
        self.login = None;
        self.toast("登录成功，正在同步资料…", ToastKind::Success);
        self.refresh_bootstrap();
    }

    fn refresh_bootstrap(&mut self) {
        self.bootstrap_seq += 1;
        self.bootstrapping = true;
        self.home.loading = true;
        worker::spawn_bootstrap(self.net_tx.clone(), self.client.clone(), self.bootstrap_seq);
    }

    fn on_login_done(&mut self, seq: u64, result: Result<(), String>) {
        let mut success = false;
        if let Some(login) = self.login.as_mut() {
            if login.seq != seq {
                return;
            }
            login.submitting = false;
            match result {
                Ok(()) => success = true,
                Err(err) => login.error = Some(err),
            }
        }
        if success {
            self.finish_login();
        }
    }

    // ------------------------------------------------------------------
    // Selection / navigation
    // ------------------------------------------------------------------

    fn go_to(&mut self, screen: Screen) {
        self.screen = screen;
        self.focus = Focus::List;
        if screen == Screen::Queue {
            self.queue.selected = self.queue.current.unwrap_or(0);
        }
    }

    fn cycle_nav(&mut self, forward: bool) {
        let current = match self.screen {
            Screen::Detail => self.detail.as_ref().map(|d| d.from).unwrap_or(Screen::Home),
            Screen::Lyrics => self.lyrics_from,
            other => other,
        };
        let index = Screen::NAV
            .iter()
            .position(|screen| *screen == current)
            .unwrap_or(0);
        let next = if forward {
            (index + 1) % Screen::NAV.len()
        } else {
            (index + Screen::NAV.len() - 1) % Screen::NAV.len()
        };
        self.go_to(Screen::NAV[next]);
    }

    fn go_back(&mut self) {
        match self.screen {
            Screen::Detail => {
                if let Some(detail) = &self.detail {
                    self.screen = detail.from;
                } else {
                    self.screen = Screen::Home;
                }
            }
            Screen::Lyrics => self.screen = self.lyrics_from,
            Screen::Search => {
                if self.focus == Focus::Input {
                    self.focus = Focus::List;
                } else {
                    self.screen = Screen::Home;
                }
            }
            Screen::Library | Screen::Queue => self.screen = Screen::Home,
            Screen::Home => {}
        }
    }

    fn open_search(&mut self) {
        self.screen = Screen::Search;
        self.focus = Focus::Input;
    }

    fn open_lyrics(&mut self) {
        if self.player.track.is_none() {
            self.toast("还没有正在播放的歌曲", ToastKind::Info);
            return;
        }
        self.lyrics_from = self.screen;
        self.screen = Screen::Lyrics;
        self.focus = Focus::List;
    }

    fn switch_home_tab(&mut self, forward: bool) {
        if self.screen != Screen::Home {
            return;
        }
        self.home_tab = if forward {
            self.home_tab.next()
        } else {
            self.home_tab.prev()
        };
        self.home.selected = 0;
    }

    fn current_len(&self) -> usize {
        match self.screen {
            Screen::Home => match self.home_tab {
                HomeTab::ForYou => self.home.daily.len(),
                HomeTab::Charts => self.home.toplists.len(),
                HomeTab::Mixes => self.home.recommended.len(),
            },
            Screen::Search => self.search.results.len(),
            Screen::Library => self.library.playlists.len(),
            Screen::Detail => self
                .detail
                .as_ref()
                .map(|detail| detail.tracks.len())
                .unwrap_or(0),
            Screen::Queue => self.queue.tracks.len(),
            Screen::Lyrics => 0,
        }
    }

    fn current_selection(&self) -> usize {
        match self.screen {
            Screen::Home => self.home.selected,
            Screen::Search => self.search.selected,
            Screen::Library => self.library.selected,
            Screen::Detail => self
                .detail
                .as_ref()
                .map(|detail| detail.selected)
                .unwrap_or(0),
            Screen::Queue => self.queue.selected,
            Screen::Lyrics => 0,
        }
    }

    fn set_selection(&mut self, value: usize) {
        match self.screen {
            Screen::Home => self.home.selected = value,
            Screen::Search => self.search.selected = value,
            Screen::Library => self.library.selected = value,
            Screen::Detail => {
                if let Some(detail) = self.detail.as_mut() {
                    detail.selected = value;
                }
            }
            Screen::Queue => self.queue.selected = value,
            Screen::Lyrics => {}
        }
    }

    fn move_selection(&mut self, delta: i64) {
        if self.screen == Screen::Lyrics {
            self.lyrics.offset = (self.lyrics.offset + delta as i32).clamp(
                -(self.lyrics.lyrics.lines.len() as i32),
                self.lyrics.lyrics.lines.len() as i32,
            );
            return;
        }
        let len = self.current_len();
        if len == 0 {
            return;
        }
        let current = self.current_selection().min(len - 1);
        let next = (current as i64 + delta).clamp(0, len as i64 - 1);
        self.set_selection(next as usize);
    }

    fn select_first(&mut self) {
        if self.screen == Screen::Lyrics {
            self.lyrics.offset = 0;
        } else {
            self.set_selection(0);
        }
    }

    fn select_last(&mut self) {
        let len = self.current_len();
        if len > 0 {
            self.set_selection(len - 1);
        }
    }

    fn selected_track(&self) -> Option<Track> {
        match self.screen {
            Screen::Home => match self.home_tab {
                HomeTab::ForYou => self.home.daily.get(self.home.selected).cloned(),
                _ => None,
            },
            Screen::Search => self.search.results.get(self.search.selected).cloned(),
            Screen::Detail => self
                .detail
                .as_ref()
                .and_then(|detail| detail.tracks.get(detail.selected).cloned()),
            Screen::Queue => self.queue.tracks.get(self.queue.selected).cloned(),
            Screen::Library | Screen::Lyrics => None,
        }
    }

    fn visible_tracks(&self) -> Vec<Track> {
        match self.screen {
            Screen::Home if self.home_tab == HomeTab::ForYou => self.home.daily.clone(),
            Screen::Search => self.search.results.clone(),
            Screen::Detail => self
                .detail
                .as_ref()
                .map(|detail| detail.tracks.clone())
                .unwrap_or_default(),
            Screen::Queue => self.queue.tracks.clone(),
            _ => Vec::new(),
        }
    }

    fn activate(&mut self) {
        match self.screen {
            Screen::Home => match self.home_tab {
                HomeTab::ForYou => {
                    if self.home.daily.is_empty() {
                        self.toast("登录后即可查看每日推荐", ToastKind::Info);
                    } else {
                        let index = self.home.selected.min(self.home.daily.len() - 1);
                        self.play_context(self.home.daily.clone(), index);
                    }
                }
                HomeTab::Charts => {
                    if let Some(playlist) = self.home.toplists.get(self.home.selected).cloned() {
                        self.open_detail(playlist, Screen::Home);
                    }
                }
                HomeTab::Mixes => {
                    if let Some(playlist) = self.home.recommended.get(self.home.selected).cloned() {
                        self.open_detail(playlist, Screen::Home);
                    }
                }
            },
            Screen::Search => {
                if !self.search.results.is_empty() {
                    let index = self.search.selected.min(self.search.results.len() - 1);
                    self.play_context(self.search.results.clone(), index);
                }
            }
            Screen::Library => {
                if let Some(playlist) = self.library.playlists.get(self.library.selected).cloned() {
                    self.open_detail(playlist, Screen::Library);
                }
            }
            Screen::Detail => {
                let Some(detail) = self.detail.as_ref() else {
                    return;
                };
                if detail.tracks.is_empty() {
                    return;
                }
                let index = detail.selected.min(detail.tracks.len() - 1);
                let tracks = detail.tracks.clone();
                self.play_context(tracks, index);
            }
            Screen::Queue => {
                if !self.queue.tracks.is_empty() {
                    let index = self.queue.selected.min(self.queue.tracks.len() - 1);
                    self.play_index(index);
                }
            }
            Screen::Lyrics => {}
        }
    }

    fn open_detail(&mut self, playlist: Playlist, from: Screen) {
        self.detail_seq += 1;
        let id = playlist.id;
        self.detail = Some(DetailState::placeholder(playlist, from));
        self.screen = Screen::Detail;
        self.focus = Focus::List;
        worker::spawn_playlist(
            self.net_tx.clone(),
            self.client.clone(),
            self.detail_seq,
            id,
        );
    }

    fn refresh_current(&mut self) {
        match self.screen {
            Screen::Home | Screen::Library => {
                self.refresh_bootstrap();
                self.toast("正在刷新…", ToastKind::Info);
            }
            Screen::Search => self.run_search(),
            Screen::Detail => {
                if let Some(detail) = self.detail.as_ref() {
                    let playlist = detail.playlist.clone();
                    let from = detail.from;
                    self.open_detail(playlist, from);
                }
            }
            _ => {}
        }
    }

    fn run_search(&mut self) {
        let keyword = self.search.input.trim().to_string();
        if keyword.is_empty() {
            self.toast("请输入搜索关键词", ToastKind::Info);
            return;
        }
        self.search_seq += 1;
        self.search.seq = self.search_seq;
        self.search.loading = true;
        self.search.error = None;
        self.search.selected = 0;
        worker::spawn_search(
            self.net_tx.clone(),
            self.client.clone(),
            self.search.seq,
            keyword,
        );
    }

    // ------------------------------------------------------------------
    // Queue / playback
    // ------------------------------------------------------------------

    fn play_context(&mut self, tracks: Vec<Track>, index: usize) {
        if tracks.is_empty() || index >= tracks.len() {
            return;
        }
        self.queue.set(tracks, index);
        self.play_index(index);
    }

    fn play_index(&mut self, index: usize) {
        let Some(track) = self.queue.tracks.get(index).cloned() else {
            return;
        };
        self.queue.current = Some(index);
        self.queue.selected = index;
        self.start_track(track);
    }

    fn start_track(&mut self, track: Track) {
        self.play_seq += 1;
        let seq = self.play_seq;
        self.player.track = Some(track.clone());
        self.player.loading = true;
        self.player.trial = false;
        self.player.error = None;
        self.lyrics = LyricState {
            loading: true,
            track_id: Some(track.id),
            ..Default::default()
        };
        self.snapshot.status = AudioStatus::Loading;
        self.snapshot.position = Duration::ZERO;
        self.audio.send(AudioCmd::Stop);
        worker::spawn_play(
            self.net_tx.clone(),
            self.client.clone(),
            seq,
            track.clone(),
            self.player.quality.clone(),
            self.cache_dir.clone(),
        );
        worker::spawn_lyric(self.net_tx.clone(), self.client.clone(), track.id);
    }

    fn toggle_pause(&mut self) {
        if self.player.track.is_none() {
            if let Some(index) = self.queue.current {
                self.play_index(index);
            } else {
                self.toast("还没有正在播放的歌曲", ToastKind::Info);
            }
            return;
        }
        if self.player.loading {
            self.toast("正在加载歌曲…", ToastKind::Info);
            return;
        }
        if self.snapshot.status == AudioStatus::Stopped {
            if let Some(index) = self.queue.current {
                self.play_index(index);
            }
            return;
        }
        self.audio.send(AudioCmd::Toggle);
    }

    fn next_track(&mut self, auto: bool) {
        if self.queue.tracks.is_empty() {
            if auto {
                self.audio.send(AudioCmd::Stop);
            } else {
                self.toast("播放队列为空", ToastKind::Info);
            }
            return;
        }
        if auto && self.repeat == Repeat::One {
            if let Some(index) = self.queue.current {
                self.play_index(index);
            }
            return;
        }
        if let Some(next) = self.queue.next_index(self.shuffle) {
            self.play_index(next);
        } else if self.repeat == Repeat::All {
            self.queue.history.clear();
            self.play_index(0);
        } else if auto {
            self.audio.send(AudioCmd::Stop);
            self.player.loading = false;
            self.toast("播放队列已结束", ToastKind::Info);
        } else {
            self.toast("已经是最后一首了", ToastKind::Info);
        }
    }

    fn previous_track(&mut self) {
        if self.queue.tracks.is_empty() {
            return;
        }
        if self.snapshot.position > Duration::from_secs(3) {
            self.audio.send(AudioCmd::Seek(Duration::ZERO));
            return;
        }
        if let Some(previous) = self.queue.previous_index() {
            self.play_index(previous);
        } else if self.repeat == Repeat::All && !self.queue.tracks.is_empty() {
            self.play_index(self.queue.tracks.len() - 1);
        } else {
            self.audio.send(AudioCmd::Seek(Duration::ZERO));
        }
    }

    fn seek_by(&mut self, seconds: i64) {
        if self.player.track.is_none() {
            return;
        }
        self.audio.send(AudioCmd::SeekBy(seconds));
    }

    fn adjust_volume(&mut self, delta: i32) {
        let next = (self.cfg.volume as i32 + delta).clamp(0, 100) as u8;
        self.cfg.volume = next;
        self.audio.send(AudioCmd::Volume(next as f32 / 100.0));
        self.toast(format!("音量 {next}%"), ToastKind::Info);
    }

    fn toggle_mute(&mut self) {
        if self.cfg.volume == 0 {
            self.cfg.volume = if self.volume_before_mute == 0 {
                60
            } else {
                self.volume_before_mute
            };
        } else {
            self.volume_before_mute = self.cfg.volume;
            self.cfg.volume = 0;
        }
        self.audio
            .send(AudioCmd::Volume(self.cfg.volume as f32 / 100.0));
        self.toast(format!("音量 {}%", self.cfg.volume), ToastKind::Info);
    }

    fn enqueue_selected(&mut self, all: bool) {
        if all {
            let tracks = self.visible_tracks();
            if tracks.is_empty() {
                self.toast("当前列表没有歌曲", ToastKind::Info);
                return;
            }
            let count = self.queue.add_many(tracks);
            self.toast(format!("已加入 {count} 首到队列"), ToastKind::Success);
        } else if let Some(track) = self.selected_track() {
            let name = track.name.clone();
            self.queue.add(track);
            self.toast(format!("已加入队列：{name}"), ToastKind::Success);
        } else {
            self.toast("请选择一首歌曲", ToastKind::Info);
        }
    }

    fn remove_selected_from_queue(&mut self) {
        if self.queue.tracks.is_empty() {
            return;
        }
        let index = self.queue.selected.min(self.queue.tracks.len() - 1);
        let was_current = self.queue.current == Some(index);
        if let Some(track) = self.queue.remove(index) {
            if was_current {
                self.audio.send(AudioCmd::Stop);
                self.player.track = None;
                self.player.loading = false;
                self.toast(format!("已移出并停止：{}", track.name), ToastKind::Info);
            } else {
                self.toast(format!("已移出队列：{}", track.name), ToastKind::Info);
            }
        }
    }

    fn toggle_like(&mut self) {
        let Some(user) = &self.user else {
            self.toast("登录后才能收藏歌曲", ToastKind::Info);
            return;
        };
        let _ = user;
        let Some(track) = self.player.track.clone().or_else(|| self.selected_track()) else {
            self.toast("请选择一首歌曲", ToastKind::Info);
            return;
        };
        let id = track.id;
        let like = !self.liked.contains(&id);
        if like {
            self.liked.insert(id);
        } else {
            self.liked.remove(&id);
        }
        worker::spawn_like(self.net_tx.clone(), self.client.clone(), id, like);
        self.toast(
            if like {
                format!("已喜欢：{}", track.name)
            } else {
                format!("已取消喜欢：{}", track.name)
            },
            ToastKind::Success,
        );
    }

    fn logout(&mut self) {
        if self.user.is_none() {
            self.toast("当前未登录", ToastKind::Info);
            return;
        }
        self.toast("正在退出登录…", ToastKind::Info);
        worker::spawn_logout(self.net_tx.clone(), self.client.clone());
    }

    // ------------------------------------------------------------------
    // Theme / misc
    // ------------------------------------------------------------------

    fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
        self.cfg.sidebar = self.sidebar_visible;
        let _ = self.cfg.save();
        self.toast(
            if self.sidebar_visible {
                "侧边栏：显示"
            } else {
                "侧边栏：隐藏"
            },
            ToastKind::Info,
        );
    }

    fn open_theme_picker(&mut self) {
        self.theme_picker = Some(ThemePicker {
            index: theme_index(self.theme.name),
            original: self.theme,
        });
    }

    pub fn toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.toast = Some(Toast {
            text: text.into(),
            kind,
            at: Instant::now(),
        });
    }

    pub fn is_liked(&self, id: u64) -> bool {
        self.liked.contains(&id)
    }

    pub fn active_nav(&self) -> Screen {
        match self.screen {
            Screen::Detail => self.detail.as_ref().map(|d| d.from).unwrap_or(Screen::Home),
            Screen::Lyrics => self.lyrics_from,
            other => other,
        }
    }

    pub fn quality_text(&self) -> String {
        if self.player.quality.is_empty() || self.player.format.is_empty() {
            quality_label(&self.cfg.quality).to_string()
        } else {
            format!(
                "{} · {}",
                quality_label(&self.player.quality),
                self.player.format.to_uppercase()
            )
        }
    }

    pub fn shutdown(&mut self) {
        self.cfg.volume = self.cfg.volume.min(100);
        let _ = self.cfg.save();
        self.client.flush_cookies();
        self.audio.shutdown();
    }
}

fn make_qr_lines(url: &str) -> Vec<String> {
    match QrCode::with_error_correction_level(url.as_bytes(), EcLevel::L) {
        Ok(code) => code
            .render::<Dense1x2>()
            .quiet_zone(true)
            .build()
            .lines()
            .map(str::to_string)
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: u64) -> Track {
        Track {
            id,
            name: format!("track-{id}"),
            ..Default::default()
        }
    }

    #[test]
    fn queue_advances_and_remembers_history() {
        let mut queue = QueueState::default();
        queue.set(vec![track(1), track(2), track(3)], 0);
        assert_eq!(queue.current_track().unwrap().id, 1);
        assert_eq!(queue.next_index(false), Some(1));
        queue.current = Some(1);
        assert_eq!(queue.previous_index(), Some(0));
        queue.current = Some(1);
        assert_eq!(queue.next_index(false), Some(2));
        queue.current = Some(2);
        assert_eq!(queue.next_index(false), None);
    }

    #[test]
    fn queue_shuffle_uses_history() {
        let mut queue = QueueState::default();
        queue.set(vec![track(1), track(2), track(3), track(4)], 0);
        let next = queue.next_index(true).unwrap();
        assert_ne!(next, 0);
        assert_eq!(queue.history, vec![0]);
        queue.current = Some(next);
        assert_eq!(queue.previous_index(), Some(0));
    }

    #[test]
    fn queue_remove_adjusts_current() {
        let mut queue = QueueState::default();
        queue.set(vec![track(1), track(2), track(3)], 2);
        queue.remove(0);
        assert_eq!(queue.current, Some(1));
        queue.remove(1);
        assert_eq!(queue.current, Some(0));
        assert_eq!(queue.tracks.len(), 1);
    }

    #[test]
    fn repeat_cycles_through_all_modes() {
        assert_eq!(Repeat::Off.next(), Repeat::All);
        assert_eq!(Repeat::All.next(), Repeat::One);
        assert_eq!(Repeat::One.next(), Repeat::Off);
    }

    #[test]
    fn home_tabs_cycle_both_ways() {
        assert_eq!(HomeTab::ForYou.next(), HomeTab::Charts);
        assert_eq!(HomeTab::ForYou.prev(), HomeTab::Mixes);
        assert_eq!(HomeTab::Mixes.next(), HomeTab::ForYou);
    }
}
