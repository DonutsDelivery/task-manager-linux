PREFIX ?= $(HOME)/.local
DESTDIR ?=
BINARY = donut-taskmgr
APP_ID = online.donutsdelivery.DonutTaskManager
ICON_SIZES = 16 22 24 32 48 64 128 256 512
# Names from before the DonutTaskManager rename. The kernel truncates comm to
# 15 characters, so the old process shows up as task-manager-li.
LEGACY_BINARY = task-manager-linux
LEGACY_COMM = task-manager-li

.PHONY: build install uninstall run clean

build:
	cargo build --release

install: build
	@pkill -x $(BINARY) 2>/dev/null; pkill -x $(LEGACY_COMM) 2>/dev/null; sleep 0.3 || true
	install -d $(DESTDIR)$(PREFIX)/bin
	install -m755 target/release/$(BINARY) $(DESTDIR)$(PREFIX)/bin/$(BINARY).new
	mv $(DESTDIR)$(PREFIX)/bin/$(BINARY).new $(DESTDIR)$(PREFIX)/bin/$(BINARY)
	install -Dm644 icons/$(APP_ID).svg $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg
	for size in $(ICON_SIZES); do \
		install -Dm644 icons/hicolor/$${size}x$${size}/apps/$(APP_ID).png \
			$(DESTDIR)$(PREFIX)/share/icons/hicolor/$${size}x$${size}/apps/$(APP_ID).png; \
	done
	install -Dm644 data/$(APP_ID).desktop $(DESTDIR)$(PREFIX)/share/applications/$(APP_ID).desktop
	install -Dm644 data/$(APP_ID).metainfo.xml $(DESTDIR)$(PREFIX)/share/metainfo/$(APP_ID).metainfo.xml
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/$(BINARY)/LICENSE
	rm -f $(DESTDIR)$(PREFIX)/bin/$(LEGACY_BINARY) \
		$(DESTDIR)$(PREFIX)/share/applications/task-manager.desktop \
		$(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/$(LEGACY_BINARY).svg
	@gtk-update-icon-cache -f -t $(DESTDIR)$(PREFIX)/share/icons/hicolor 2>/dev/null || true
	@update-desktop-database $(DESTDIR)$(PREFIX)/share/applications 2>/dev/null || true
	@echo "Installed to $(DESTDIR)$(PREFIX)/bin/$(BINARY)"

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(BINARY)
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg
	for size in $(ICON_SIZES); do \
		rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/$${size}x$${size}/apps/$(APP_ID).png; \
	done
	rm -f $(DESTDIR)$(PREFIX)/share/applications/$(APP_ID).desktop
	rm -f $(DESTDIR)$(PREFIX)/share/metainfo/$(APP_ID).metainfo.xml
	rm -rf $(DESTDIR)$(PREFIX)/share/licenses/$(BINARY)

run: install
	nohup $(PREFIX)/bin/$(BINARY) >/dev/null 2>&1 &
	@echo "Launched $(BINARY)"

clean:
	cargo clean
