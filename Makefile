PREFIX ?= $(HOME)/.local

.PHONY: all build install uninstall clean

all: build

build:
	cargo build --release

install: build
	install -Dm755 target/release/rmd $(PREFIX)/bin/rmd
	install -Dm644 extra/rmd.service $(HOME)/.config/systemd/user/rmd.service
	systemctl --user daemon-reload

uninstall:
	systemctl --user stop rmd.service 2>/dev/null || true
	systemctl --user disable rmd.service 2>/dev/null || true
	rm -f $(PREFIX)/bin/rmd
	rm -f $(HOME)/.config/systemd/user/rmd.service
	systemctl --user daemon-reload

clean:
	cargo clean
