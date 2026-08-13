# SPDX-License-Identifier: Apache-2.0
# Build both signer programs. Invoked by the root Makefile.

TARGET ?= aarch64-unknown-linux-gnu
CROSS_COMPILE ?= aarch64-linux-gnu-

# If _HOST or _TA specific compiler/target are not specified, use the common
# compiler/target for both
CROSS_COMPILE_HOST ?= $(CROSS_COMPILE)
CROSS_COMPILE_TA ?= $(CROSS_COMPILE)
TARGET_HOST ?= $(TARGET)
TARGET_TA ?= $(TARGET)

all:
	$(q)make -C host TARGET=$(TARGET_HOST) \
		CROSS_COMPILE=$(CROSS_COMPILE_HOST)
	$(q)make -C ta TARGET=$(TARGET_TA) \
		CROSS_COMPILE=$(CROSS_COMPILE_TA)

clean:
	$(q)make -C host clean
	$(q)make -C ta clean
