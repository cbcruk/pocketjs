#include "../3ds/view.h"
#include <assert.h>
#include <stdio.h>

int main(void) {
  /* A screen-right gesture stays right, at constant speed, around the island.
   * The same basis projects the speech anchor and the world geometry. */
  for (int i = -8; i <= 8; i++) for (int j = -1; j <= 1; j++) {
    IslandView v = island_view_make(7.2f, 6.6f, 10, .8f, i * .4f, j * .3f);
    float x = 1, z = 0, sx, sy;
    island_view_move(&v, &x, &z);
    assert(fabsf(x * x + z * z - 1) < .00001f);
    island_view_project(&v, x, .8f, z, &sx, &sy);
    assert(fabsf(sx - (200 + 400 / 7.2f)) < .0001f);
    assert(fabsf(sy - 120) < .0001f);
    x = 0; z = -1;
    island_view_move(&v, &x, &z);
    island_view_project(&v, x, .8f, z, &sx, &sy);
    assert(fabsf(sx - 200) < .0001f && sy < 120);
    island_view_project(&v, 0, 2, 0, &sx, &sy);
    assert(sx == 200 && sy < 120); // Speech stays above the camera target.
    assert(fabsf((v.eye_height - .8f) * (v.eye_height - .8f) +
                 v.distance * v.distance - (5.8f * 5.8f + 100)) < .0001f);
  }
  IslandOrbit a = {0}, b = {0};
  for (int i = 0; i < 30; i++) island_orbit_update(&a, .5f, .2f, 1.f / 30, false);
  for (int i = 0; i < 60; i++) island_orbit_update(&b, .5f, .2f, 1.f / 60, false);
  assert(fabsf(a.yaw - b.yaw) < .00001f && fabsf(a.tilt - b.tilt) < .00001f);
  b = a;
  island_orbit_update(&b, .1f, -.1f, 1, false);
  assert(b.yaw == a.yaw && b.tilt == a.tilt); // Resting-stick noise.
  island_orbit_update(&b, 1, 1, 20, false);
  assert(b.yaw - a.yaw <= .14001f); // Pause/resume cannot jump around the island.
  for (int i = 0; i < 100; i++) island_orbit_update(&b, 1, 1, .1f, false);
  assert(b.tilt <= .35f && fabsf(b.yaw) <= 3.141593f);
  island_orbit_update(&b, 1, 1, .1f, true);
  assert(b.yaw == 0 && b.tilt == 0);
  puts("PASS: orbit, screen-relative movement, speech projection, dead zone and reset");
}
