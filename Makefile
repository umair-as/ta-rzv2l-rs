# SPDX-License-Identifier: Apache-2.0
#
# Build and deploy the signer TA + client.
#
# Machine-specific paths live in an untracked `local.mk` (see README):
#   TA_DEV_KIT_DIR       OP-TEE TA dev kit export dir (export-ta_arm64)
#   OPTEE_CLIENT_EXPORT  dir containing usr/lib/libteec.so for cross-linking
#   BOARD_HOST           board IP for `make deploy` / `make smoke`
-include local.mk

BOARD_USER ?= devel
SSH ?= ssh
SCP ?= scp
TARGET ?= aarch64-unknown-linux-gnu

UUID := $(shell cat signer/uuid.txt)
TA_BIN := signer/ta/target/$(TARGET)/release/$(UUID).ta
CLIENT_BIN := signer/host/target/$(TARGET)/release/signer-client
PROBE_BIN := signer/host/target/$(TARGET)/release/ta-probe

export TA_DEV_KIT_DIR OPTEE_CLIENT_EXPORT

all:
	@test -n "$(TA_DEV_KIT_DIR)" || { echo "TA_DEV_KIT_DIR is not set - create local.mk (see README)" >&2; exit 2; }
	@test -n "$(OPTEE_CLIENT_EXPORT)" || { echo "OPTEE_CLIENT_EXPORT is not set - create local.mk (see README)" >&2; exit 2; }
	$(MAKE) -C signer

# Recipe lines are silenced (@): SSH/SCP may be overridden in local.mk with
# credential-carrying wrappers, and make would otherwise echo them expanded.
deploy: all
	@test -n "$(BOARD_HOST)" || { echo "BOARD_HOST is not set" >&2; exit 2; }
	@echo "SCP  =>  $(UUID).ta signer-client ta-probe -> $(BOARD_USER)@$(BOARD_HOST):/tmp/"
	@$(SCP) $(TA_BIN) $(CLIENT_BIN) $(PROBE_BIN) $(BOARD_USER)@$(BOARD_HOST):/tmp/
	@$(SSH) $(BOARD_USER)@$(BOARD_HOST) "sudo install -m 444 /tmp/$(UUID).ta /lib/optee_armtz/ \
		&& sudo install -D -m 755 /tmp/signer-client /usr/local/bin/signer-client \
		&& sudo install -D -m 755 /tmp/ta-probe /usr/local/bin/ta-probe \
		&& rm -f /tmp/$(UUID).ta /tmp/signer-client /tmp/ta-probe"
	@echo "DEPLOY => $(UUID).ta + signer-client to $(BOARD_HOST)"

test:
	@test -n "$(BOARD_HOST)" || { echo "BOARD_HOST is not set" >&2; exit 2; }
	@BOARD_HOST=$(BOARD_HOST) BOARD_USER=$(BOARD_USER) SSH="$(SSH)" REBOOT=$(REBOOT) tests/board-smoke.sh

smoke: test

# fmt + clippy. TA crate uses the SDK's strict lint set (no unwrap/expect/panic
# in secure-world code); clippy needs the same env as a build.
lint:
	@test -n "$(TA_DEV_KIT_DIR)" || { echo "TA_DEV_KIT_DIR is not set - create local.mk (see README)" >&2; exit 2; }
	@test -n "$(OPTEE_CLIENT_EXPORT)" || { echo "OPTEE_CLIENT_EXPORT is not set - create local.mk (see README)" >&2; exit 2; }
	cd signer/proto && cargo fmt --check && cargo clippy --release -- -D warnings
	cd signer/ta && cargo fmt --check && RUSTFLAGS="-C panic=abort" cargo clippy --target $(TARGET) --release -- \
		-D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic
	cd signer/host && cargo fmt --check && cargo clippy --target $(TARGET) --release -- -D warnings

clean:
	$(MAKE) -C signer clean

.PHONY: all deploy test smoke lint clean
