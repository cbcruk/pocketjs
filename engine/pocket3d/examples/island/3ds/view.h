#ifndef ISLAND_VIEW_H
#define ISLAND_VIEW_H
#include <math.h>
#include <stdbool.h>

typedef struct { float yaw, tilt; } IslandOrbit;
typedef struct {
  float span, target_height, eye_height, distance;
  float sin_yaw, cos_yaw, sin_pitch, cos_pitch;
} IslandView;
static inline float view_clamp(float x, float lo, float hi) { return fmaxf(lo, fminf(hi, x)); }
static inline void island_orbit_update(IslandOrbit *o, float x, float y, float dt, bool reset) {
  if (reset) { *o = (IslandOrbit){0}; return; }
  dt = view_clamp(dt, 0, .1f);
  x = fabsf(x) < .18f ? 0 : view_clamp(x, -1, 1);
  y = fabsf(y) < .18f ? 0 : view_clamp(y, -1, 1);
  o->yaw = remainderf(o->yaw + x * 1.4f * dt, 6.28318530718f);
  o->tilt = view_clamp(o->tilt + y * .7f * dt, -.3f, .35f);
}
static inline IslandView island_view_make(float span, float height, float distance,
                                          float target, float yaw, float tilt) {
  float rise = height - target, radius = sqrtf(rise * rise + distance * distance);
  float pitch = view_clamp(atan2f(rise, distance) + tilt, .34906585f, .87266463f);
  IslandView v = { .span = span, .target_height = target,
    .sin_yaw = sinf(yaw), .cos_yaw = cosf(yaw), .sin_pitch = sinf(pitch), .cos_pitch = cosf(pitch) };
  v.distance = radius * v.cos_pitch;
  v.eye_height = target + radius * v.sin_pitch;
  return v;
}
/* Screen-relative movement remains consistent after orbiting the camera. */
static inline void island_view_move(const IslandView *v, float *x, float *z) {
  float a = *x, b = *z;
  *x = a * v->cos_yaw + b * v->sin_yaw;
  *z = -a * v->sin_yaw + b * v->cos_yaw;
}
/* Shared with the 3D view: speech anchors must follow yaw and elevation. */
static inline void island_view_project(const IslandView *v, float dx, float y, float dz,
                                       float *screen_x, float *screen_y) {
  float pixels = 400.f / v->span;
  *screen_x = 200.f + (dx * v->cos_yaw - dz * v->sin_yaw) * pixels;
  *screen_y = 120.f - ((y - v->target_height) * v->cos_pitch -
                     (dx * v->sin_yaw + dz * v->cos_yaw) * v->sin_pitch) * pixels;
}
#endif
