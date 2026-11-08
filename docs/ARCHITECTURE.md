# 架构说明

本文说明 7player 的分层、并发模型和关键数据流，便于继续扩展或排查问题。

## 1. 总览

```text
┌─────────────────────────────────────────────────────────────┐
│                         main.rs                             │
│  clap CLI · 终端 raw/alternate screen · tokio event loop    │
└───────────────┬─────────────────────────────┬───────────────┘
                │ KeyEvent / tick             │ Msg
                ▼                             │
┌──────────────────────────────┐              │
│           app.rs             │◄─────────────┘
│  Screen / Focus / Overlay    │
│  Queue / Player / Login      │
│  Theme / Search / Detail     │
└───────┬──────────────┬───────┘
        │ draw         │ commands
        ▼              ▼
┌──────────────┐  ┌──────────────────────────────┐
│    ui/       │  │ audio.rs (dedicated thread)  │
│ ratatui      │  │ rodio Sink / silent clock     │
│ widgets      │  │ snapshot + AudioEvent         │
└──────────────┘  └──────────────────────────────┘
        ▲
        │ Msg
┌───────┴────────────────────────────────────────────────────┐
│ worker.rs (tokio tasks)                                     │
│ search / playlist / song URL / download / lyrics / QR / like│
└───────┬─────────────────────────────────────────────────────┘
        │ reqwest
        ▼
┌────────────────────────────────────────────────────────────┐
│ netease/                                                    │
│  mod.rs    endpoints + weapi/eapi transport                 │
│  crypto.rs AES-CBC + RSA / AES-ECB + MD5                    │
│  cookie.rs persistent CookieJar + Set-Cookie                │
│  model.rs  typed JSON models                                │
│  lrc.rs    LRC parser + translation merge                   │
└────────────────────────────────────────────────────────────┘
```

## 2. 线程与任务

### 主线程 / Tokio runtime

`main.rs` 使用 `#[tokio::main]` 启动多线程 runtime。TUI 事件循环运行在主任务中：

```rust
tokio::select! {
    event = events.next() => app.on_key(...),
    msg = rx.recv() => app.on_msg(msg),
    _ = tick.tick() => app.on_tick(),
}
```

每 16ms（约 60fps）刷新一次画面，保证入场动画和进度平滑。网络请求全部在 `worker.rs` 中用 `tokio::spawn` 派发，主循环只接收 `Msg`，不会因为接口慢而卡住。

### 音频线程

`rodio::OutputStream` 在部分平台上不是 `Send`，因此 `audio.rs` 把它固定在一个独立 OS 线程中：

- UI 通过 `std::sync::mpsc::Sender<AudioCmd>` 发送 `Load / Pause / Resume / Seek / Volume / Stop`。
- 音频线程每 40ms 更新 `Arc<Mutex<PlaybackSnapshot>>`（状态、位置、时长、音量、后端名）。
- 播放结束、解码错误、后端不可用通过 `tokio::sync::mpsc::UnboundedSender<AudioEvent>` 发回；App 在每帧 `on_tick` 里 `try_recv` 排空。
- 没有音频设备时，`Engine` 自动使用 `SilentPlayback` 时钟；位置、暂停、seek、结束事件与真实后端一致。`--no-audio` 可强制该模式。

### 网络任务

`worker.rs` 中每个操作都是一个短生命周期任务，完成后发一条消息。对可能过期的响应使用序号：

- 搜索：`search.seq`
- 歌单详情：`detail_seq`
- 播放请求：`play_seq`（避免用户快速切歌时旧 URL 覆盖新歌）
- 二维码：`login.seq`（刷新或重新打开后丢弃旧轮询）
- 歌词：按 `track_id` 校验

## 3. 网易云协议

### weapi

用于登录、搜索、歌单、歌词、喜欢等：

1. 生成 16 字节随机 secret，并映射到 62 个 `[a-zA-Z0-9]` 字符。
2. `params1 = AES-128-CBC(JSON, preset_key, iv)`，Base64。
3. `params2 = AES-128-CBC(params1, secret, iv)`，Base64 后作为 `params`。
4. `encSecKey = textbook RSA(reverse(secret), e=65537, n=<1024-bit modulus>)`，左侧补零到 128 字节，hex。
5. 表单 POST 到 `https://music.163.com/weapi/...`。

实现见 `src/netease/crypto.rs`，测试覆盖 MD5、Base64/URL 编码、AES 结构与 RSA 输出长度。

### eapi

用于播放地址（`/api/song/enhance/player/url/v1`）：

```text
message = "nobody" + path + "use" + json + "md5forencrypt"
digest  = md5(message)
data    = path + "-36cd479b6b5-" + json + "-36cd479b6b5-" + digest
params  = hex(AES-128-ECB(data, "e82ckenh8dichen8"))
```

请求还会加入 `header` 对象（设备 ID、`appver`、`os`、`requestId` 等）和 `csrf_token`。播放地址请求使用 `Host: interface.music.163.com`。

### Cookie

`CookieJar` 维护：

- `deviceId`：52 位十六进制设备指纹，持久化；
- `_ntes_nuid` / `_ntes_nnid` / `NMTID` / `WNMCID` 等会话标识；
- `MUSIC_U`：登录态核心 Cookie；
- `__csrf`：从 Set-Cookie 提取，用于 weapi 的 query 与 JSON。

匿名注册通过 `/api/register/anonimous` 获取设备级 `MUSIC_U`，让搜索与公开接口更稳定；它不等同于真实登录，UI 是否已登录以 `/api/nuser/account/get` 的 `profile` 为准。

Cookie 文件使用 `OpenOptions::mode(0o600)` 创建临时文件再 rename，避免宽权限；退出和登录成功时主动 flush。

## 4. 播放数据流

```text
Enter
  └─ App::play_context(tracks, index)
       ├─ QueueState::set
       └─ App::start_track(track)
            ├─ play_seq += 1
            ├─ audio.send(Stop)          // 立刻停旧歌
            ├─ spawn_play(seq, track)    // 异步
            └─ spawn_lyric(track.id)

spawn_play:
  song_url(id, quality)
    └─ cache_song(id, url, ext)      // 下载到 ~/.cache/7player/audio/
         └─ Msg::PlayReady { seq, path, url }

App::on_msg(PlayReady):
  if seq != play_seq: ignore
  audio.send(Load { path, duration })
    └─ AudioEvent::Ended
         └─ App::next_track(auto=true)
```

- 下载采用流式写入 `.part`，完成后原子 rename；已存在且 >1KB 的缓存直接复用。
- 播放 URL 为空时返回 `NcmError::NoUrl`，UI 显示“无可用音源”，不会自动跳歌以免死循环。
- 音量/播放模式是 UI 状态；`AudioCmd::Volume` 只改变 Sink 音量。
- `Sink::try_seek` 失败时会发 `AudioEvent::Error`，提示当前音源不支持跳转。

## 5. UI 状态机

### 输入层

键盘输入不再散落在 `app.rs` 的 `match KeyCode` 里，而是走一条明确的管道：

```text
KeyEvent ──► KeyChord ──► Keymap::resolve ──► Action ──► App::perform
```

- `src/keymap.rs` 只依赖 crossterm 和 serde，负责按键语法、前缀序列和配置覆盖；
- `App::perform(Action)` 是唯一的键盘命令入口，便于测试和扩展；
- 搜索框、登录、主题选择器等文本/弹层上下文仍有自己的固定按键，不经过普通 keymap；
- `Ctrl+C` 是全局安全出口，始终硬编码。

这样新增一个动作只需要：在 `Action` 加枚举、在 `DEFAULT_BINDINGS` 加默认键、
在 `App::perform` 加分支，不需要在 UI 里碰 `KeyCode`。

### 模块边界与已知耦合

- `keymap`：只依赖 crossterm/serde，不依赖 `App` 或 ratatui，可单独测试；
- `app`：状态与动作执行中心，依赖 audio/worker/netease，是当前的“状态枢纽”；
- `ui`：immediate-mode 渲染层，读取 `App`，不反向修改业务状态；
- `config`：只依赖 `BindingValue` 类型，不依赖 UI。

仍然存在的耦合：`app` 为鼠标命中注册导入了 `ui::HitAction`，且 `ui/mod.rs` 仍是
单文件（约 2600 行），后续若要继续拆，优先把 `HitAction`/`HitRegion` 移到共享的
input 模型，再把 `ui` 按 header/sidebar/player/overlays/screens 拆分。这不阻塞当前
按键、主题、数据源的扩展。

### Screen

```text
Home ── Enter(playlist) ──► Detail ── Esc ──► 来源页
Search ── Enter(track) ──► 播放，仍留在 Search
Library ── Enter ──► Detail
Queue ── Enter ──► 播放
Lyrics ── Esc ──► 来源页
```

`active_nav()` 让 Detail/Lyrics 仍高亮来源导航项。

### 主界面布局

固定垂直结构：2 行顶栏（第二行是细分隔线）、弹性主体、4 行播放栏、1 行快捷键。
主体宽度为「侧边栏 24 列 + 主内容 + 可选 36 列歌词栏」：

- 左侧侧边栏没有重复的品牌区，顶部只保留 `浏览` 小标题；
- `b` 或点击顶栏的 `▤` 切换侧边栏，状态写入 `config.toml` 的 `sidebar`；
- 侧边栏关闭后主内容自动占满，宽屏右侧歌词栏仍然保留；
- 顶栏品牌只出现一次，当前页面标题紧随其后。

### 纯享歌词

`y` 进入 `Screen::Lyrics` 时，`ui::draw` 直接走全屏分支，不再绘制顶栏、侧边栏、
播放栏和快捷键栏。纯享页只保留：

- 顶部：歌曲名/歌手、专辑/音质、当前时间/总时长；
- 中部：居中的歌词窗口，活动行高亮，翻译紧跟在活动行下；
- 底部：细进度线和 `y / Esc / j / k / g / Space` 提示。

歌词窗口会按活动行做垂直居中，即使当前处于歌曲开头也会把第一句推到视觉中心。
再按 `y` 或 `Esc` 返回来源页。

### 入场动画

`App.splash` 保存动画起始时间；`ui::draw` 在 `splash.is_some()` 时只绘制全屏
签名画面，不渲染主界面。动画由 `on_tick` 以 16ms（约 60fps）推进：

- 0–1.15s：从 `SIGNATURE_LINES` 解码出来的 Braille 点阵按列揭开，模拟笔尖逐点书写；
- 1.15–1.70s：长下划线从左到右收笔，末端带一个小回锋；
- 1.55s 后：淡入“按任意键进入”，并做柔和呼吸；
- 2.8s 自动进入；任意键或鼠标点击立即跳过；`--no-splash` 完全关闭。

签名素材由 Segoe Script 的 “7player” 光栅化后转成 2×4 点/单元的 Braille 矩阵
（66×11 个终端单元），没有使用 figlet 的块状 banner。动画期间网络 bootstrap
仍在后台运行，进入主界面时数据通常已就绪。

### 弹层

优先级从高到低：`help > theme_picker > login > 搜索输入框 > 普通快捷键`。
弹层打开时，普通快捷键被屏蔽，避免 `q`/`Space` 误触。帮助和登录弹层会先调用
`overlay_backdrop()` 清空整个 frame 并铺主题背景，再绘制弹窗，避免底层文字
从弹窗左侧漏出来；主题弹窗保留背景，便于实时判断配色效果。

### 鼠标

鼠标捕获在 `main.rs` 中通过 `EnableMouseCapture` 开启。UI 绘制时把可点击矩形记录在
`ui::HitAction` / `HitRegion` 中（线程局部，绘制结束由 `take_hits()` 取走），
`App::on_mouse` 按“后绘制优先”查找命中区域，因此弹层、播放栏按钮可以覆盖底层列表。

- 列表：`TableState::offset()` + 表头高度换算出可见行号，单击选中并执行 `activate()`。
- 播放栏：歌名区域打开歌词，`▶/⏸` 切换播放，进度条按比例 seek，右下角音量静音。
- 主题选择器/登录面板：行、标签、输入框、提交行分别注册 `HitAction`。
- 滚轮：播放栏调音量，歌词栏滚动歌词，其余区域移动列表选中项。

> 鼠标捕获会阻止普通拖拽选择文本；需要复制时按住 Shift 拖动。

### 主题

`ui/theme.rs` 是唯一的颜色来源，10 套主题均为 `const fn rgb()` 生成的 `Color::Rgb`。切换主题只替换 `App.theme`，不重建任何状态；主题选择器移动时即时预览，Enter 才写盘。

## 6. 错误处理

- 网络/解析错误统一转成 `String` 后进入 `Msg`，由 App 显示 toast，绝不让任务 panic 拖垮 UI。
- 接口错误码映射为中文提示（如 `-462` 登录风控、`502` 密码错误）。
- 歌词加载失败只影响歌词栏，不影响播放。
- 音频设备初始化使用 `catch_unwind` + `Result` 双重保护；失败时降级静音模拟。
- 退出时 `App::shutdown`：保存配置、flush Cookie、停止并 join 音频线程。

## 7. 测试策略

- 纯函数：AES/eapi 可解密回原文、weapi 字段、Cookie 解析与持久化、LRC offset/翻译/多时间戳、JSON 模型、主题索引。
- 状态机：队列 next/prev/shuffle/remove、循环模式、Home tab。
- 联网：`--check` 和 `examples/api_probe.rs` 手动验证真实接口。
- UI：在 tmux 中以 120×40 运行，按键驱动后 `capture-pane` 截图；`tools/ansi2png.py` 可复现截图。

## 8. 扩展点

- **新主题**：在 `ui/theme.rs` 的 `THEME_NAMES` 和 `theme_at()` 增加颜色；在 `ui/mod.rs::theme_description` 加描述。
- **新接口**：在 `netease/mod.rs` 增加方法，`worker.rs` 增加 `Msg` 和 spawn 函数，`app.rs::on_msg` 处理。
- **新页面**：扩展 `Screen`、`current_len/current_selection/set_selection/activate/go_back`，再在 `ui/mod.rs` 加渲染函数。
- **新音频后端**：`audio.rs` 已经把引擎封装在 `Engine`；可替换 Sink 实现，保持 `AudioCmd`/`AudioEvent` 协议即可。
- **逐字歌词**：`lrc.rs` 目前只保留行时间；可增加 `words: Vec<(Duration, String)>`，在 UI 中按时间渐变高亮。

## 9. 安全与合规

- Cookie 等价于登录凭证；文件权限 0600，README 明确提示不要分享。
- 不实现任何绕过付费/版权/地区限制的逻辑；请求的播放 URL 由网易云服务端决定。
- 仅调用公开 Web/移动端接口，不注入、不修改客户端二进制。
- 项目仅用于学习与研究，使用者需自行遵守网易云音乐服务条款和当地法律。
