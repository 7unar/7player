//! Playback engine.
//!
//! Audio runs on a dedicated OS thread because `rodio::OutputStream` is not
//! `Send` on every platform. The UI talks to it through a command channel and
//! reads a shared snapshot every frame. If no audio device is available (for
//! example inside a headless WSL session), the engine falls back to a silent
//! clock so the rest of the application remains fully usable.

use rodio::{Decoder, OutputStream, Sink, Source};
use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioStatus {
    Stopped,
    Loading,
    Playing,
    Paused,
}

#[derive(Debug, Clone)]
pub struct PlaybackSnapshot {
    pub status: AudioStatus,
    pub position: Duration,
    pub duration: Duration,
    pub volume: f32,
    pub backend: String,
}

impl Default for PlaybackSnapshot {
    fn default() -> Self {
        Self {
            status: AudioStatus::Stopped,
            position: Duration::ZERO,
            duration: Duration::ZERO,
            volume: 0.7,
            backend: "未初始化".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum AudioCmd {
    Load { path: PathBuf, duration: Duration },
    Pause,
    Resume,
    Toggle,
    Stop,
    Seek(Duration),
    SeekBy(i64),
    Volume(f32),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum AudioEvent {
    Ended,
    Error(String),
    Backend(String),
}

pub struct AudioHandle {
    tx: Sender<AudioCmd>,
    snapshot: Arc<Mutex<PlaybackSnapshot>>,
    events: UnboundedReceiver<AudioEvent>,
    join: Option<thread::JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

/// WSL2 has no native sound card. WSLg exposes a PulseAudio socket, but ALSA's
/// `default` PCM often points at a missing card unless the user has written an
/// `~/.asoundrc`. Point ALSA at WSLg's Pulse server automatically, before the
/// audio thread initialises cpal. A user-supplied `ALSA_CONFIG_PATH` always wins.
fn prepare_wsl_audio() {
    if std::env::var_os("ALSA_CONFIG_PATH").is_some() {
        return;
    }
    if std::env::var_os("WSL_DISTRO_NAME").is_none() {
        return;
    }
    if std::path::Path::new("/proc/asound/cards").exists() {
        return;
    }
    if std::env::var_os("PULSE_SERVER").is_none()
        || !std::path::Path::new("/mnt/wslg/PulseServer").exists()
    {
        return;
    }
    let dir = crate::config::cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("asound-pulse.conf");
    let config = "pcm.!default { type pulse }\nctl.!default { type pulse }\n";
    if std::fs::write(&path, config).is_err() {
        return;
    }
    // Safe here: this runs on the UI thread before any audio/network threads are
    // started by App. No other thread is reading the environment yet.
    unsafe {
        std::env::set_var("ALSA_CONFIG_PATH", &path);
    }
}

impl AudioHandle {
    pub fn spawn(force_silent: bool, volume: f32) -> Self {
        if !force_silent {
            prepare_wsl_audio();
        }
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = unbounded_channel();
        let snapshot = Arc::new(Mutex::new(PlaybackSnapshot {
            volume,
            ..Default::default()
        }));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_snapshot = snapshot.clone();
        let thread_shutdown = shutdown.clone();
        let join = thread::Builder::new()
            .name("ncm-audio".into())
            .spawn(move || {
                let mut engine = Engine::new(
                    force_silent,
                    volume,
                    thread_snapshot,
                    event_tx.clone(),
                    thread_shutdown,
                );
                engine.run(cmd_rx);
            })
            .ok();
        Self {
            tx: cmd_tx,
            snapshot,
            events: event_rx,
            join,
            shutdown,
        }
    }

    pub fn send(&self, cmd: AudioCmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn snapshot(&self) -> PlaybackSnapshot {
        self.snapshot.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn events_mut(&mut self) -> &mut UnboundedReceiver<AudioEvent> {
        &mut self.events
    }

    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = self.tx.send(AudioCmd::Shutdown);
        if let Some(join) = self.join.take() {
            // rodio prints a drop notice to stderr; keep the shell clean.
            let _ = silence_stderr(|| join.join());
        }
    }
}

impl Drop for AudioHandle {
    fn drop(&mut self) {
        if self.join.is_some() {
            self.shutdown();
        }
    }
}

struct Engine {
    snapshot: Arc<Mutex<PlaybackSnapshot>>,
    events: UnboundedSender<AudioEvent>,
    shutdown: Arc<AtomicBool>,
    stream: Option<OutputStream>,
    sink: Option<Sink>,
    loaded: bool,
    volume: f32,
    silent: Option<SilentPlayback>,
    silent_mode: bool,
}

struct SilentPlayback {
    duration: Duration,
    offset: Duration,
    started: Instant,
    paused: bool,
}

impl SilentPlayback {
    fn new(duration: Duration) -> Self {
        Self {
            duration,
            offset: Duration::ZERO,
            started: Instant::now(),
            paused: false,
        }
    }

    fn position(&self) -> Duration {
        if self.paused {
            self.offset.min(self.duration)
        } else {
            (self.offset + self.started.elapsed()).min(self.duration)
        }
    }

    fn pause(&mut self) {
        if !self.paused {
            self.offset = self.position();
            self.paused = true;
        }
    }

    fn resume(&mut self) {
        if self.paused {
            self.paused = false;
            self.started = Instant::now();
        }
    }

    fn seek(&mut self, position: Duration) {
        self.offset = position.min(self.duration);
        self.started = Instant::now();
    }

    fn ended(&self) -> bool {
        !self.paused && self.duration > Duration::ZERO && self.position() >= self.duration
    }
}

impl Engine {
    fn new(
        force_silent: bool,
        volume: f32,
        snapshot: Arc<Mutex<PlaybackSnapshot>>,
        events: UnboundedSender<AudioEvent>,
        shutdown: Arc<AtomicBool>,
    ) -> Self {
        let mut engine = Self {
            snapshot,
            events,
            shutdown,
            stream: None,
            sink: None,
            loaded: false,
            volume,
            silent: None,
            silent_mode: force_silent,
        };
        if !force_silent {
            engine.try_open_output();
        } else {
            engine.set_backend("静音模拟（--no-audio）");
            let _ = engine
                .events
                .send(AudioEvent::Backend("已启用静音模拟模式".into()));
        }
        engine
    }

    fn try_open_output(&mut self) {
        // cpal/ALSA probe missing cards and write PAGEs of errors to stderr.
        // The TUI owns the terminal at this point, so those writes corrupt the
        // alternate screen. Silence stderr for the probe; the actual failure is
        // reported through AudioEvent::Backend.
        let opened = silence_stderr(|| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let events = self.events.clone();
                let builder = rodio::OutputStreamBuilder::from_default_device()?
                    .with_error_callback(move |err| {
                        // Never print to stderr while the TUI owns the screen.
                        let _ = events.send(AudioEvent::Error(format!("音频输出流错误: {err}")));
                    });
                let stream = builder.open_stream()?;
                let sink = Sink::connect_new(stream.mixer());
                sink.set_volume(self.volume);
                Ok::<_, rodio::StreamError>((stream, sink))
            }))
        });
        match opened {
            Ok(Ok((stream, sink))) => {
                self.stream = Some(stream);
                self.sink = Some(sink);
                self.set_backend("ALSA / PulseAudio");
            }
            Ok(Err(err)) => {
                self.silent_mode = true;
                self.set_backend("静音模拟（无音频设备）");
                let _ = self.events.send(AudioEvent::Backend(format!(
                    "无法打开音频设备，已切换静音模拟：{err}"
                )));
            }
            Err(_) => {
                self.silent_mode = true;
                self.set_backend("静音模拟（音频初始化崩溃）");
                let _ = self
                    .events
                    .send(AudioEvent::Backend("音频初始化失败，已切换静音模拟".into()));
            }
        }
    }

    fn set_backend(&self, name: &str) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.backend = name.to_string();
            snapshot.volume = self.volume;
        }
    }

    fn publish(&self) {
        let (status, position, duration) = self.current_state();
        if let Ok(mut snapshot) = self.snapshot.lock() {
            if matches!(status, AudioStatus::Playing | AudioStatus::Paused) {
                snapshot.status = status;
                snapshot.position = position;
                snapshot.duration = duration;
            }
            snapshot.volume = self.volume;
        }
    }

    fn current_state(&self) -> (AudioStatus, Duration, Duration) {
        if let Some(sink) = &self.sink {
            let status = if self.loaded {
                if sink.is_paused() {
                    AudioStatus::Paused
                } else if sink.empty() {
                    AudioStatus::Stopped
                } else {
                    AudioStatus::Playing
                }
            } else {
                AudioStatus::Stopped
            };
            let duration = self.snapshot.lock().map(|s| s.duration).unwrap_or_default();
            (status, sink.get_pos().min(duration), duration)
        } else if let Some(silent) = &self.silent {
            let status = if silent.paused {
                AudioStatus::Paused
            } else {
                AudioStatus::Playing
            };
            (status, silent.position(), silent.duration)
        } else {
            let duration = self.snapshot.lock().map(|s| s.duration).unwrap_or_default();
            (AudioStatus::Stopped, Duration::ZERO, duration)
        }
    }

    fn run(&mut self, rx: Receiver<AudioCmd>) {
        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(40)) {
                Ok(cmd) => self.handle(cmd),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.publish();
            self.check_end();
        }
        if let Some(sink) = &self.sink {
            sink.stop();
        }
    }

    fn handle(&mut self, cmd: AudioCmd) {
        match cmd {
            AudioCmd::Load { path, duration } => self.load(path, duration),
            AudioCmd::Pause => self.pause(),
            AudioCmd::Resume => self.resume(),
            AudioCmd::Toggle => {
                if self.is_paused() {
                    self.resume();
                } else {
                    self.pause();
                }
            }
            AudioCmd::Stop => self.stop(),
            AudioCmd::Seek(position) => self.seek(position),
            AudioCmd::SeekBy(delta) => {
                let (_, pos, duration) = self.current_state();
                let target = if delta >= 0 {
                    (pos + Duration::from_secs(delta as u64)).min(duration)
                } else {
                    pos.saturating_sub(Duration::from_secs(delta.unsigned_abs()))
                };
                self.seek(target);
            }
            AudioCmd::Volume(volume) => self.set_volume(volume),
            AudioCmd::Shutdown => {
                self.stop();
                if let Some(sink) = &self.sink {
                    sink.stop();
                }
                self.shutdown.store(true, Ordering::Relaxed);
            }
        }
    }

    fn is_paused(&self) -> bool {
        self.sink
            .as_ref()
            .map(Sink::is_paused)
            .or_else(|| self.silent.as_ref().map(|s| s.paused))
            .unwrap_or(false)
    }

    fn load(&mut self, path: PathBuf, duration: Duration) {
        self.loaded = false;
        if self.silent_mode {
            self.silent = Some(SilentPlayback::new(duration));
            self.loaded = true;
            self.set_status(AudioStatus::Playing, Duration::ZERO, duration);
            return;
        }

        let file = match File::open(&path) {
            Ok(file) => file,
            Err(err) => {
                let _ = self
                    .events
                    .send(AudioEvent::Error(format!("打开音频文件失败: {err}")));
                self.set_status(AudioStatus::Stopped, Duration::ZERO, duration);
                return;
            }
        };
        let decoder = match Decoder::try_from(file) {
            Ok(decoder) => decoder,
            Err(err) => {
                let _ = self
                    .events
                    .send(AudioEvent::Error(format!("解码音频失败: {err}")));
                self.set_status(AudioStatus::Stopped, Duration::ZERO, duration);
                return;
            }
        };
        let total = decoder.total_duration().unwrap_or(duration);
        let Some(sink) = &self.sink else {
            self.silent = Some(SilentPlayback::new(total));
            self.loaded = true;
            self.set_status(AudioStatus::Playing, Duration::ZERO, total);
            return;
        };
        sink.stop();
        sink.set_volume(self.volume);
        sink.append(decoder);
        sink.play();
        self.silent = None;
        self.loaded = true;
        self.set_status(AudioStatus::Playing, Duration::ZERO, total);
    }

    fn set_status(&self, status: AudioStatus, position: Duration, duration: Duration) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.status = status;
            snapshot.position = position;
            snapshot.duration = duration;
            snapshot.volume = self.volume;
        }
    }

    fn pause(&mut self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
        if let Some(silent) = &mut self.silent {
            silent.pause();
        }
        let (_, pos, duration) = self.current_state();
        self.set_status(AudioStatus::Paused, pos, duration);
    }

    fn resume(&mut self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
        if let Some(silent) = &mut self.silent {
            silent.resume();
        }
        let (_, pos, duration) = self.current_state();
        self.set_status(AudioStatus::Playing, pos, duration);
    }

    fn stop(&mut self) {
        if let Some(sink) = &self.sink {
            sink.stop();
        }
        self.silent = None;
        self.loaded = false;
        let duration = self.snapshot.lock().map(|s| s.duration).unwrap_or_default();
        self.set_status(AudioStatus::Stopped, Duration::ZERO, duration);
    }

    fn seek(&mut self, position: Duration) {
        if let Some(sink) = &self.sink {
            match sink.try_seek(position) {
                Ok(()) => {}
                Err(err) => {
                    let _ = self
                        .events
                        .send(AudioEvent::Error(format!("当前音源不支持跳转: {err}")));
                }
            }
        }
        if let Some(silent) = &mut self.silent {
            silent.seek(position);
        }
        let (_, pos, duration) = self.current_state();
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.position = pos;
            snapshot.duration = duration;
        }
    }

    fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
        }
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.volume = self.volume;
        }
    }

    fn check_end(&mut self) {
        if !self.loaded {
            return;
        }
        let ended = if let Some(sink) = &self.sink {
            sink.empty() && !sink.is_paused()
        } else if let Some(silent) = &self.silent {
            silent.ended()
        } else {
            false
        };
        if ended {
            self.loaded = false;
            let duration = self.snapshot.lock().map(|s| s.duration).unwrap_or_default();
            if let Ok(mut snapshot) = self.snapshot.lock() {
                snapshot.status = AudioStatus::Stopped;
                snapshot.position = duration;
            }
            self.silent = None;
            let _ = self.events.send(AudioEvent::Ended);
        }
    }
}

/// Redirect fd 2 to `/dev/null` for the duration of `f`, then restore it.
///
/// ALSA prints configuration errors directly to stderr from C, bypassing Rust's
/// logging. That is fine in a CLI, but while the ratatui alternate screen is
/// active those writes overwrite the UI and make it look scrambled. The audio
/// probe is the only place where these errors can occur.
#[cfg(unix)]
fn silence_stderr<T>(f: impl FnOnce() -> T) -> T {
    use std::os::unix::io::{AsRawFd, RawFd};

    struct Restore(RawFd);
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe {
                libc::dup2(self.0, 2);
                libc::close(self.0);
            }
        }
    }

    let saved = unsafe { libc::dup(2) };
    if saved < 0 {
        return f();
    }
    let devnull = match std::fs::OpenOptions::new().write(true).open("/dev/null") {
        Ok(file) => file,
        Err(_) => {
            unsafe { libc::close(saved) };
            return f();
        }
    };
    if unsafe { libc::dup2(devnull.as_raw_fd(), 2) } < 0 {
        unsafe { libc::close(saved) };
        return f();
    }
    let _restore = Restore(saved);
    f()
}

#[cfg(not(unix))]
fn silence_stderr<T>(f: impl FnOnce() -> T) -> T {
    f()
}

/// Small clock used by the silent backend; unit-tested separately because it is
/// deterministic without an audio device.
#[derive(Debug)]
pub struct SilentClock {
    offset: Duration,
    started: Instant,
    duration: Duration,
    paused: bool,
}

impl SilentClock {
    pub fn new(duration: Duration) -> Self {
        Self {
            offset: Duration::ZERO,
            started: Instant::now(),
            duration,
            paused: false,
        }
    }

    pub fn position(&self) -> Duration {
        if self.paused {
            self.offset.min(self.duration)
        } else {
            (self.offset + self.started.elapsed()).min(self.duration)
        }
    }

    pub fn pause(&mut self) {
        self.offset = self.position();
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
        self.started = Instant::now();
    }

    pub fn seek(&mut self, position: Duration) {
        self.offset = position.min(self.duration);
        self.started = Instant::now();
    }

    pub fn ended(&self) -> bool {
        !self.paused && self.duration > Duration::ZERO && self.position() >= self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_clock_pauses_and_resumes() {
        let mut clock = SilentClock::new(Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(30));
        clock.pause();
        let a = clock.position();
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(a, clock.position());
        clock.resume();
        assert!(!clock.ended());
        clock.seek(Duration::from_secs(10));
        assert!(clock.ended());
    }

    #[test]
    fn silent_clock_zero_duration_never_ends() {
        let clock = SilentClock::new(Duration::ZERO);
        assert!(!clock.ended());
    }

    #[test]
    fn snapshot_defaults_are_sane() {
        let snap = PlaybackSnapshot::default();
        assert_eq!(snap.status, AudioStatus::Stopped);
        assert!(snap.volume > 0.0);
    }
}
