PREFIX ?= /usr/local
CARGO ?= cargo

.PHONY: all build release test run install uninstall clean

all: build

build:
	$(CARGO) build

release:
	$(CARGO) build --release

test:
	$(CARGO) test

run:
	$(CARGO) run

install: release
	PREFIX=$(PREFIX) packaging/install.sh

uninstall:
	rm -f $(PREFIX)/bin/revolinux
	rm -f $(PREFIX)/share/applications/revolinux.desktop
	rm -f $(PREFIX)/share/icons/hicolor/scalable/apps/revolinux.svg

clean:
	$(CARGO) clean
