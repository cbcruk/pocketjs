#include "pocket3d.h"
#include <string.h>
static DVLB_s *binary;
static shaderProgram_s program;
static int projection;
bool p3d_init(const void *shader, size_t size) {
  binary = DVLB_ParseFile((u32 *)shader, size);
  if (!binary)
    return false;
  shaderProgramInit(&program);
  shaderProgramSetVsh(&program, &binary->DVLE[0]);
  projection =
      shaderInstanceGetUniformLocation(program.vertexShader, "projection");
  return projection >= 0;
}
void p3d_exit(void) {
  shaderProgramFree(&program);
  if (binary)
    DVLB_Free(binary);
  binary = NULL;
}
bool p3d_mesh_create(P3D_Mesh *m, size_t capacity) {
  memset(m, 0, sizeof *m);
  if (capacity > 300000)
    return false;
  m->vertices = linearAlloc(capacity * sizeof *m->vertices);
  if (!m->vertices)
    return false;
  m->capacity = capacity;
  BufInfo_Init(&m->buffer);
  BufInfo_Add(&m->buffer, m->vertices, sizeof *m->vertices, 2, 0x10);
  return true;
}
bool p3d_mesh_upload(P3D_Mesh *m, const P3D_ColorVertex *v, size_t n) {
  if (n > m->capacity || n % 3)
    return false;
  memcpy(m->vertices, v, n * sizeof *v);
  m->count = n;
  GSPGPU_FlushDataCache(m->vertices, n * sizeof *v);
  return true;
}
void p3d_mesh_free(P3D_Mesh *m) {
  if (m->vertices)
    linearFree(m->vertices);
  memset(m, 0, sizeof *m);
}
void p3d_begin(const C3D_Mtx *vp) {
  C3D_BindProgram(&program);
  C3D_FVUnifMtx4x4(GPU_VERTEX_SHADER, projection, vp);
  C3D_AttrInfo *a = C3D_GetAttrInfo();
  AttrInfo_Init(a);
  AttrInfo_AddLoader(a, 0, GPU_FLOAT, 3);
  AttrInfo_AddLoader(a, 1, GPU_FLOAT, 4);
  C3D_DepthTest(true, GPU_GEQUAL, GPU_WRITE_ALL);
  C3D_CullFace(GPU_CULL_NONE);
  C3D_AlphaBlend(GPU_BLEND_ADD, GPU_BLEND_ADD, GPU_SRC_ALPHA,
                 GPU_ONE_MINUS_SRC_ALPHA, GPU_ONE, GPU_ONE_MINUS_SRC_ALPHA);
  C3D_AlphaTest(false, GPU_ALWAYS, 0);
  C3D_SetScissor(GPU_SCISSOR_DISABLE, 0, 0, 0, 0);
  C3D_TexEnv *env = C3D_GetTexEnv(0);
  C3D_TexEnvInit(env);
  C3D_TexEnvSrc(env, C3D_Both, GPU_PRIMARY_COLOR, 0, 0);
  C3D_TexEnvFunc(env, C3D_Both, GPU_REPLACE);
  for (int i = 1; i < 6; i++)
    C3D_TexEnvInit(C3D_GetTexEnv(i));
}
void p3d_draw(const P3D_Mesh *m) {
  C3D_SetBufInfo((C3D_BufInfo *)&m->buffer);
  C3D_DrawArrays(GPU_TRIANGLES, 0, m->count);
}
