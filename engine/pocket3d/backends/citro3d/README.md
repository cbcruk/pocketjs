# Pocket3D citro3d backend

This backend draws **colored triangle streams and indexed rigid skins on the
PICA200**. It accepts geometry and affine bone matrices from `pocket3d-anim`,
with no application state, character names or controller assumptions. C owns
citro3d calls; Rust owns animation sampling and skeleton interpolation.

The host owns C3D initialization, render targets and the frame boundary.
For a colored stream, call `p3d_mesh_create`, upload a static mesh once or a
dynamic mesh after `C3D_FrameBegin`, then call `p3d_begin` with a citro3d
view-projection matrix and `p3d_draw`. `ColorVertex` contains three position
floats followed by four color floats, for **28 bytes per vertex**.

For a rigid skin, initialize `skin.v.pica` with `p3d_skin_init` and pass a
`P3D_SkinSource` to `p3d_skin_create`. The backend copies **40-byte unique
vertices and 16-bit indices into one resident linear allocation**. Each vertex
contains position, normal, square-root material color and its bone's matrix-row
offset. The source can be released after creation. Every actor can share the
resulting `P3D_SkinMesh`; it contains no mutable pose or per-actor vertex stream.

Before drawing, call `p3d_begin` for common depth/blend state, then
`p3d_skin_begin` for the skin shader and attributes. Pass each actor's affine
matrix palette and `p3d_skin_visible` mask to `p3d_skin_draw`. **Up to 29 joints
use 87 float-vector uniforms**, leaving room for projection and shader
constants within the PICA200's 96-register limit. This backend rejects larger
palettes. Its input contract has one bone per vertex; weighted multi-bone
blending is not implemented.

The shader transforms positions and normals, normalizes the normals and
computes the existing diffuse pastel lighting. An all-zero affine matrix marks
a hidden joint. Immutable index ranges record the three influencing joints;
ranges with no visible joint are skipped, and adjacent visible ranges are
submitted together. **Expression visibility needs no vertex or index uploads.**
Each actor updates at most 1,392 bytes of matrix uniforms before drawing.

Buffers use `linearAlloc` and a data-cache flush before their first submission.
The host must wait for the GPU before freeing or changing an in-flight buffer.
The backend uses a depth buffer, source-alpha blending and no backface culling.
The host must restore UI shader, attributes, buffers, blending and depth state
before drawing 2D UI. The island host in `../../examples/island/3ds` demonstrates
a static terrain, shared shadow mesh and independent actors using these paths.

Both shaders are assembled by devkitPro's `picasso`. Texture sampling,
stereo-eye submission and dynamic lights are outside this profile.
