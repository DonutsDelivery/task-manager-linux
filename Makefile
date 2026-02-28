PREFIX ?= $(HOME)/.local
BINARY = task-manager-linux

.PHONY: build install run clean

build:
	cargo build --release

install: build
	@pkill -x $(BINARY) 2>/dev/null; sleep 0.3 || true
	@mkdir -p $(PREFIX)/bin
	cp target/release/$(BINARY) /tmp/$(BINARY)-install
	mv /tmp/$(BINARY)-install $(PREFIX)/bin/$(BINARY)
	@mkdir -p $(PREFIX)/share/icons/hicolor/scalable/apps
	cp icons/task-manager-linux.svg $(PREFIX)/share/icons/hicolor/scalable/apps/task-manager-linux.svg
	@gtk-update-icon-cache -f -t $(PREFIX)/share/icons/hicolor 2>/dev/null || true
	@echo "Installed to $(PREFIX)/bin/$(BINARY)"

run: install
	nohup $(PREFIX)/bin/$(BINARY) >/dev/null 2>&1 &
	@echo "Launched $(BINARY)"

clean:
	cargo clean
