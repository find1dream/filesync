IMAGE := filesync-build:latest
DIST  := dist

.PHONY: all docker native clean help

## Build static Linux binary via Docker (default)
all: docker

## Build static Linux x86_64 binary inside Docker, extract to dist/
docker: $(DIST)/filesync-linux-x86_64

.docker-stamp: Dockerfile Cargo.toml $(wildcard src/*.rs)
	podman build -t $(IMAGE) .
	@touch $@

$(DIST)/filesync-linux-x86_64: .docker-stamp
	@mkdir -p $(DIST)
	$(eval CID := $(shell podman create $(IMAGE) /bin/true))
	podman cp $(CID):/dist/filesync $(DIST)/filesync-linux-x86_64
	podman rm $(CID)
	@chmod +x $(DIST)/filesync-linux-x86_64
	@echo "  →  $(DIST)/filesync-linux-x86_64   ($$(du -sh $(DIST)/filesync-linux-x86_64 | cut -f1))"

## Build binary natively for the current platform (run on macOS for arm64)
native: $(DIST)/filesync-native

$(DIST)/filesync-native: Cargo.toml $(wildcard src/*.rs)
	@mkdir -p $(DIST)
	cargo build --release
	cp target/release/filesync $(DIST)/filesync-native
	@chmod +x $(DIST)/filesync-native
	@echo "  →  $(DIST)/filesync-native   ($$(du -sh $(DIST)/filesync-native | cut -f1))"

# ---- housekeeping ------------------------------------------------------------

## Remove built binaries and stamps
clean:
	rm -rf $(DIST) .docker-stamp

help:
	@printf '\nUsage:\n'
	@printf '  make                build static Linux x86_64 binary via Docker\n'
	@printf '  make native         build binary natively (run on macOS for arm64)\n'
	@printf '  make clean          remove dist/ and rebuild stamps\n'
	@printf '\nOutput:\n'
	@printf '  dist/filesync-linux-x86_64   static binary (any Linux)\n'
	@printf '  dist/filesync-native         native binary for the build platform\n\n'
