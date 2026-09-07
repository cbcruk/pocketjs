#ifndef ISLAND_SCRIPT_H
#define ISLAND_SCRIPT_H
#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>
#include "quickjs.h"

#define ISLAND_SCRIPT_LIMIT 8192
typedef struct {
  float span, eye_height, distance, target_height;
} IslandCamera;
typedef struct {
  JSRuntime *runtime;
  JSContext *context;
  JSValue app, handler;
  uint64_t deadline, hash;
  char title[96], room[32], phrases[4][64];
  IslandCamera camera;
} IslandScript;
typedef struct {
  unsigned flags;
  int expression;
  char message[193];
} IslandCommand;
/* Host clock, monotonic milliseconds; no platform API enters the JS layer. */
uint64_t island_script_now(void);
bool island_script_prepare(IslandScript *, const char *, size_t, char *, size_t);
bool island_script_event(IslandScript *, const char *type, int value,
                         unsigned expression, const char *text,
                         IslandCommand *, char *, size_t);
void island_script_free(IslandScript *);
uint64_t island_script_hash(const char *, size_t);
#endif
