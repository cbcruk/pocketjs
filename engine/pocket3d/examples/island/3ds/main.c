#include "color_shbin.h"
#include "island.h"
#include "pocket3d.h"
#include <3ds.h>
#include <citro2d.h>
#include <math.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
unsigned int __stacksize__ = 1024 * 1024;
static C3D_RenderTarget *top, *bottom;
static C2D_TextBuf textbuf;
static Island *island;
static IslandSnapshot state;
static P3D_Mesh terrain, avatar[2];
static int tab = 0;
static const char *expressions[] = {"Calm",  "Happy", "Sad",   "Wow!",
                                    "Angry", "Shy",   "Sleepy"};
static const char *actions[] = {
    "Enjoying the breeze", "Taking a walk",    "Running",
    "Sitting down",        "Taking a break",   "Getting up",
    "Waving hello",        "Feeling wonderful"};
static const char *phrases[] = {"Hello, island!", "Let's take a walk.",
                                "This is my happy place.",
                                "See you by the sea!"};
static char notice[64] = "Touch a phrase to say hello.";
static uint32_t ink, paper, muted, mint, accent, linecol;
static void rect(float x, float y, float w, float h, uint32_t c) {
  C2D_DrawRectSolid(x, y, .1, w, h, c);
}
static void roundrect(float x, float y, float w, float h, float r, uint32_t c) {
  // Solid triangles avoid procedural-texture state during 3D/UI switches.
  float px[28], py[28];
  for (int corner = 0; corner < 4; corner++) {
    float cx = x + (corner == 0 || corner == 3 ? r : w - r),
          cy = y + (corner < 2 ? r : h - r);
    for (int j = 0; j < 7; j++) {
      float a = (180 + corner * 90 + j * 15) * M_PI / 180.0f;
      px[corner * 7 + j] = cx + cosf(a) * r;
      py[corner * 7 + j] = cy + sinf(a) * r;
    }
  }
  for (int i = 0; i < 28; i++) {
    int j = (i + 1) % 28;
    C2D_DrawTriangle(x + w / 2, y + h / 2, c, px[i], py[i], c, px[j], py[j], c,
                     .1);
  }
}
static void text(float x, float y, float scale, uint32_t color, const char *s) {
  C2D_Text t;
  C2D_TextParse(&t, textbuf, s);
  C2D_TextOptimize(&t);
  C2D_DrawText(&t, C2D_WithColor, x, y, .2, scale, scale, color);
}
static void centered(float x, float y, float scale, uint32_t color,
                     const char *s) {
  C2D_Text t;
  float w;
  C2D_TextParse(&t, textbuf, s);
  C2D_TextGetDimensions(&t, scale, scale, &w, NULL);
  C2D_DrawText(&t, C2D_WithColor, x - w / 2, y, .2, scale, scale, color);
}
/* Wrap on UTF-8 codepoint boundaries, with explicit line limits. System font
 * supplies Japanese/CJK glyphs where installed; unsupported glyphs use its
 * fallback. */
static void wrapped(float x, float y, float scale, float width,
                    unsigned maxlines, const char *s, uint32_t color) {
  char row[196];
  size_t used = 0;
  unsigned lines = 0;
  while (*s && lines < maxlines) {
    unsigned char lead = (unsigned char)*s;
    size_t n = lead < 0x80 ? 1 : (lead < 0xe0 ? 2 : (lead < 0xf0 ? 3 : 4));
    if (used + n >= sizeof row)
      break;
    memcpy(row + used, s, n);
    row[used + n] = 0;
    C2D_Text t;
    float w;
    C2D_TextParse(&t, textbuf, row);
    C2D_TextGetDimensions(&t, scale, scale, &w, NULL);
    if (w > width && used > 0) {
      row[used] = 0;
      text(x, y + lines * 14, scale, color, row);
      lines++;
      used = 0;
      continue;
    }
    used += n;
    s += n;
  }
  if (used && lines < maxlines)
    text(x, y + lines * 14, scale, color, row);
}
static void send_text(const char *s) {
  int result = island_send(island, (const uint8_t *)s, strlen(s));
  snprintf(notice, sizeof notice, "%s",
           result == 0    ? "Said in this local room."
           : result == -2 ? "One moment before your next message."
                          : "Use 1 to 192 UTF-8 bytes of text.");
}
static void keyboard(void) {
  static SwkbdState keyboard;
  char output[193] = {0};
  swkbdInit(&keyboard, SWKBD_TYPE_NORMAL, 2, 96);
  swkbdSetHintText(&keyboard, "Say something to the island");
  swkbdSetValidation(&keyboard, SWKBD_NOTEMPTY_NOTBLANK, 0, 0);
  swkbdSetButton(&keyboard, SWKBD_BUTTON_LEFT, "Cancel", false);
  swkbdSetButton(&keyboard, SWKBD_BUTTON_RIGHT, "Say", true);
  if (swkbdInputText(&keyboard, output, sizeof output) == SWKBD_BUTTON_RIGHT)
    send_text(output);
}
static void top_ui(void) {
  C2D_Prepare();
  C2D_SceneBegin(top);
  C3D_DepthTest(false, GPU_ALWAYS, GPU_WRITE_COLOR);
  C3D_AlphaBlend(GPU_BLEND_ADD, GPU_BLEND_ADD, GPU_SRC_ALPHA,
                 GPU_ONE_MINUS_SRC_ALPHA, GPU_ONE, GPU_ONE_MINUS_SRC_ALPHA);
  roundrect(10, 10, 126, 31, 9, paper);
  text(19, 14, .46, ink, "POCKET ISLAND");
  roundrect(306, 10, 84, 23, 8, paper);
  text(316, 14, .36, muted, "LOCAL ROOM");
  // A readable ground shadow roots the avatar in the 3D scene.
  char message[193];
  if (island_bubble(island, (uint8_t *)message, sizeof message)) {
    const float pixels = 400.0f / 10.8f;
    float x = 200 + (state.anchor_x - state.cam_x) * pixels;
    float y = 120 - ((state.anchor_y - .60f) * .7808688f -
                     (state.anchor_z - state.cam_z) * .624695f) *
                        pixels;
    bool right = x <= 200;
    float bx = fmaxf(8, fminf(226, right ? x + 22 : x - 188)),
          by = fmaxf(43, fminf(154, y - 16));
    roundrect(bx + 1, by + 2, 166, 48, 9, C2D_Color32(48, 77, 63, 60));
    roundrect(bx, by, 166, 48, 9, paper);
    float edge = right ? bx : bx + 166;
    C2D_DrawTriangle(edge, by + 20, paper, edge, by + 33, paper, x, y + 22,
                     paper, .1);
    text(bx + 10, by + 4, .34, accent, "Mira");
    wrapped(bx + 10, by + 18, .40, 146, 2, message, ink);
  }
  roundrect(10, 211, 178, 21, 7, paper);
  text(18, 214, .36, muted, actions[state.action]);
  text(268, 217, .34, ink, "Circle Pad + B to run");
}
static void bottom_ui(void) {
  C2D_Prepare();
  C2D_SceneBegin(bottom);
  C3D_DepthTest(false, GPU_ALWAYS, GPU_WRITE_COLOR);
  C3D_AlphaBlend(GPU_BLEND_ADD, GPU_BLEND_ADD, GPU_SRC_ALPHA,
                 GPU_ONE_MINUS_SRC_ALPHA, GPU_ONE, GPU_ONE_MINUS_SRC_ALPHA);
  rect(0, 0, 320, 240, paper);
  text(15, 9, .72, ink, "A little island, together.");
  text(16, 34, .37, muted, "MIRA  /  1 visitor  /  local demo");
  for (int i = 0; i < 2; i++) {
    roundrect(14 + i * 149, 56, 143, 27, 8, i == tab ? mint : linecol);
    centered(85 + i * 149, 61, .45, i == tab ? paper : muted,
             i == 0 ? "Conversation" : "Expressions");
  }
  if (tab == 0) {
    if (state.messages) {
      char msg[193];
      island_message(island, 0, (uint8_t *)msg, sizeof msg);
      text(17, 91, .34, accent, "Mira  -  local");
      wrapped(17, 105, .40, 283, 2, msg, ink);
    } else {
      text(17, 93, .42, muted, "Your words appear beside Mira.");
      text(17, 111, .36, muted, "Use Y for the keyboard, or a phrase below.");
    }
    for (int i = 0; i < 4; i++) {
      float x = 14 + (i % 2) * 149, y = 139 + (i / 2) * 30;
      roundrect(x, y, 143, 25, 7, linecol);
      text(x + 8, y + 5, .34, ink, phrases[i]);
    }
    roundrect(14, 202, 292, 27, 8, mint);
    centered(160, 207, .45, paper, "Y   Write a message");
  } else {
    for (int i = 0; i < 7; i++) {
      float x = 14 + (i % 4) * 74, y = 94 + (i / 4) * 32;
      roundrect(x, y, 69, 27, 7,
                state.expression == (uint32_t)i ? accent : linecol);
      centered(x + 34, y + 5, .39,
               state.expression == (uint32_t)i ? paper : ink, expressions[i]);
    }
    const char *labels[] = {"A  Wave", "X  Sit / stand", "Cheer"};
    for (int i = 0; i < 3; i++) {
      float x = 14 + i * 99;
      roundrect(x, 166, 94, 29, 8, mint);
      centered(x + 47, 173, .36, paper, labels[i]);
    }
    text(17, 207, .37, muted, "L / R changes expression while you move.");
  }
  // Feedback only replaces the subtitle, preserving touch target geometry.
  if (strcmp(notice, "Touch a phrase to say hello.")) {
    rect(0, 33, 320, 17, paper);
    text(16, 34, .34, muted, notice);
  }
}
static uint32_t touch_action(touchPosition p) {
  if (p.py >= 56 && p.py < 83) {
    tab = p.px >= 163;
    return 0;
  }
  if (tab == 0) {
    if (p.py >= 139 && p.py < 194 && p.px >= 14 && p.px < 306) {
      int col = p.px >= 163, row = (p.py - 139) / 30;
      if (row < 2)
        send_text(phrases[row * 2 + col]);
    } else if (p.py >= 202 && p.py < 230)
      keyboard();
  } else {
    if (p.py >= 94 && p.py < 153 && p.px >= 14) {
      int i = (p.py - 94) / 32 * 4 + (p.px - 14) / 74;
      if (i < 7)
        island_expression(island, i);
    }
    if (p.py >= 166 && p.py < 195 && p.px >= 14 && p.px < 306) {
      int i = (p.px - 14) / 99;
      return i == 0 ? 2 : i == 1 ? 4 : 8;
    }
  }
  return 0;
}
#ifdef ISLAND_CAPTURE
static uint8_t *capture;
static bool capture_frame(unsigned frame) {
  return frame == 1 || frame == 31 || frame == 61 || frame == 91 ||
         frame == 121 || frame == 151 || frame == 181 || frame == 211 ||
         frame == 241 || frame == 301;
}
static bool dump(C3D_RenderTarget *target, unsigned width, unsigned frame,
                 const char *name) {
  C3D_SyncDisplayTransfer((u32 *)target->frameBuf.colorBuf,
                          GX_BUFFER_DIM(240, width), (u32 *)capture,
                          GX_BUFFER_DIM(240, width),
                          GX_TRANSFER_IN_FORMAT(GX_TRANSFER_FMT_RGBA8) |
                              GX_TRANSFER_OUT_FORMAT(GX_TRANSFER_FMT_RGB8));
  GSPGPU_InvalidateDataCache(capture, width * 240 * 3);
  char path[100];
  snprintf(path, sizeof path, "sdmc:/pocket-island/%s-%03u.bgr", name, frame);
  FILE *f = fopen(path, "wb");
  if (!f)
    return false;
  bool ok = fwrite(capture, 1, width * 240 * 3, f) == width * 240 * 3;
  return fclose(f) == 0 && ok;
}
#endif
int main(void) {
  gfxInitDefault();
  gfxSet3D(false);
  if (!C3D_Init(C3D_DEFAULT_CMDBUF_SIZE * 2) || !C2D_Init(4096))
    return 1;
  C2D_Prepare();
  top = C2D_CreateScreenTarget(GFX_TOP, GFX_LEFT);
  bottom = C2D_CreateScreenTarget(GFX_BOTTOM, GFX_LEFT);
  textbuf = C2D_TextBufNew(8192);
  if (!textbuf || !top || !bottom)
    return 2;
  ink = C2D_Color32(48, 68, 54, 255);
  paper = C2D_Color32(255, 250, 232, 255);
  muted = C2D_Color32(108, 120, 98, 255);
  mint = C2D_Color32(74, 126, 103, 255);
  accent = C2D_Color32(183, 113, 46, 255);
  linecol = C2D_Color32(235, 233, 210, 255);
  island = island_new();
  if (!island || !p3d_init(color_shbin, color_shbin_size))
    return 3;
  uint32_t n;
  const P3D_ColorVertex *v = island_vertices(island, true, &n);
  if (!p3d_mesh_create(&terrain, n) || !p3d_mesh_upload(&terrain, v, n) ||
      !p3d_mesh_create(&avatar[0], 60000) ||
      !p3d_mesh_create(&avatar[1], 60000))
    return 4;
#ifdef ISLAND_CAPTURE
  mkdir("sdmc:/pocket-island", 0777);
  capture = linearAlloc(400 * 240 * 3);
  if (!capture)
    return 5;
  FILE *receipt = fopen("sdmc:/pocket-island/receipt.jsonl", "wb");
  if (!receipt)
    return 6;
#endif
  unsigned frame = 0;
  uint32_t pending_actions = 0;
  uint64_t last = osGetTime();
  double accumulator = 0;
  while (aptMainLoop()) {
    hidScanInput();
    u32 down = hidKeysDown(), held = hidKeysHeld();
    if (down & KEY_START)
      break;
    circlePosition pad;
    hidCircleRead(&pad);
    float x = pad.dx / 156.f, z = -pad.dy / 156.f;
    if (held & KEY_DLEFT)
      x = -1;
    if (held & KEY_DRIGHT)
      x = 1;
    if (held & KEY_DUP)
      z = -1;
    if (held & KEY_DDOWN)
      z = 1;
    uint32_t flags = (held & KEY_B ? 1 : 0) | (down & KEY_A ? 2 : 0) |
                     (down & KEY_X ? 4 : 0);
    island_snapshot(island, &state);
    if (down & KEY_R)
      island_expression(island, (state.expression + 1) % 7);
    if (down & KEY_L)
      island_expression(island, (state.expression + 6) % 7);
    if (down & KEY_Y) {
      keyboard();
      last = osGetTime();
    }
    if (down & KEY_TOUCH) {
      touchPosition p;
      hidTouchRead(&p);
      flags |= touch_action(p);
      last = osGetTime();
    }
    uint64_t now = osGetTime();
    accumulator += fmin((now - last) / 1000., .10);
    last = now;
#ifdef ISLAND_CAPTURE
    accumulator = 1. / 30.;
    x = z = 0;
    flags = 0;
    if (frame >= 15 && frame < 45)
      x = 1;
    if (frame >= 45 && frame < 70) {
      x = -1;
      flags = 1;
    }
    if (frame == 75)
      flags = 2;
    if (frame == 110) {
      island_expression(island, 1);
      touch_action((touchPosition){.px = 40, .py = 150});
    }
    if (frame == 145)
      flags = 4;
    if (frame == 210)
      flags = 4;
    if (frame == 250) {
      island_expression(island, 3);
      flags = 8;
    }
    if (frame == 280) {
      touch_action((touchPosition){.px = 220, .py = 65});
      touch_action((touchPosition){.px = 100, .py = 139});
    }
#endif
    // Inputs are edge commands; consume once even if catch-up needs two turns.
    pending_actions |= flags & ~1u;
    while (accumulator >= 1. / 30.) {
      island_step(island, x, z, (flags & 1) | pending_actions);
      pending_actions = 0;
      accumulator -= 1. / 30.;
    }
    island_snapshot(island, &state);
    v = island_vertices(island, false, &n);
#ifdef ISLAND_CAPTURE
    // Every logical turn is simulated; only selected poses submit a GPU frame.
    // Readback tests need pixel receipts, not real-time video playback.
    if (!capture_frame(frame + 1)) {
      frame++;
      continue;
    }
#endif
    // FrameBegin waits for the preceding GPU submission before slot reuse.
    if (!C3D_FrameBegin(C3D_FRAME_SYNCDRAW))
      continue;
    if (!p3d_mesh_upload(&avatar[frame % 2], v, n))
      break;
    C2D_TextBufClear(textbuf);
    C3D_RenderTargetClear(top, C3D_CLEAR_ALL, 0x83cbcaff, 0);
    C3D_RenderTargetClear(bottom, C3D_CLEAR_ALL, 0xfffae8ff, 0);
    C3D_FrameDrawOn(top);
    C3D_Mtx projection, view, vp;
    Mtx_OrthoTilt(&projection, -5.4, 5.4, -3.24, 3.24, .1, 100, false);
    C3D_FVec eye = FVec3_New(state.cam_x, 8.60, state.cam_z + 10),
             target = FVec3_New(state.cam_x, .60, state.cam_z),
             up = FVec3_New(0, 1, 0);
    Mtx_LookAt(&view, eye, target, up, false);
    Mtx_Multiply(&vp, &projection, &view);
    p3d_begin(&vp);
    p3d_draw(&terrain);
    p3d_draw(&avatar[frame % 2]);
    top_ui();
    bottom_ui();
    C2D_Flush();
    C3D_FrameEnd(0);
    frame++;
#ifdef ISLAND_CAPTURE
    if (capture_frame(frame)) {
      gspWaitForVBlank();
      if (!dump(top, 400, frame, "top") || !dump(bottom, 320, frame, "bottom"))
        break;
      fprintf(receipt,
              "{\"frame\":%u,\"tick\":%u,\"x\":%.4f,\"z\":%.4f,\"action\":%u,"
              "\"expression\":%u,\"messages\":%u,\"vertices\":%u}\n",
              frame, (unsigned)state.tick, state.x, state.z,
              (unsigned)state.action, (unsigned)state.expression,
              (unsigned)state.messages, (unsigned)n);
      fflush(receipt);
    }
    if (frame == 301) {
      fclose(receipt);
      receipt = NULL;
      FILE *f = fopen("sdmc:/pocket-island/done", "wb");
      if (f) {
        fputs("ok\n", f);
        fclose(f);
      }
      break;
    }
#endif
  }
#ifdef ISLAND_CAPTURE
  if (receipt)
    fclose(receipt);
  linearFree(capture);
#endif
  p3d_mesh_free(&terrain);
  p3d_mesh_free(&avatar[0]);
  p3d_mesh_free(&avatar[1]);
  p3d_exit();
  island_free(island);
  C2D_TextBufDelete(textbuf);
  C2D_Fini();
  C3D_Fini();
  gfxExit();
  return 0;
}
