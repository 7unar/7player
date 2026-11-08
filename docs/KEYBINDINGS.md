# 按键配置

7player 的普通界面按键由 `src/keymap.rs` 统一解析。内置一套接近 Vim 的默认键位，
可以在 `~/.config/7player/config.toml` 的 `[keys]` 表里按动作覆盖。

## 工作方式

```text
KeyEvent ──► KeyChord ──► Keymap::resolve ──► Action ──► App::perform
```

按键名、大小写、Ctrl/Alt、`g g` 这类多键序列都只存在于 `keymap` 层；`app.rs`
不再直接匹配 `KeyCode`。搜索框、登录面板、主题选择器属于文本输入/弹层上下文，
仍使用各自固定的按键，不受这里影响。

## 基本写法

```toml
[keys]
# 单个按键
quit = "Q"

# 同一动作的多个替代键
move_up = ["k", "up"]
move_down = ["j", "down"]

# 多键序列：用空格分隔
top = ["g g", "home", "0"]

# 禁用某个动作
help = []
```

值可以是字符串，也可以是字符串数组：

```toml
quality = "exhigh"       # 非按键配置
volume = 70
sidebar = true

[keys]
search = ["/", "ctrl+p"]
login = ["L", "ctrl+o"]
```

## 按键语法

- 单个字符：`q`、`/`、`?`、`+`、`-`
- 特殊键：
  `space`、`enter`、`tab`、`backtab`、`esc`、`backspace`、`delete`、`insert`、
  `home`、`end`、`pgup`、`pgdown`、`up`、`down`、`left`、`right`、`f1` … `f24`
- 修饰键：`ctrl+`、`alt+`、`shift+`，例如 `ctrl+d`、`alt+left`
- 多键序列：用空格分开，例如 `g g`
- 同一动作多个键：写成数组
- 禁用：`[]` 或 `"none"`

覆盖是**整组替换**：例如把 `move_up = ["w"]` 后，默认的 `k` 和 `↑` 都不再移动选中项。
如果新按键和另一个动作冲突，后写入的动作生效，启动时会显示一条 toast 警告；
未知动作名和非法按键也会被忽略，并保留该动作的默认绑定。

底部快捷键提示会读取每个动作的**第一个**绑定并实时更新；`?` 帮助页展示的是内置
默认表，底部标题会注明“默认键位，可在 [keys] 覆盖”。

## 默认键位

| 动作 ID | 默认键 | 说明 |
| --- | --- | --- |
| `quit` | `q` | 退出 |
| `help` | `?` | 帮助 |
| `theme` | `t` | 主题选择器 |
| `login` | `L` | 登录 |
| `logout` | `Ctrl+L` | 退出登录 |
| `play_pause` | `Space` | 播放 / 暂停 |
| `next` | `n` | 下一首 |
| `prev` | `p` | 上一首 |
| `nav_prev` | `h` | 上一个页面 |
| `nav_next` | `l` | 下一个页面 |
| `sidebar` | `b` | 显示 / 隐藏侧边栏 |
| `queue` | `f` | 队列页 |
| `lyrics` | `y` | 进入 / 退出纯享歌词 |
| `search` | `/`、`i` | 搜索 |
| `shuffle` | `s` | 随机播放 |
| `repeat` | `r` | 循环模式 |
| `mute` | `m` | 静音 |
| `like` | `z` | 喜欢当前歌曲 |
| `enqueue` | `a` | 加入队列 |
| `enqueue_all` | `A` | 全部加入队列 |
| `refresh` | `R` | 刷新当前页 |
| `move_up` | `↑`、`k` | 上移 |
| `move_down` | `↓`、`j` | 下移 |
| `page_up` | `PgUp` | 上移 10 行 |
| `page_down` | `PgDn` | 下移 10 行 |
| `fast_page_up` | `Ctrl+B` | 上移 20 行 |
| `fast_page_down` | `Ctrl+F` | 下移 20 行 |
| `half_page_up` | `Ctrl+U` | 上移 10 行 |
| `half_page_down` | `Ctrl+D` | 下移 10 行 |
| `top` | `g g`、`Home`、`0` | 跳到首行 |
| `bottom` | `End`、`G`、`$` | 跳到末行 |
| `prev_tab` | `[` | 发现页上一栏 |
| `next_tab` | `]` | 发现页下一栏 |
| `screen_home` | `1` | 发现 |
| `screen_search` | `2` | 搜索 |
| `screen_library` | `3` | 歌单 |
| `screen_queue` | `4` | 队列 |
| `activate` | `Enter` | 打开 / 播放 |
| `back` | `Esc` | 返回 |
| `seek_back` | `←` | 后退 5 秒 |
| `seek_forward` | `→` | 前进 5 秒 |
| `remove_queue` | `d` | 从队列移除（仅队列页） |
| `volume_up` | `+`、`=` | 音量 +5% |
| `volume_down` | `-`、`_` | 音量 -5% |

`gg` 是真正的前缀序列：按下第一个 `g` 后 700ms 内再按 `g` 才生效；超时会自动清空，
不会把很久之后的 `g` 误当成序列的一部分。

## 上下文按键

以下按键不属于全局 keymap，按上下文固定处理：

- 搜索框：字符输入、`Backspace`、`Delete`、`Ctrl+U`、`Enter`、`Esc`
- 登录面板：`Tab`、`Shift+Tab`、`Enter`、`Esc`，以及二维码/手机号切换
- 主题选择器：`j/k`、`↑/↓`、`Enter`、`Esc`、`t`、`q`
- 帮助弹层：`Esc`、`?`、`q` 关闭
- `Ctrl+C`：全局强制退出，始终有效

后续如果要把这些也纳入配置，可以在 `keymap` 里增加 context 维度，而不需要改 UI。

## 排错

启动时如果 `[keys]` 里存在未知动作、非法按键或冲突，顶部会出现一条错误提示：

```text
按键配置: 未知按键动作 `foo`，已忽略
```

配置文件仍能正常加载，出错的动作保留默认绑定。也可以运行：

```bash
./target/release/7player --check
```

确认基础配置和网络没有问题。
