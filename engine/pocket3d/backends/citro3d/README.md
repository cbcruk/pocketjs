# Pocket3D citro3d backend

This backend draws **colored 3D triangle streams on the PICA200**. It accepts
`ColorVertex` data produced by `pocket3d-anim`, with no application state or
3DS controller assumptions. C owns citro3d calls; Rust samples animation and
skins the character before upload.

The host owns C3D initialization, render targets and the frame boundary.
Call `p3d_mesh_create`, upload a static mesh once or a dynamic mesh after
`C3D_FrameBegin`, then call `p3d_begin` with a citro3d view-projection matrix and
`p3d_draw`. The backend uses a depth buffer, source-alpha blending and no
backface culling; opaque geometry must precede translucent geometry.

The vertex layout is 28 bytes: three position floats followed by four color
floats. Buffers use `linearAlloc` and a data-cache flush before submission.
A host must wait for the GPU before reusing a buffer and must restore its UI
shader, attributes, buffers, blending and depth state before drawing 2D UI.

`color.v.pica` is assembled by devkitPro's `picasso`. This profile has no
texture sampling, GPU skinning, stereo-eye submission or dynamic lighting;
colors and lighting arrive in the vertex stream. The island host is a complete
consumer in `../../examples/island/3ds`.
