default_prefix = /usr/local
prefix ?= $(default_prefix)
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
datarootdir = $(prefix)/share
datadir = $(datarootdir)
example_config = ferox-config.toml.example
config_file = ferox-config.toml

SHR_SOURCES = $(shell find src -type f -wholename '*src/*.rs') Cargo.toml Cargo.lock

RELEASE = debug
DEBUG ?= 0
ifeq (0,$(DEBUG))
	ARGS = --release
	RELEASE = release
endif

VENDORED ?= 0
ifeq (1,$(VENDORED))
    ARGS += --frozen
endif

TARGET = target/$(RELEASE)

.PHONY: all clean distclean install uninstall update

BIN=feroxbuster
DESKTOP=$(APPID).desktop

all: cli

cli: $(TARGET)/$(BIN) $(TARGET)/$(BIN).1.gz $(SHR_SOURCES)

clean:
	cargo clean

distclean: clean
	rm -rf .cargo vendor Cargo.lock vendor.tar

vendor: vendor.tar

vendor.tar:
	mkdir -p .cargo
	cargo vendor | head -n -1 > .cargo/config
	echo 'directory = "vendor"' >> .cargo/config
	tar pcf vendor.tar vendor
	rm -rf vendor

install-cli: cli
	install -Dm 0755 "$(TARGET)/$(BIN)" "$(DESTDIR)$(bindir)/$(BIN)"
	install -Dm 0644 "$(TARGET)/$(BIN).1.gz" "$(DESTDIR)$(datadir)/man/man1/$(BIN).1.gz"
	install -Dm 0644 "$(example_config)" "/etc/$(BIN)/$(config_File)"

install: all install-cli

uninstall-cli:
	rm -f "$(DESTDIR)$(bindir)/$(BIN)"
	rm -f "$(DESTDIR)$(datadir)/man/man1/$(BIN).1.gz"
	rm -rf "/etc/$(BIN)/"

uninstall: uninstall-cli

update:
	cargo update

extract:
ifeq ($(VENDORED),1)
	tar pxf vendor.tar
endif

$(TARGET)/$(BIN): extract
	cargo build --manifest-path Cargo.toml $(ARGS)

$(TARGET)/$(BIN).1.gz: $(TARGET)/$(BIN)
	help2man --no-info $< | gzip -c > $@.partial
	mv $@.partial $@
