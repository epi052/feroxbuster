default_prefix = /usr/local
prefix ?= $(default_prefix)
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
datarootdir = $(prefix)/share
datadir = $(datarootdir)
example_config = ferox-config.toml.example
config_file = ferox-config.toml
completion_dir = shell_completions
completion_prefix = $(completion_dir)/$(BIN)

BIN=feroxbuster
SHR_SOURCES = $(shell find src -type f -wholename '*src/*.rs') Cargo.toml Cargo.lock

RELEASE = debug
DEBUG ?= 0

ifeq (0, $(DEBUG))
	ARGS = --release
	RELEASE = release
endif

VENDORED ?= 0
ifeq (1,$(VENDORED))
    ARGS += --frozen
endif

TARGET = target/$(RELEASE)

.PHONY: all clean install uninstall test update

all: cli
cli: $(TARGET)/$(BIN) $(TARGET)/$(BIN).1.gz $(SHR_SOURCES)
install: all install-cli

verify:
	cargo fmt
	cargo clippy --all-targets --all-features -- -D warnings -A clippy::mutex-atomic
	cargo test

clean:
	cargo clean

vendor: vendor.tar

vendor.tar:
	cargo vendor
	tar pcf vendor.tar vendor
	rm -rf vendor

install-cli: cli
	install -Dm 0644 "$(completion_prefix).bash" "$(DESTDIR)/usr/share/bash-completion/completions/$(BIN).bash"
	install -Dm 0644 "$(completion_prefix).fish" "$(DESTDIR)/usr/share/fish/completions/$(BIN).fish"
	install -Dm 0644 "$(completion_dir)/_$(BIN)" "$(DESTDIR)/usr/share/zsh/vendor-completions/_$(BIN)"
	install -sDm 0755 "$(TARGET)/$(BIN)" "$(DESTDIR)$(bindir)/$(BIN)"
	install -Dm 0644 "$(TARGET)/$(BIN).1.gz" "$(DESTDIR)$(datadir)/man/man1/$(BIN).1.gz"
	install -Dm 0644 "$(example_config)" "$(DESTDIR)/etc/$(BIN)/$(config_file)"

uninstall:
	rm -f "$(DESTDIR)$(bindir)/$(BIN)"
	rm -f "$(DESTDIR)$(datadir)/man/man1/$(BIN).1.gz"
	rm -rf "$(DESTDIR)/etc/$(BIN)/"
	rm -f "$(DESTDIR)/usr/share/bash-completion/completions/$(BIN).bash"
	rm -f "$(DESTDIR)/usr/share/zsh/vendor-completions/_$(BIN)"
	rm -f "$(DESTDIR)/usr/share/fish/completions/$(BIN).fish"

extract:
ifeq (1, $(VENDORED))
	tar pxf vendor.tar
endif

$(TARGET)/$(BIN): extract
	mkdir -p .cargo debian
	touch debian/cargo.config
	cp debian/cargo.config .cargo/config.toml
	cargo build $(ARGS)

$(TARGET)/$(BIN).1.gz: $(TARGET)/$(BIN)
	help2man --no-info $< | gzip -c > $@.partial
	mv $@.partial $@
