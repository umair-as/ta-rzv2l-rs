# SPDX-License-Identifier: Apache-2.0
# Build, strip and sign the TA. Follows the SDK v0.7.0 example shape.

UUID ?= $(shell cat "../uuid.txt")

TARGET ?= aarch64-unknown-linux-gnu
CROSS_COMPILE ?= aarch64-linux-gnu-
OBJCOPY := $(CROSS_COMPILE)objcopy
# GCC as linker works for both cross-compilation and native ARM builds
LINKER_CFG := target.$(TARGET).linker=\"$(CROSS_COMPILE)gcc\"

# no-std TAs cannot unwind; make that explicit everywhere
RUSTFLAGS := -C panic=abort

TA_SIGN_KEY ?= $(TA_DEV_KIT_DIR)/keys/default_ta.pem
SIGN := $(TA_DEV_KIT_DIR)/scripts/sign_encrypt.py
OUT_DIR := $(CURDIR)/target/$(TARGET)/release

# One explicit interpreter for the dev kit's signing script instead of its env
# shebang: a virtualenv python3 without `cryptography` breaks signing with a
# confusing error otherwise. Same knob as tests/board-smoke.sh.
PYTHON ?= python3

all: ta strip sign

ta:
	@RUSTFLAGS="$(RUSTFLAGS)" cargo build --target $(TARGET) --release --config $(LINKER_CFG)

strip: ta
	@$(OBJCOPY) --strip-unneeded $(OUT_DIR)/ta $(OUT_DIR)/stripped_ta

sign: strip
	@$(PYTHON) -c 'import cryptography' 2>/dev/null || \
		{ echo "error: $(PYTHON) cannot import 'cryptography' (needed by sign_encrypt.py); deactivate the venv or set PYTHON=" >&2; exit 2; }
	@$(PYTHON) $(SIGN) --uuid $(UUID) --key $(TA_SIGN_KEY) --in $(OUT_DIR)/stripped_ta --out $(OUT_DIR)/$(UUID).ta
	@echo "SIGN =>  ${UUID}"

clean:
	@cargo clean
