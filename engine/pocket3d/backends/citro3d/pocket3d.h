#ifndef POCKET3D_CITRO3D_H
#define POCKET3D_CITRO3D_H
#include <citro3d.h>
#include <stdbool.h>
#include <stddef.h>
/* P3M1 ColorVertex layout: position.xyz, color.rgba; renderer owns GPU memory.
 */
typedef struct {
  float position[3], color[4];
} P3D_ColorVertex;
typedef struct {
  P3D_ColorVertex *vertices;
  size_t count, capacity;
  C3D_BufInfo buffer;
} P3D_Mesh;
bool p3d_init(const void *shader, size_t size);
void p3d_exit(void);
bool p3d_mesh_create(P3D_Mesh *mesh, size_t capacity);
bool p3d_mesh_upload(P3D_Mesh *mesh, const P3D_ColorVertex *vertices,
                     size_t count);
void p3d_mesh_free(P3D_Mesh *mesh);
void p3d_begin(const C3D_Mtx *view_projection);
void p3d_draw(const P3D_Mesh *mesh);
#endif
