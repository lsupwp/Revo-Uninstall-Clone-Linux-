# GNU make and a C linker are required on Fedora:
#   sudo dnf install cargo rust gcc

BIN := revolinux
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
OUT := dist/$(BIN)-$(VERSION)-x86_64-unknown-linux-gnu.tar.gz
SHA := $(OUT).sha256

.PHONY: all build release test run install uninstall clean dist

all: build

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

run:
	cargo run

install: release
	PREFIX=$(PREFIX) packaging/install.sh

uninstall:
	rm -f $(PREFIX)/bin/revolinux
	rm -f $(PREFIX)/share/applications/revolinux.desktop
	rm -f $(PREFIX)/share/icons/hicolor/scalable/apps/revolinux.svg

clean:
	cargo clean

# Release tarball for GitHub Releases: binary, icon, desktop entry, installer.
dist: release
	rm -rf dist/$(BIN)
	mkdir -p dist/$(BIN)
	cp target/release/$(BIN) dist/$(BIN)/
	cp assets/revolinux.svg assets/revolinux.desktop dist/$(BIN)/
	cp packaging/install-release.sh dist/$(BIN)/install.sh
	chmod +x dist/$(BIN)/install.sh
	tar -C dist -czf $(OUT) $(BIN)
	cd dist && sha256sum $(notdir $(OUT)) > $(notdir $(SHA))
	@echo "built $(OUT)"
	@cat $(SHA)
