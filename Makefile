IMAGE       := filesync-build:latest
DIST        := dist
BINARY      := $(DIST)/filesync
INSTALL_DIR := /usr/local/bin

.PHONY: all docker native install clean help

## Build static Linux binary via Docker (default)
all: docker

## Build static Linux x86_64 binary inside Docker, extract to dist/filesync
docker: $(BINARY)

.docker-stamp: Dockerfile Cargo.toml $(wildcard src/*.rs)
	podman build -t $(IMAGE) .
	@touch $@

$(BINARY): .docker-stamp
	@mkdir -p $(DIST)
	$(eval CID := $(shell podman create $(IMAGE) /bin/true))
	podman cp $(CID):/dist/filesync $(BINARY)
	podman rm $(CID)
	@chmod +x $(BINARY)
	@echo "  →  $(BINARY)   ($$(du -sh $(BINARY) | cut -f1))"

## Build binary natively for the current platform (run on macOS for arm64)
native:
	@mkdir -p $(DIST)
	cargo build --release
	cp target/release/filesync $(BINARY)
	@chmod +x $(BINARY)
	@echo "  →  $(BINARY)   ($$(du -sh $(BINARY) | cut -f1))"

## Install dist/filesync to $(INSTALL_DIR)
install: $(BINARY)
	install -m 755 $(BINARY) $(INSTALL_DIR)/filesync
	@echo "  installed  →  $(INSTALL_DIR)/filesync"

# ---- housekeeping ------------------------------------------------------------

## Remove built binaries and stamps
clean:
	rm -rf $(DIST) .docker-stamp

help:
	@printf '\nUsage:\n'
	@printf '  make                build static Linux x86_64 binary via Docker\n'
	@printf '  make native         build binary natively (run on macOS for arm64)\n'
	@printf '  make install        install to $(INSTALL_DIR)/filesync\n'
	@printf '  make clean          remove dist/ and rebuild stamps\n'
	@printf '\nOutput:\n'
	@printf '  dist/filesync       binary for the build platform\n\n'
