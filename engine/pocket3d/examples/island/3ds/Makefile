DEVKITPRO ?= /opt/devkitpro
DEVKITARM ?= $(DEVKITPRO)/devkitARM
CC := $(DEVKITARM)/bin/arm-none-eabi-gcc
ARCH := -march=armv6k -mtune=mpcore -mfloat-abi=hard -mtp=soft
BACKEND := /repo/engine/pocket3d/backends/citro3d
SOURCE := /repo/engine/pocket3d/examples/island/3ds
HOST := /repo/hosts/3ds
QUICKJS := /repo/dist/island/quickjs
BUILD := /repo/dist/island/$(FLAVOR)
RUST := /repo/engine/pocket3d/examples/island/3ds/core/target/armv6k-nintendo-3ds/release/libpocket_island_3ds.a
CFLAGS := $(ARCH) -O2 -Wall -Wextra -std=gnu11 -mword-relocations -ffunction-sections -fdata-sections -D__3DS__ -I$(DEVKITPRO)/libctru/include -I$(BACKEND) -I$(SOURCE) -I$(BUILD)
BUILD_ID ?= unknown
CFLAGS += -DISLAND_BUILD_ID=\"$(BUILD_ID)\"
# ABI 0 identifies a native Pocket3D host, not a PocketJS guest ABI.
CFLAGS += -DJS_NO_NAN_BOXING -DPOCKETJS_HOST_ABI=0 -DPOCKETJS_TARGET_ID=\"p3d-island\" -DPOCKETJS_RUNTIME_SLOT=\"pocket-island-native\" -I$(HOST)/src -I$(HOST)/include -isystem $(QUICKJS)
ifeq ($(FLAVOR),capture)
CFLAGS += -DISLAND_CAPTURE
endif
OBJECTS := $(BUILD)/main.o $(BUILD)/pocket3d.o $(BUILD)/color_shbin.o $(BUILD)/skin_shbin.o $(BUILD)/script.o $(BUILD)/devlink.o $(BUILD)/devserver.o $(BUILD)/dev_protocol.o $(BUILD)/soc.o $(BUILD)/app_js.o
all: $(BUILD)/pocket-island.3dsx
$(BUILD):
	mkdir -p $@
$(BUILD)/color.shbin: $(BACKEND)/color.v.pica | $(BUILD)
	$(DEVKITPRO)/tools/bin/picasso -o $@ $<
$(BUILD)/color_shbin.s $(BUILD)/color_shbin.h &: $(BUILD)/color.shbin
	cd $(BUILD) && $(DEVKITPRO)/tools/bin/bin2s -a 4 -H color_shbin.h color.shbin > color_shbin.s
$(BUILD)/color_shbin.o: $(BUILD)/color_shbin.s
	$(CC) $(ARCH) -c $< -o $@
$(BUILD)/skin.shbin: $(BACKEND)/skin.v.pica | $(BUILD)
	$(DEVKITPRO)/tools/bin/picasso -o $@ $<
$(BUILD)/skin_shbin.s $(BUILD)/skin_shbin.h &: $(BUILD)/skin.shbin
	cd $(BUILD) && $(DEVKITPRO)/tools/bin/bin2s -a 4 -H skin_shbin.h skin.shbin > skin_shbin.s
$(BUILD)/skin_shbin.o: $(BUILD)/skin_shbin.s
	$(CC) $(ARCH) -c $< -o $@
$(BUILD)/app_js.s $(BUILD)/app_js.h &: $(SOURCE)/../app.js | $(BUILD)
	cd $(BUILD) && $(DEVKITPRO)/tools/bin/bin2s -a 4 -H app_js.h $< > app_js.s
$(BUILD)/app_js.o: $(BUILD)/app_js.s
	$(CC) $(ARCH) -c $< -o $@
$(BUILD)/script.o: $(SOURCE)/script.c $(SOURCE)/script.h | $(BUILD)
	$(CC) $(CFLAGS) -c $< -o $@
$(BUILD)/devlink.o: $(SOURCE)/devlink.c $(SOURCE)/devlink.h $(SOURCE)/script.h $(SOURCE)/perf.h $(BUILD)/app_js.h FORCE
	$(CC) $(CFLAGS) -c $< -o $@
$(BUILD)/devserver.o: $(HOST)/src/devserver.c $(HOST)/src/devserver.h FORCE
	$(CC) $(CFLAGS) -c $< -o $@
$(BUILD)/dev_protocol.o: $(HOST)/src/dev_protocol.c $(HOST)/src/dev_protocol.h | $(BUILD)
	$(CC) $(CFLAGS) -c $< -o $@
$(BUILD)/soc.o: $(HOST)/src/soc.c $(HOST)/src/soc.h | $(BUILD)
	$(CC) $(CFLAGS) -c $< -o $@
$(BUILD)/main.o: $(SOURCE)/main.c $(SOURCE)/island.h $(SOURCE)/perf.h $(SOURCE)/view.h $(SOURCE)/devlink.h $(BACKEND)/pocket3d.h $(BUILD)/color_shbin.h $(BUILD)/skin_shbin.h FORCE
	$(CC) $(CFLAGS) -c $< -o $@
$(BUILD)/pocket3d.o: $(BACKEND)/pocket3d.c $(BACKEND)/pocket3d.h | $(BUILD)
	$(CC) $(CFLAGS) -c $< -o $@
$(BUILD)/pocket-island.elf: $(OBJECTS) $(RUST) $(QUICKJS)/libquickjs.a
	$(CC) -specs=3dsx.specs $(ARCH) -Wl,--gc-sections $(OBJECTS) $(RUST) $(QUICKJS)/libquickjs.a -L$(DEVKITPRO)/libctru/lib -lcitro2d -lcitro3d -lctru -lm -o $@
$(BUILD)/pocket-island.smdh: /repo/hosts/3ds/icon.png | $(BUILD)
	$(DEVKITPRO)/tools/bin/smdhtool --create 'Pocket Island' 'Mira - a little island, together' 'PocketJS' $< $@ /repo/hosts/3ds/icon-small.png
$(BUILD)/pocket-island.3dsx: $(BUILD)/pocket-island.elf $(BUILD)/pocket-island.smdh
	$(DEVKITPRO)/tools/bin/3dsxtool $< $@ --smdh=$(BUILD)/pocket-island.smdh
.PHONY: all FORCE
FORCE:
