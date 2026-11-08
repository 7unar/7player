# Contributing to 7player

Thanks for taking the time to improve 7player. This is a small Rust TUI project;
issues and focused pull requests are both welcome.

## Before you start

- 7player is an **unofficial** NetEase Cloud Music client. Do not add features
  intended to bypass VIP, paid, copyright or region restrictions.
- Never commit cookies, tokens, account names, or captured `cookies.json` files.
- For large changes, open an issue first so the behavior and scope can be agreed on.

## Development setup

```bash
git clone https://github.com/7unar/7player.git
cd 7player

# Debian/Ubuntu
sudo apt install build-essential pkg-config libasound2-dev

# Arch
sudo pacman -S --needed base-devel pkgconf alsa-lib

cargo build
cargo run -- --no-splash
```

A terminal of at least 100×30 is recommended; 120×40 is the normal layout.

## Checks

CI runs the same commands:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --check   # optional, requires network
```

For audio work, also try:

```bash
cargo run -- --no-audio        # silent fallback must still render correctly
cargo run -- --no-splash       # fast iteration
```

## Code layout

```text
src/main.rs        CLI, terminal lifecycle, --check
src/app.rs         application state and Action execution
src/keymap.rs      KeyEvent -> KeyChord -> Action, default bindings
src/config.rs      config.toml, paths and [keys] overrides
src/audio.rs       Rodio thread, silent fallback, WSL audio setup
src/worker.rs      async network tasks and Msg
src/ui/            Ratatui rendering and hit regions
src/netease/       API client, crypto, cookies, models, LRC
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[docs/KEYBINDINGS.md](docs/KEYBINDINGS.md) for details.

### Adding a command

1. Add a variant to `Action` in `src/keymap.rs`.
2. Add its default binding to `DEFAULT_BINDINGS`.
3. Implement it in `App::perform` in `src/app.rs`.
4. If it should appear in the footer, add it to `render_footer` in `src/ui/mod.rs`.
5. Add a test for parsing/resolution if the binding is non-trivial.

## Pull request checklist

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] Checked the TUI at 120×40 and, when relevant, 80×24
- [ ] No cookies, logs, `target/`, or generated screenshots beyond the intended ones
- [ ] Updated `README.md` / `docs/` when behavior or keybindings changed

## Commit style

Short conventional commits are preferred:

```text
feat: add queue reordering
fix: keep ALSA stderr out of the alternate screen
docs: document [keys] overrides
```

## Release process

Maintainers:

1. Update `CHANGELOG.md` and the version in `Cargo.toml`.
2. Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
3. Commit and tag: `git tag -a vX.Y.Z -m "7player vX.Y.Z"`.
4. Push the tag: `git push origin vX.Y.Z`.
5. The `Release` workflow builds the Linux x86_64 tarball and attaches it to the
   GitHub release. The installer resolves the `latest` release by default.
