"""Render actual Blender actions and facial layers for an asset contact sheet."""

import bpy
from pathlib import Path
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[1]
bpy.ops.wm.open_mainfile(filepath=str(ROOT / "assets/pocket-island.blend"))
scene = bpy.context.scene
rig = next(o for o in scene.objects if o.type == "ARMATURE")
for o in scene.objects:
    if o.type == "MESH":
        o.hide_render = o.parent != rig
scene.render.resolution_x = 256
scene.render.resolution_y = 288
scene.render.resolution_percentage = 100
scene.cycles.samples = 12
cam = scene.camera
cam.location = (0.35, -6, 2.3)
cam.rotation_euler = (
    (Vector((0, 0, 0.95)) - cam.location).to_track_quat("-Z", "Y").to_euler()
)
cam.data.ortho_scale = 2.15
out = ROOT / "../../../.." / "dist/island/character-qa"
out.mkdir(parents=True, exist_ok=True)
shots = [
    ("Idle", 12, "neutral"),
    ("Walk", 6, "neutral"),
    ("Run", 5, "neutral"),
    ("Wave", 18, "happy"),
    ("SitIdle", 12, "neutral"),
    ("BenchSitIdle", 12, "neutral"),
    ("Cheer", 16, "happy"),
] + [
    ("Idle", 1, e)
    for e in ["neutral", "happy", "sad", "surprised", "angry", "shy", "sleepy"]
]
for i, (action, frame, expression) in enumerate(shots):
    rig.animation_data_create()
    rig.animation_data.action = bpy.data.actions[action]
    scene.frame_set(frame)
    for p in rig.pose.bones:
        if p.name.startswith("face.") or p.name == "blink":
            p.scale = (1, 1, 1) if p.name == "face." + expression else (0.0001,) * 3
    bpy.context.view_layer.update()
    scene.render.filepath = str(out / f"{i:02d}-{action}-{expression}.png")
    bpy.ops.render.render(write_still=True)
print("CHARACTER_QA", out)
