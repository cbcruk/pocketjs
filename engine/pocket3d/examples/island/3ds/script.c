#include "script.h"
#include <math.h>
#include <stdio.h>
#include <string.h>

static int interrupt(JSRuntime *rt, void *opaque) {
  (void)rt;
  return island_script_now() > ((IslandScript *)opaque)->deadline;
}
static void budget(IslandScript *s) { s->deadline = island_script_now() + 50; }
static bool exception(IslandScript *s, char *error, size_t size) {
  JSValue ex = JS_GetException(s->context);
  const char *text = JS_ToCString(s->context, ex);
  snprintf(error, size, "%s", text ? text : "JavaScript exception");
  if (text) JS_FreeCString(s->context, text);
  JS_FreeValue(s->context, ex);
  return false;
}
static bool string_value(JSContext *ctx, JSValueConst v, char *out, size_t cap) {
  if (!JS_IsString(v)) return false;
  size_t len;
  const char *text = JS_ToCStringLen(ctx, &len, v);
  if (!text) return false;
  bool ok = len < cap && !memchr(text, 0, len);
  if (ok) memcpy(out, text, len + 1);
  JS_FreeCString(ctx, text);
  return ok;
}
static bool property_text(JSContext *ctx, JSValueConst obj, const char *key,
                          char *out, size_t cap) {
  JSValue v = JS_GetPropertyStr(ctx, obj, key);
  bool ok = string_value(ctx, v, out, cap);
  JS_FreeValue(ctx, v);
  return ok;
}
static bool camera_number(JSContext *ctx, JSValueConst obj, const char *key,
                           float *out, double low, double high) {
  JSValue v = JS_GetPropertyStr(ctx, obj, key);
  double n;
  bool ok = JS_IsNumber(v) && JS_ToFloat64(ctx, &n, v) == 0 &&
            isfinite(n) && n >= low && n <= high;
  if (ok) *out = n;
  JS_FreeValue(ctx, v);
  return ok;
}
uint64_t island_script_hash(const char *source, size_t length) {
  uint64_t h = UINT64_C(14695981039346656037);
  for (size_t i = 0; i < length; i++) h = (h ^ (unsigned char)source[i]) * UINT64_C(1099511628211);
  return h;
}
void island_script_free(IslandScript *s) {
  if (s->context) {
    JS_FreeValue(s->context, s->handler);
    JS_FreeValue(s->context, s->app);
    JS_FreeContext(s->context);
  }
  if (s->runtime) JS_FreeRuntime(s->runtime);
  memset(s, 0, sizeof *s);
}
bool island_script_event(IslandScript *s, const char *type, int value,
                         unsigned expression, const char *text,
                         IslandCommand *out, char *error, size_t size) {
  *out = (IslandCommand){ .expression = -1 };
  budget(s);
  JSContext *ctx = s->context;
  JSValue event = JS_NewObject(ctx);
  JS_SetPropertyStr(ctx, event, "type", JS_NewString(ctx, type));
  JS_SetPropertyStr(ctx, event, "value", JS_NewInt32(ctx, value));
  JS_SetPropertyStr(ctx, event, "expression", JS_NewUint32(ctx, expression));
  JS_SetPropertyStr(ctx, event, "text", JS_NewString(ctx, text ? text : ""));
  JSValue result = JS_Call(ctx, s->handler, s->app, 1, &event);
  JS_FreeValue(ctx, event);
  if (JS_IsException(result)) return exception(s, error, size);
  bool ok = JS_IsObject(result);
  JSValue flags = JS_GetPropertyStr(ctx, result, "flags");
  JSValue face = JS_GetPropertyStr(ctx, result, "expression");
  JSValue message = JS_GetPropertyStr(ctx, result, "message");
  int32_t n;
  if (!JS_IsUndefined(flags)) {
    ok = ok && JS_IsNumber(flags) && JS_ToInt32(ctx, &n, flags) == 0 && n >= 0 && (n & ~14) == 0;
    if (ok) out->flags = n;
  }
  if (!JS_IsUndefined(face)) {
    ok = ok && JS_IsNumber(face) && JS_ToInt32(ctx, &n, face) == 0 && n >= 0 && n < 7;
    if (ok) out->expression = n;
  }
  if (!JS_IsUndefined(message)) ok = ok && string_value(ctx, message, out->message, sizeof out->message);
  JS_FreeValue(ctx, flags);
  JS_FreeValue(ctx, face);
  JS_FreeValue(ctx, message);
  JS_FreeValue(ctx, result);
  if (!ok) {
    *out = (IslandCommand){ .expression = -1 };
    snprintf(error, size, "handle must return bounded flags, expression and message fields");
  }
  return ok;
}
bool island_script_prepare(IslandScript *s, const char *source, size_t length,
                           char *error, size_t size) {
  memset(s, 0, sizeof *s);
  s->app = s->handler = JS_UNDEFINED;
  if (!length || length > ISLAND_SCRIPT_LIMIT || memchr(source, 0, length)) {
    snprintf(error, size, "JavaScript source must contain 1 to 8192 bytes without NUL");
    return false;
  }
  s->runtime = JS_NewRuntime();
  if (!s->runtime) goto invalid;
  JS_SetMemoryLimit(s->runtime, 4 * 1024 * 1024);
  JS_SetMaxStackSize(s->runtime, 128 * 1024);
  JS_SetInterruptHandler(s->runtime, interrupt, s);
  s->context = JS_NewContext(s->runtime);
  if (!s->context) goto invalid;
  budget(s);
  char terminated[ISLAND_SCRIPT_LIMIT + 1];
  memcpy(terminated, source, length);
  terminated[length] = 0;
  JSValue value = JS_Eval(s->context, terminated, length, "island-app.js", JS_EVAL_TYPE_GLOBAL);
  if (JS_IsException(value)) {
    exception(s, error, size);
    island_script_free(s);
    return false;
  }
  JS_FreeValue(s->context, value);
  JSValue global = JS_GetGlobalObject(s->context);
  s->app = JS_GetPropertyStr(s->context, global, "islandApp");
  JS_FreeValue(s->context, global);
  s->handler = JS_GetPropertyStr(s->context, s->app, "handle");
  JSValue version = JS_GetPropertyStr(s->context, s->app, "version");
  int32_t n = 0;
  bool ok = JS_ToInt32(s->context, &n, version) == 0 && n == 1 && JS_IsFunction(s->context, s->handler);
  JS_FreeValue(s->context, version);
  ok = ok && property_text(s->context, s->app, "title", s->title, sizeof s->title);
  ok = ok && property_text(s->context, s->app, "room", s->room, sizeof s->room);
  JSValue phrases = JS_GetPropertyStr(s->context, s->app, "phrases");
  for (int i = 0; i < 4; i++) {
    JSValue phrase = JS_GetPropertyUint32(s->context, phrases, i);
    ok = ok && string_value(s->context, phrase, s->phrases[i], sizeof s->phrases[i]);
    JS_FreeValue(s->context, phrase);
  }
  JS_FreeValue(s->context, phrases);
  // Older v1 scripts retain the close view; explicit camera edits validate
  // in the candidate context before the host replaces the active script.
  s->camera = (IslandCamera){7.2f, 6.6f, 10.f, .8f};
  JSValue camera = JS_GetPropertyStr(s->context, s->app, "camera");
  if (!JS_IsUndefined(camera)) {
    ok = ok && JS_IsObject(camera) &&
      camera_number(s->context, camera, "span", &s->camera.span, 5, 16) &&
      camera_number(s->context, camera, "eyeHeight", &s->camera.eye_height, 3, 14) &&
      camera_number(s->context, camera, "distance", &s->camera.distance, 6, 20) &&
      camera_number(s->context, camera, "targetHeight", &s->camera.target_height, .4, 1.8);
  }
  JS_FreeValue(s->context, camera);
  IslandCommand command;
  if (!ok || !island_script_event(s, "validate", 0, 0, "", &command, error, size)) goto invalid;
  s->hash = island_script_hash(source, length);
  return true;
invalid:
  snprintf(error, size, "Invalid islandApp v1, handler, labels, phrases or bounded camera");
  island_script_free(s);
  return false;
}
