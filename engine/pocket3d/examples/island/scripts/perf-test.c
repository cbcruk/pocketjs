#include "../3ds/perf.h"
#include <assert.h>
#include <math.h>
#include <stdio.h>

int main(void) {
  PerfStats p = {0};
  float stages[PERF_STAGES] = {2, 1, 3, 4, 10, 7};
  /* 50 submitted frames over 1 second, not the fixed 30 Hz simulation. */
  for (int i = 0; i < 50; i++)
    assert(perf_record(&p, 20, stages, i != 0, 1, 24960, 100, 0, false) == (i == 49));
  assert(fabs(p.latest.fps - 50) < .001);
  assert(p.latest.frame_ms == 20 && p.latest.p95_ms == 20);
  assert(p.latest.stage[PERF_GPU] == 7 && p.latest.stage[PERF_UPDATE] == 2);
  /* A stall belongs in frame time even if CPU submission was short. */
  perf_record(&p, 1000, stages, true, 3, 24960, 100, 1, false);
  assert(p.latest.fps == 1 && p.latest.max_ms == 1000);
  /* Reset partial windows after keyboard/export/menu transitions. */
  perf_record(&p, 100, stages, true, 1, 24960, 100, 1, false);
  perf_reset_window(&p);
  perf_record(&p, 1000, stages, true, 0, 24960, 100, 0, true);
  assert(p.latest.frames == 1 && p.latest.panel == 1);
  /* Nearest-rank p95 differs from maximum, even at low frame rates. */
  for (int i = 1; i <= 20; i++)
    perf_record(&p, i * 5, stages, true, 1, 10, 20, 2, false);
  assert(p.latest.p95_ms == 95 && p.latest.max_ms == 100);
  assert(fabs(p.latest.fps - 1000.0 * 20 / 1050) < .001);
  for (unsigned i = 0; i < PERF_HISTORY + 3; i++)
    perf_record(&p, 1000, stages, false, 1, i, 20, 2, false);
  assert(p.count == PERF_HISTORY);
  assert(p.history[(p.head + PERF_HISTORY - p.count) % PERF_HISTORY].avatar_vertices == 3);
  assert(p.latest.stage[PERF_GPU] == 0);
  puts("PASS: measured FPS, GPU sample alignment, stalls, p95, reset and bounded history");
}
