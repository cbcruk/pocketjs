DEVKITPRO ?= /opt/devkitpro
DEVKITARM ?= $(DEVKITPRO)/devkitARM
CC := $(DEVKITARM)/bin/arm-none-eabi-gcc
ARCH := -march=armv6k -mtune=mpcore -mfloat-abi=hard -mtp=soft
BACKEND := /repo/engine/pocket3d/backends/citro3d
SOURCE := /repo/engine/pocket3d/examples/island/3ds
BUILD := /repo/dist/island/$(FLAVOR)
RUST := /repo/engine/pocket3d/examples/island/3ds/core/target/armv6k-nintendo-3ds/release/libpocket_island_3ds.a
CFLAGS := $(ARCH) -O2 -Wall -Wextra -std=gnu11 -mword-relocations -ffunction-sections -fdata-sections -D__3DS__ -I$(DEVKITPRO)/libctru/include -I$(BACKEND) -I$(SOURCE) -I$(BUILD)
ifeq ($(FLAVOR),capture)
CFLAGS += -DISLAND_CAPTURE
endif
OBJECTS := $(BUILD)/main.o $(BUILD)/pocket3d.o $(BUILD)/color_shbin.o
all: $(BUILD)/pocket-island.3dsx
$(BUILD):
	mkdir -p $@
$(BUILD)/color.shbin: $(BACKEND)/color.v.pica | $(BUILD)
	$(DEVKITPRO)/tools/bin/picasso -o $@ $<
$(BUILD)/color_shbin.s $(BUILD)/color_shbin.h &: $(BUILD)/color.shbin
	cd $(BUILD) && $(DEVKITPRO)/tools/bin/bin2s -a 4 -H color_shbin.h color.shbin > color_shbin.s
$(BUILD)/color_shbin.o: $(BUILD)/color_shbin.s
	$(CC) $(ARCH) -c $< -o $@
$(BUILD)/main.o: $(SOURCE)/main.c $(SOURCE)/island.h $(BACKEND)/pocket3d.h $(BUILD)/color_shbin.h
	$(CC) $(CFLAGS) -c $< -o $@
$(BUILD)/pocket3d.o: $(BACKEND)/pocket3d.c $(BACKEND)/pocket3d.h | $(BUILD)
	$(CC) $(CFLAGS) -c $< -o $@
$(BUILD)/pocket-island.elf: $(OBJECTS) $(RUST)
	$(CC) -specs=3dsx.specs $(ARCH) -Wl,--gc-sections $(OBJECTS) $(RUST) -L$(DEVKITPRO)/libctru/lib -lcitro2d -lcitro3d -lctru -lm -o $@
$(BUILD)/pocket-island.smdh: /repo/hosts/3ds/icon.png | $(BUILD)
	$(DEVKITPRO)/tools/bin/smdhtool --create 'Pocket Island' 'Mira - a little island, together' 'PocketJS' $< $@ /repo/hosts/3ds/icon-small.png
$(BUILD)/pocket-island.3dsx: $(BUILD)/pocket-island.elf $(BUILD)/pocket-island.smdh
	$(DEVKITPRO)/tools/bin/3dsxtool $< $@ --smdh=$(BUILD)/pocket-island.smdh
.PHONY: all
