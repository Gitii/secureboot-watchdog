UI        := ui
UI_BIN    := $(UI)/target/release/secureboot-watchdog-ui
SUDO      := $(if $(filter 0,$(shell id -u)),,sudo)
DIST      := dist

SCRIPTS   := bin/secureboot-notify sbin/secureboot-check sbin/secureboot-fix \
             sbin/secureboot-tpm-preflight \
             kernel/zzz-secureboot-recheck debian/postinst scripts/*.sh tests/*.sh
UNITS     := systemd/*.service systemd/*.timer systemd/*.path

# Ubuntu 24.04 ships its new-enough Rust toolchain outside PATH; see debian/rules.
ifneq ("$(wildcard /usr/lib/rust-1.91/bin/cargo)","")
    export PATH := /usr/lib/rust-1.91/bin:$(PATH)
endif

.PHONY: all check check-installed lint deb install uninstall clean

all:
	cd $(UI) && cargo build --release --locked

check:
	bash tests/test-tpm-repair-preflight.sh
	bash tests/test-tpm-check.sh
	bash tests/test-notify.sh
	cd $(UI) && cargo test --locked
	@cargo_ver="$$(sed -n 's/^version = "\(.*\)"/\1/p' $(UI)/Cargo.toml | head -n1)"; \
	  deb_ver="$$(dpkg-parsechangelog -S Version | sed 's/[~+].*//')"; \
	  [ "$$cargo_ver" = "$$deb_ver" ] || { echo "ui/Cargo.toml version $$cargo_ver != debian/changelog $$deb_ver"; exit 1; }
	@echo "checks OK"

# Smoke test of the installed package (e.g. from the .deb): the units load,
# the checker emits a status report and the UI binary runs.
check-installed:
	systemd-analyze verify $(addprefix /usr/lib/systemd/system/,$(notdir $(wildcard $(UNITS))))
	$(SUDO) /usr/sbin/secureboot-check | jq -e 'has("ok") and has("issues")' >/dev/null
	secureboot-watchdog-ui --version
	@echo "installed package OK"

lint:
	shellcheck $(SCRIPTS)
	xmllint --noout polkit/*.policy
	cd $(UI) && cargo fmt --check
	cd $(UI) && cargo clippy --all-targets --locked -- -D warnings

# Binary package only; dpkg-buildpackage writes to the parent directory, so
# collect this build's files (by version) in $(DIST)/.
deb:
	dpkg-buildpackage -us -uc -b
	mkdir -p $(DIST)
	@ver="$$(dpkg-parsechangelog -S Version)"; \
	  for f in ../secureboot-watchdog_"$$ver"_* ../secureboot-watchdog-dbgsym_"$$ver"_*; do \
	    [ ! -e "$$f" ] || mv "$$f" $(DIST)/; \
	  done
	@ls -l $(DIST)

install: deb
	$(SUDO) apt-get install -y --reinstall ./$(DIST)/secureboot-watchdog_*.deb

uninstall:
	$(SUDO) apt-get purge -y secureboot-watchdog

clean:
	rm -rf $(DIST) $(UI)/target
	rm -rf debian/.debhelper debian/secureboot-watchdog debian/files debian/*.substvars \
	  debian/*.debhelper.log debian/debhelper-build-stamp
