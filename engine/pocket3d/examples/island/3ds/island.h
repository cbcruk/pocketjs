#ifndef POCKET_ISLAND_H
#define POCKET_ISLAND_H
#include <stdbool.h>
#include <stdint.h>
typedef struct Island Island;
typedef struct {
  float x, y, z, cam_x, cam_z, anchor_x, anchor_y, anchor_z;
  uint32_t action, expression, tick, messages;
} IslandSnapshot;
Island *island_new(void);
void island_free(Island *);
void island_step(Island *, float x, float z, uint32_t flags);
void island_present(Island *, float alpha);
void island_expression(Island *, uint32_t expression);
const void *island_vertices(const Island *, bool terrain, uint32_t *count);
void island_snapshot(const Island *, IslandSnapshot *);
int32_t island_send(Island *, const uint8_t *, uint32_t length);
uint32_t island_bubble(const Island *, uint8_t *out, uint32_t capacity);
uint32_t island_message(const Island *, uint32_t index, uint8_t *out,
                        uint32_t capacity);
#endif
