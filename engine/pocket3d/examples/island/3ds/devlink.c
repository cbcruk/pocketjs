/* The shared authenticated Pocket Runtime wire owns sockets, pairing,
 * backpressure and pixel transport. This adapter owns native app commands. */
#include "devlink.h"
#include "app_js.h"
#include "devserver.h"
#include "soc.h"
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define SCRIPT_ACTIVE POCKET_RUNTIME_APP_ROOT "/active.js"
#define SCRIPT_PREVIOUS POCKET_RUNTIME_APP_ROOT "/previous.js"
#define SCRIPT_PENDING POCKET_RUNTIME_APP_ROOT "/pending.js"
static IslandScript scripts[2], *active;
static JSRuntime *json_runtime;
static JSContext *json;
static char status[96], script_error[192];
static DevserverInitResult link_state;
static PocketRuntimeState runtime_state;
static uint64_t retry_at;
static bool pause_requested, command_ready;
static IslandCommand pending_command;
static struct { float x, z; unsigned flags, left; bool done; char id[64]; } input;
static IslandBenchmark benchmark;
static uint64_t benchmark_deadline;
static IslandSnapshot benchmark_actors[ISLAND_MAX_ACTORS];
void island_dev_benchmark_actors(Island *const *actors, unsigned count) {
  for (unsigned i = 0; i < count && i < ISLAND_MAX_ACTORS; i++) island_snapshot(actors[i], &benchmark_actors[i]);
}
const IslandBenchmark *island_dev_benchmark(void) { return &benchmark; }
void island_dev_benchmark_stop(void) {
  if (!benchmark.enabled) return;
  benchmark.enabled = false;
  benchmark.generation++;
  pause_requested = true;
}

uint64_t island_script_now(void) { return osGetTime(); }
static void number(JSValue obj, const char *key, double value) {
  JS_SetPropertyStr(json, obj, key, JS_NewFloat64(json, value));
}
static void string(JSValue obj, const char *key, const char *value) {
  JS_SetPropertyStr(json, obj, key, JS_NewString(json, value));
}
static JSValue response(const char *type, const char *id) {
  JSValue obj = JS_NewObject(json);
  string(obj, "t", type);
  if (id) string(obj, "id", id);
  return obj;
}
static void send_control(JSValue obj) {
  JSValue value = JS_JSONStringify(json, obj, JS_UNDEFINED, JS_UNDEFINED);
  size_t length;
  const char *text = JS_ToCStringLen(json, &length, value);
  if (text) {
    devserver_send_ctrl(text, length);
    JS_FreeCString(json, text);
  }
  JS_FreeValue(json, value);
  JS_FreeValue(json, obj);
}
static void reply(const char *id, bool ok, const char *message) {
  JSValue obj = response("island.reply", id);
  JS_SetPropertyStr(json, obj, "ok", JS_NewBool(json, ok));
  string(obj, "message", message);
  char hash[17];
  snprintf(hash, sizeof hash, "%016llx", (unsigned long long)active->hash);
  string(obj, "scriptHash", hash);
  number(obj, "generation", runtime_state.generation);
  send_control(obj);
}
static bool text_property(JSValue obj, const char *key, char *out, size_t cap) {
  JSValue value = JS_GetPropertyStr(json, obj, key);
  size_t len = 0;
  const char *text = JS_IsString(value) ? JS_ToCStringLen(json, &len, value) : NULL;
  bool ok = text && len < cap && !memchr(text, 0, len);
  if (ok) memcpy(out, text, len + 1);
  if (text) JS_FreeCString(json, text);
  JS_FreeValue(json, value);
  return ok;
}
static bool numeric_property(JSValue obj, const char *key, double *out, double lo, double hi) {
  JSValue value = JS_GetPropertyStr(json, obj, key);
  bool ok = JS_IsNumber(value) && JS_ToFloat64(json, out, value) == 0 && *out >= lo && *out <= hi;
  JS_FreeValue(json, value);
  return ok;
}
static bool bool_property(JSValue obj, const char *key, bool *out) {
  JSValue value = JS_GetPropertyStr(json, obj, key);
  bool ok = JS_IsBool(value);
  if (ok) *out = JS_ToBool(json, value);
  JS_FreeValue(json, value);
  return ok;
}
static void announce(unsigned frame) {
  runtime_state.active_hash = active->hash;
  devserver_set_runtime(&runtime_state, NULL, "native-script", frame);
}
static bool persist(const char *source, size_t length) {
  FILE *f = fopen(SCRIPT_PENDING, "wb");
  if (!f) return false;
  bool ok = fwrite(source, 1, length, f) == length && fflush(f) == 0 && fsync(fileno(f)) == 0;
  if (fclose(f)) ok = false;
  if (!ok) return false;
  struct stat info;
  if (stat(SCRIPT_PREVIOUS, &info) == 0) {
    if (remove(SCRIPT_PREVIOUS) != 0) return false;
  } else if (errno != ENOENT) return false;
  bool had_active = stat(SCRIPT_ACTIVE, &info) == 0;
  if (!had_active && errno != ENOENT) return false;
  if (had_active && rename(SCRIPT_ACTIVE, SCRIPT_PREVIOUS) != 0) return false;
  if (rename(SCRIPT_PENDING, SCRIPT_ACTIVE) == 0) return true;
  if (had_active) rename(SCRIPT_PREVIOUS, SCRIPT_ACTIVE);
  return false;
}
static bool replace_script(const char *source, size_t length, bool save) {
  // Stable slots keep QuickJS's interrupt opaque pointer alive after swap.
  IslandScript *candidate = active == &scripts[0] ? &scripts[1] : &scripts[0];
  if (!island_script_prepare(candidate, source, length, script_error, sizeof script_error)) return false;
  if (save && !persist(source, length)) {
    island_script_free(candidate);
    snprintf(script_error, sizeof script_error, "SD commit failed; current JavaScript retained");
    return false;
  }
  IslandScript *previous = active;
  active = candidate;
  if (previous) island_script_free(previous);
  runtime_state.generation++;
  pause_requested = true;
  return true;
}
static bool load_file(const char *path) {
  char source[ISLAND_SCRIPT_LIMIT + 1];
  FILE *f = fopen(path, "rb");
  if (!f) return false;
  size_t n = fread(source, 1, sizeof source, f);
  bool ok = !ferror(f) && n <= ISLAND_SCRIPT_LIMIT;
  if (fclose(f)) ok = false;
  return ok && replace_script(source, n, false);
}
bool island_dev_init(void) {
  if (!replace_script((const char *)app_js, app_js_size, false)) return false;
  json_runtime = JS_NewRuntime();
  if (!json_runtime) return false;
  JS_SetMemoryLimit(json_runtime, 2 * 1024 * 1024);
  json = JS_NewContext(json_runtime);
  if (!json) return false;
#ifndef ISLAND_CAPTURE
  mkdir(POCKET_RUNTIME_ROOT, 0777);
  mkdir(POCKET_RUNTIME_APPS, 0777);
  mkdir(POCKET_RUNTIME_APP_ROOT, 0777);
  if (!load_file(SCRIPT_ACTIVE)) load_file(SCRIPT_PREVIOUS);
  devserver_allow_packages(false);
  link_state = devserver_init(&runtime_state, status, sizeof status);
#else
  (void)load_file;
  link_state = DEVSERVER_DISABLED;
#endif
  announce(0);
  return true;
}
const IslandScript *island_dev_script(void) { return active; }
bool island_dev_event(const char *event, int value, unsigned expression,
                      const char *text, IslandCommand *out) {
  bool ok = island_script_event(active, event, value, expression, text, out, script_error, sizeof script_error);
  if (!ok) devserver_report_log("error", script_error);
  return ok;
}
const char *island_dev_status(void) {
  if (link_state == DEVSERVER_DISABLED) return "Dev link: no pairing key";
  if (link_state == DEVSERVER_ERROR) return status;
  DevserverSnapshot s;
  devserver_snapshot(&s);
  snprintf(status, sizeof status, "%s %s:%u", s.connected ? "LINKED" : "READY", s.ip, s.port);
  return status;
}
static void stats(const PerfStats *p, const IslandSnapshot *s, unsigned frame, const char *id) {
  JSValue obj = response("island.stats", id);
  string(obj, "build", ISLAND_BUILD_ID);
  number(obj, "speedupRequested", 1);
  number(obj, "benchmarkEnabled", benchmark.enabled);
  number(obj, "benchmarkGeneration", benchmark.generation);
  number(obj, "measuredBenchmarkGeneration", p->latest.workload_generation);
  number(obj, "actorCount", benchmark.enabled ? benchmark.actors : 1);
  string(obj, "actorMotion", benchmark.enabled ? (benchmark.animated ? "walk" : "frozen") : "player");
  number(obj, "terrainEnabled", !benchmark.enabled || benchmark.terrain);
  number(obj, "linearFreeBytes", linearSpaceFree());
  if (benchmark.enabled) {
    JSValue actors = JS_NewArray(json);
    for (unsigned i = 0; i < benchmark.actors; i++) {
      const IslandSnapshot *actor = &benchmark_actors[i];
      JSValue item = JS_NewObject(json);
      number(item, "x", actor->x); number(item, "z", actor->z);
      number(item, "action", actor->action); number(item, "tick", actor->tick);
      number(item, "actionTime", actor->action_time);
      JS_SetPropertyUint32(json, actors, i, item);
    }
    JS_SetPropertyStr(json, obj, "actors", actors);
  }
  number(obj, "cameraSpan", active->camera.span);
  number(obj, "cameraEyeHeight", active->camera.eye_height);
  number(obj, "cameraDistance", active->camera.distance);
  number(obj, "cameraTargetHeight", active->camera.target_height);
  string(obj, "title", active->title);
  char hash[17];
  snprintf(hash, sizeof hash, "%016llx", (unsigned long long)active->hash);
  string(obj, "scriptHash", hash);
  number(obj, "generation", runtime_state.generation);
  number(obj, "frame", frame); number(obj, "tick", s->tick);
  number(obj, "x", s->x); number(obj, "z", s->z);
  number(obj, "action", s->action); number(obj, "expression", s->expression);
  number(obj, "actionTime", s->action_time);
  number(obj, "messages", s->messages);
  number(obj, "fps", p->latest.fps); number(obj, "frameMs", p->latest.frame_ms);
  number(obj, "p95Ms", p->latest.p95_ms); number(obj, "maxMs", p->latest.max_ms);
  number(obj, "samples", p->latest.frames); number(obj, "elapsedMs", p->latest.elapsed_ms);
  number(obj, "panel", p->latest.panel);
  number(obj, "avatarVertices", p->latest.avatar_vertices);
  number(obj, "terrainVertices", p->latest.terrain_vertices);
  number(obj, "stepsPerFrame", p->latest.steps);
  number(obj, "remoteFramesLeft", input.left);
  const char *names[] = {"updateSkinMs", "uploadMs", "drawUiMs", "endMs", "waitMs", "gpuPreviousMs"};
  for (int i = 0; i < PERF_STAGES; i++) number(obj, names[i], p->latest.stage[i]);
  send_control(obj);
}
static void control(char *line, size_t length, const IslandSnapshot *s, const PerfStats *p, unsigned frame) {
  JSValue obj = JS_ParseJSON(json, line, length, "dev-control");
  if (JS_IsException(obj)) {
    JS_FreeValue(json, JS_GetException(json));
    return;
  }
  char type[32], id[64] = "", event[32], message[193] = "";
  if (!text_property(obj, "t", type, sizeof type)) goto done;
  text_property(obj, "id", id, sizeof id);
  if (!strcmp(type, "island.stats") || !strcmp(type, "devStats")) stats(p, s, frame, id);
  else if (!strcmp(type, "screenshot")) {
    if (!devserver_request_screenshot()) reply(id, false, "Screenshot busy");
  } else if (!strcmp(type, "island.reload")) {
    char source[ISLAND_SCRIPT_LIMIT + 1];
    bool ok = text_property(obj, "source", source, sizeof source);
    if (ok) ok = replace_script(source, strlen(source), true);
    else snprintf(script_error, sizeof script_error, "Source exceeds 8192 bytes or is not a string");
    if (ok) announce(frame);
    reply(id, ok, ok ? "JavaScript replaced; world and chat preserved" : script_error);
  } else if (!strcmp(type, "island.event")) {
    double value = 0;
    numeric_property(obj, "value", &value, 0, 6);
    text_property(obj, "text", message, sizeof message);
    bool ok = !benchmark.enabled && !command_ready && text_property(obj, "event", event, sizeof event)
        && island_dev_event(event, value, s->expression, message, &pending_command);
    if (ok) command_ready = true;
    reply(id, ok, ok ? "Event queued" : "Invalid event or command pending");
  } else if (!strcmp(type, "island.benchmark")) {
    IslandBenchmark candidate = {.generation = benchmark.generation + 1};
    double actors = 0;
    char motion[16];
    bool ok = bool_property(obj, "enabled", &candidate.enabled) && !input.left;
    if (ok && candidate.enabled) {
      ok = numeric_property(obj, "actors", &actors, 0, ISLAND_MAX_ACTORS) && actors == (unsigned)actors &&
        bool_property(obj, "terrain", &candidate.terrain) && text_property(obj, "motion", motion, sizeof motion) &&
        (!strcmp(motion, "walk") || !strcmp(motion, "frozen"));
      if (ok) {
        candidate.actors = actors;
        candidate.animated = !strcmp(motion, "walk");
      }
    }
    if (ok) {
      benchmark = candidate;
      benchmark_deadline = osGetTime() + 90000;
      pause_requested = true;
    }
    reply(id, ok, ok ? "Local load test configured; player state preserved" : "Invalid load test or input tape active");
  } else if (!strcmp(type, "island.input")) {
    double x, z, frames, flags;
    bool ok = !benchmark.enabled && !input.left && !input.done && numeric_property(obj, "x", &x, -1, 1)
        && numeric_property(obj, "z", &z, -1, 1) && numeric_property(obj, "frames", &frames, 1, 120)
        && numeric_property(obj, "flags", &flags, 0, 15) && frames == (unsigned)frames && flags == (unsigned)flags;
    if (ok) {
      input.x = x; input.z = z; input.left = frames; input.flags = flags;
      snprintf(input.id, sizeof input.id, "%s", id);
    }
    reply(id, ok, ok ? "Remote input queued" : "Invalid input or tape already running");
  } else reply(id, false, "Unsupported native control; use island.stats, island.reload, island.event or island.input");
done:
  JS_FreeValue(json, obj);
}
void island_dev_poll(const IslandSnapshot *s, const PerfStats *p, unsigned frame) {
  if (link_state == DEVSERVER_ERROR && osGetTime() >= retry_at) {
    retry_at = osGetTime() + 5000;
    link_state = devserver_init(&runtime_state, status, sizeof status);
  }
  announce(frame);
  devserver_poll();
  if (benchmark.enabled && (!devserver_connected() || osGetTime() >= benchmark_deadline)) island_dev_benchmark_stop();
  if (!devserver_connected()) input.left = input.done = 0;
  if (input.done) {
    stats(p, s, frame, input.id);
    input.done = false;
  }
  static char lines[64 * 1024 + 1];
  size_t length = devserver_recv_ctrl(lines, sizeof lines);
  char *cursor = lines, *end = lines + length;
  while (cursor < end) {
    char *newline = memchr(cursor, '\n', end - cursor);
    if (!newline) break;
    *newline = 0;
    control(cursor, newline - cursor, s, p, frame);
    cursor = newline + 1;
  }
}
bool island_dev_input(float *x, float *z, uint32_t *flags) {
  if (!input.left) return false;
  *x = input.x; *z = input.z; *flags = input.flags;
  input.flags &= 1; // Emotes are edges; running is held.
  if (--input.left == 0) input.done = true;
  return true;
}
bool island_dev_command(IslandCommand *out) {
  if (!command_ready) return false;
  *out = pending_command;
  command_ready = false;
  return true;
}
bool island_dev_pause_requested(void) {
  bool value = pause_requested;
  pause_requested = false;
  return value;
}
bool island_dev_capture(C3D_RenderTarget *top, C3D_RenderTarget *bottom, unsigned frame) {
  if (!devserver_take_screenshot_request()) return false;
  uint8_t *pixels[2];
  if (!devserver_screenshot_begin(frame, 400, 240, 320, 240, &pixels[0], &pixels[1])) return false;
  C3D_RenderTarget *targets[] = {top, bottom};
  unsigned widths[] = {400, 320};
  for (int i = 0; i < 2; i++) {
    C3D_SyncDisplayTransfer(targets[i]->frameBuf.colorBuf,
        GX_BUFFER_DIM(240, widths[i]), (u32 *)pixels[i], GX_BUFFER_DIM(240, widths[i]),
        GX_TRANSFER_IN_FORMAT(GX_TRANSFER_FMT_RGBA8) | GX_TRANSFER_OUT_FORMAT(GX_TRANSFER_FMT_RGB8));
    GSPGPU_InvalidateDataCache(pixels[i], widths[i] * 240 * 3);
  }
  devserver_screenshot_ready();
  return true;
}
void island_dev_shutdown(void) {
  devserver_shutdown();
  soc_shutdown();
  island_script_free(active);
  JS_FreeContext(json);
  JS_FreeRuntime(json_runtime);
}
bool island_dev_self_test(void) {
  uint64_t hash = active->hash;
  const char *bad = "globalThis.islandApp = {";
  if (replace_script(bad, strlen(bad), false) || active->hash != hash) return false;
  const char *loop = "while(true) {}";
  if (replace_script(loop, strlen(loop), false) || active->hash != hash) return false;
  IslandCommand command;
  if (!island_dev_event("wave", 0, 0, "", &command) || command.flags != 2) return false;
  if (!island_dev_event("message", 0, 0, "  hello  ", &command) || strcmp(command.message, "hello")) return false;
  char source[ISLAND_SCRIPT_LIMIT + 1];
  size_t length = app_js_size;
  memcpy(source, app_js, length);
  source[length] = 0;
  char *title = strstr(source, "A little island, together.");
  if (!title) return false;
  memcpy(title, "A linked island, together", 25);
  if (!replace_script(source, length, false) || active->hash == hash) return false;
  return replace_script((const char *)app_js, app_js_size, false) && active->hash == hash;
}
