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
	@echo "Installed to $(PREFIX)/bin/$(BINARY)"

run: install
	nohup $(PREFIX)/bin/$(BINARY) >/dev/null 2>&1 &
	@echo "Launched $(BINARY)"

clean:
	cargo clean
