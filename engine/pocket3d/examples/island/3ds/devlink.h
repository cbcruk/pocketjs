#ifndef ISLAND_DEVLINK_H
#define ISLAND_DEVLINK_H
#include "island.h"
#include "perf.h"
#include "script.h"
#include <citro3d.h>
#define ISLAND_MAX_ACTORS 8
typedef struct {
  bool enabled, animated, terrain;
  unsigned actors, generation;
} IslandBenchmark;
const IslandBenchmark *island_dev_benchmark(void);
void island_dev_benchmark_stop(void);
void island_dev_benchmark_actors(Island *const *, unsigned count);
bool island_dev_init(void);
void island_dev_shutdown(void);
void island_dev_poll(const IslandSnapshot *, const PerfStats *, unsigned frame);
bool island_dev_input(float *x, float *z, uint32_t *flags);
bool island_dev_command(IslandCommand *out);
bool island_dev_capture(C3D_RenderTarget *top, C3D_RenderTarget *bottom, unsigned frame);
bool island_dev_pause_requested(void);
const IslandScript *island_dev_script(void);
bool island_dev_event(const char *event, int value, unsigned expression,
                      const char *text, IslandCommand *out);
const char *island_dev_status(void);
bool island_dev_self_test(void);
#endif
