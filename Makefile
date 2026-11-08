CARGO ?= cargo

.PHONY: build run check test lint fmt install screenshots clean

build:
	$(CARGO) build --release --locked

run:
	$(CARGO) run --locked

# Headless NetEase API / playback URL self-test (requires network)
check:
	$(CARGO) run --locked -- --check

test:
	$(CARGO) test --locked

fmt:
	$(CARGO) fmt

install:
	$(CARGO) install --path . --locked

lint:
	$(CARGO) fmt --check
	$(CARGO) clippy --all-targets -- -D warnings
	$(CARGO) test --locked

# Regenerate a screenshot from a running tmux session named "7player".
# See tools/ansi2png.py for the rendering dependencies.
screenshots:
	tmux capture-pane -e -p -t 7player > /tmp/7player-shot.ansi
	python3 tools/ansi2png.py /tmp/7player-shot.ansi screenshots/preview.png \
		--theme tokyo-night --cols 120 --rows 40 --scale 2 --font-size 14

clean:
	$(CARGO) clean
