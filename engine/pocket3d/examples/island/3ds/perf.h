#ifndef ISLAND_PERF_H
#define ISLAND_PERF_H

#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/* Bounded, in-memory measurements. No SD writes or allocations per frame.
 * Frame time includes GPU/vblank waiting; GPU queue time overlaps CPU work
 * and must not be added to it. The host supplies monotonic milliseconds. */
enum { PERF_WINDOW_FRAMES = 256, PERF_HISTORY = 180, PERF_STAGES = 6 };
enum { PERF_UPDATE, PERF_UPLOAD, PERF_DRAW_UI, PERF_END, PERF_WAIT, PERF_GPU };
typedef struct {
  double elapsed_ms;
  float fps, frame_ms, p95_ms, max_ms, stage[PERF_STAGES], steps;
  uint32_t frames, avatar_vertices, terrain_vertices, action, panel, workload_generation;
} PerfRow;
typedef struct {
  PerfRow latest, history[PERF_HISTORY];
  unsigned head, count, frames, gpu_frames;
  unsigned workload_generation;
  double elapsed_ms, window_ms, sum[PERF_STAGES], steps;
  float durations[PERF_WINDOW_FRAMES];
} PerfStats;

static inline int perf_compare(const void *a, const void *b) {
  float x = *(const float *)a, y = *(const float *)b;
  return (x > y) - (x < y);
}
static inline void perf_reset_window(PerfStats *p) {
  p->frames = p->gpu_frames = 0;
  p->window_ms = p->steps = 0;
  memset(p->sum, 0, sizeof p->sum);
}
static inline bool perf_record(PerfStats *p, float frame_ms,
                               const float stage[PERF_STAGES], bool gpu_valid,
                               unsigned steps, unsigned avatar, unsigned terrain,
                               unsigned action, bool panel) {
  if (!(frame_ms > 0)) return false;
  p->durations[p->frames++] = frame_ms;
  p->window_ms += frame_ms;
  p->elapsed_ms += frame_ms;
  p->steps += steps;
  for (unsigned i = 0; i < PERF_GPU; i++) p->sum[i] += stage[i];
  if (gpu_valid) {
    p->sum[PERF_GPU] += stage[PERF_GPU];
    p->gpu_frames++;
  }
  if (p->window_ms < 1000 && p->frames < PERF_WINDOW_FRAMES) return false;
  qsort(p->durations, p->frames, sizeof(float), perf_compare);
  PerfRow row = {
      .elapsed_ms = p->elapsed_ms,
      .fps = 1000.0 * p->frames / p->window_ms,
      .frame_ms = p->window_ms / p->frames,
      .p95_ms = p->durations[(p->frames * 95 + 99) / 100 - 1],
      .max_ms = p->durations[p->frames - 1],
      .steps = p->steps / p->frames,
      .frames = p->frames,
      .avatar_vertices = avatar, .terrain_vertices = terrain,
      .action = action, .panel = panel,
      .workload_generation = p->workload_generation,
  };
  for (unsigned i = 0; i < PERF_GPU; i++) row.stage[i] = p->sum[i] / p->frames;
  row.stage[PERF_GPU] = p->gpu_frames ? p->sum[PERF_GPU] / p->gpu_frames : 0;
  p->latest = p->history[p->head] = row;
  p->head = (p->head + 1) % PERF_HISTORY;
  if (p->count < PERF_HISTORY) p->count++;
  perf_reset_window(p);
  return true;
}
#endif
