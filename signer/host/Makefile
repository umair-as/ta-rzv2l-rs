# SPDX-License-Identifier: Apache-2.0
# Build the signer client + ta-probe for the board.

NAME := signer-client
PROBE := ta-probe

TARGET ?= aarch64-unknown-linux-gnu
CROSS_COMPILE ?= aarch64-linux-gnu-
OBJCOPY := $(CROSS_COMPILE)objcopy
LINKER_CFG := target.$(TARGET).linker=\"$(CROSS_COMPILE)gcc\"

OUT_DIR := $(CURDIR)/target/$(TARGET)/release

all: host strip

host:
	@cargo build --target $(TARGET) --release --config $(LINKER_CFG)

strip: host
	@$(OBJCOPY) --strip-unneeded $(OUT_DIR)/$(NAME) $(OUT_DIR)/$(NAME)
	@$(OBJCOPY) --strip-unneeded $(OUT_DIR)/$(PROBE) $(OUT_DIR)/$(PROBE)

clean:
	@cargo clean
