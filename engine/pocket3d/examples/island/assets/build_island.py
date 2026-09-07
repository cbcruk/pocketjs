"""Original Pocket Island art. Blender 5.1; no downloaded character assets.
Run: Blender --background --factory-startup --python build_island.py
Exports editable blend, GLB, and the versioned Pocket3D colored-mesh profile.
"""

import bpy, math, struct, json, os, random
from mathutils import Vector, Matrix, Quaternion
from pathlib import Path

ROOT = Path(__file__).resolve().parent
random.seed(29)
bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete(use_global=False)
for x in bpy.data.materials:
    bpy.data.materials.remove(x)
MATS = {}


def mat(name, color):
    m = bpy.data.materials.new(name)
    m.diffuse_color = (*color, 1)
    m.use_nodes = True
    bs = m.node_tree.nodes.get("Principled BSDF")
    bs.inputs["Base Color"].default_value = (*color, 1)
    bs.inputs["Roughness"].default_value = 0.82
    MATS[name] = m
    return m


skin = mat("peach porcelain", (0.88, 0.55, 0.37))
hair = mat("chestnut", (0.105, 0.041, 0.024))
shine = mat("warm hair ribbons", (0.19, 0.075, 0.032))
cream = mat("cotton cream", (0.94, 0.90, 0.74))
dress = mat("marigold linen", (0.86, 0.43, 0.095))
hem = mat("ochre seam", (0.53, 0.235, 0.055))
shoe = mat("cocoa leather", (0.15, 0.067, 0.04))
dark = mat("espresso eyes", (0.036, 0.016, 0.016))
white = mat("eye sparkle", (1, 0.98, 0.90))
blush = mat("rose cheeks", (0.94, 0.28, 0.22))
leaf = mat("leaf embroidery", (0.20, 0.39, 0.18))
sea = mat("lagoon turquoise", (0.16, 0.57, 0.66))
shallow = mat("shallow aqua", (0.32, 0.72, 0.70))
foam = mat("seafoam", (0.73, 0.89, 0.76))
sand = mat("warm sand", (0.83, 0.70, 0.43))
sandedge = mat("sand bank", (0.65, 0.48, 0.25))
grass = mat("meadow", (0.32, 0.59, 0.23))
edge = mat("grass bank", (0.22, 0.40, 0.145))
pathmat = mat("garden path", (0.73, 0.60, 0.34))
trunk = mat("tree bark", (0.30, 0.15, 0.065))
foliage = mat("pear foliage", (0.24, 0.48, 0.16))
foliage2 = mat("sunlit foliage", (0.38, 0.63, 0.23))
fruit = mat("peaches", (0.95, 0.38, 0.19))
wall = mat("cottage plaster", (0.91, 0.79, 0.55))
roof = mat("terracotta roof", (0.64, 0.235, 0.125))
wood = mat("honey timber", (0.45, 0.235, 0.095))
teal = mat("sea glass door", (0.13, 0.39, 0.38))
glass = mat("window blue", (0.20, 0.45, 0.50))
pink = mat("cosmos petals", (0.96, 0.44, 0.43))
stone = mat("warm stone", (0.49, 0.52, 0.40))
CHAR = []
WORLD = []
OWN = {}
bones = []
bone_defs = {}


def register(o, name, material, bone=None, world=False):
    o.name = name
    o.data.materials.append(material)
    for p in o.data.polygons:
        p.use_smooth = True
    (WORLD if world else CHAR).append(o)
    if bone:
        OWN[o.name] = bone
    return o


def ell(name, loc, scale, material, bone=None, world=False, seg=8, rings=6):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=seg, ring_count=rings, location=loc)
    o = bpy.context.object
    o.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return register(o, name, material, bone, world)


def disc(name, loc, scale, material, seg=12):
    # Raised center and an elliptical rim: the buried underside of paths and
    # subpixel flower petals does not need a closed, latitude-divided sphere.
    verts = [(loc[0], loc[1], loc[2] + scale[2])]
    verts += [(loc[0] + scale[0] * math.cos(i * math.tau / seg),
               loc[1] + scale[1] * math.sin(i * math.tau / seg), loc[2])
              for i in range(seg)]
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], [(0, i + 1, (i + 1) % seg + 1) for i in range(seg)])
    o = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(o)
    return register(o, name, material, world=True)


def cube(name, loc, scale, material, bone=None, world=False, bevel=0):
    bpy.ops.mesh.primitive_cube_add(size=1, location=loc)
    o = bpy.context.object
    o.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    if bevel:
        md = o.modifiers.new("soft corners", "BEVEL")
        md.width = bevel
        md.segments = 1
        bpy.context.view_layer.objects.active = o
        bpy.ops.object.modifier_apply(modifier=md.name)
    return register(o, name, material, bone, world)


def cone(name, loc, r1, r2, depth, material, bone=None, world=False, verts=12):
    bpy.ops.mesh.primitive_cone_add(
        vertices=verts, radius1=r1, radius2=r2, depth=depth, location=loc
    )
    return register(bpy.context.object, name, material, bone, world)


def line(name, pts, r, material, bone=None, world=False):
    cv = bpy.data.curves.new(name, "CURVE")
    cv.dimensions = "3D"
    cv.resolution_u = 2
    cv.bevel_depth = r
    cv.bevel_resolution = 1
    s = cv.splines.new("POLY")
    s.points.add(len(pts) - 1)
    for p, v in zip(s.points, pts):
        p.co = (*v, 1)
    o = bpy.data.objects.new(name, cv)
    bpy.context.collection.objects.link(o)
    bpy.context.view_layer.objects.active = o
    o.select_set(True)
    bpy.ops.object.convert(target="MESH")
    o.select_set(False)
    return register(o, name, material, bone, world)


def bone(name, head, parent=None):
    bones.append(name)
    bone_defs[name] = (head, parent)
    return name


bone("root", (0, 0, 0))
bone("hips", (0, 0, 0.64), "root")
bone("chest", (0, 0, 0.87), "hips")
bone("head", (0, 0, 1.15), "chest")
bone("chat.anchor", (0, 0, 1.94), "head")
for side, sg in [("L", 1), ("R", -1)]:
    bone("upper_arm." + side, (sg * 0.24, 0, 1.01), "chest")
    bone("forearm." + side, (sg * 0.34, -0.005, 0.80), "upper_arm." + side)
    bone("hand." + side, (sg * 0.39, -0.015, 0.63), "forearm." + side)
    bone("thigh." + side, (sg * 0.115, 0, 0.63), "hips")
    bone("shin." + side, (sg * 0.115, 0, 0.34), "thigh." + side)
    bone("foot." + side, (sg * 0.115, -0.025, 0.105), "shin." + side)
    bone("hair." + side, (sg * 0.22, 0.03, 1.45), "head")
bone("hair.back", (0, 0.17, 1.47), "head")
# Face geometry lives on expression-specific bones. Body clips omit them.
for exp in ["neutral", "happy", "sad", "surprised", "angry", "shy", "sleepy"]:
    bone("face." + exp, (0, 0, 1.4), "head")
bone("blink", (0, 0, 1.4), "head")
# Body, stitched pinafore, blouse collar, shoes and hands.
ell("blouse", (0, 0, 0.96), (0.25, 0.135, 0.215), cream, "chest")
cone("pinafore skirt", (0, 0, 0.665), 0.285, 0.195, 0.32, dress, "hips", verts=16)
ell("pinafore bib", (0, -0.132, 0.92), (0.158, 0.024, 0.135), dress, "chest")
for sg, side in [(1, "L"), (-1, "R")]:
    cube(
        "pinafore strap " + side,
        (sg * 0.127, -0.132, 1.02),
        (0.045, 0.025, 0.21),
        dress,
        "chest",
        bevel=0.008,
    )
    ell(
        "brass button " + side,
        (sg * 0.126, -0.155, 0.954),
        (0.017, 0.009, 0.017),
        hem,
        "chest",
        seg=8,
        rings=6,
    )
    ell(
        "peter pan collar " + side,
        (sg * 0.066, -0.137, 1.083),
        (0.068, 0.022, 0.039),
        white,
        "chest",
    )
    ell(
        "puff sleeve " + side,
        (sg * 0.271, 0, 0.958),
        (0.087, 0.095, 0.125),
        cream,
        "upper_arm." + side,
    )
    ell(
        "upper arm " + side,
        (sg * 0.307, 0, 0.872),
        (0.050, 0.052, 0.125),
        skin,
        "upper_arm." + side,
    )
    ell(
        "forearm " + side,
        (sg * 0.365, -0.005, 0.718),
        (0.047, 0.047, 0.112),
        skin,
        "forearm." + side,
    )
    ell(
        "mitten hand " + side,
        (sg * 0.394, -0.018, 0.606),
        (0.055, 0.044, 0.066),
        skin,
        "hand." + side,
    )
    ell(
        "thumb " + side,
        (sg * 0.356, -0.043, 0.618),
        (0.024, 0.024, 0.033),
        skin,
        "hand." + side,
        seg=8,
        rings=6,
    )
    ell(
        "leg " + side,
        (sg * 0.115, 0, 0.456),
        (0.068, 0.073, 0.175),
        skin,
        "thigh." + side,
    )
    ell(
        "sock " + side,
        (sg * 0.115, 0, 0.23),
        (0.066, 0.070, 0.135),
        cream,
        "shin." + side,
    )
    ell(
        "mary jane " + side,
        (sg * 0.115, -0.065, 0.079),
        (0.086, 0.139, 0.075),
        shoe,
        "foot." + side,
    )
    ell(
        "shoe opening " + side,
        (sg * 0.115, -0.012, 0.126),
        (0.06, 0.075, 0.021),
        skin,
        "foot." + side,
    )
    cube(
        "shoe strap " + side,
        (sg * 0.115, -0.058, 0.139),
        (0.126, 0.025, 0.02),
        shoe,
        "foot." + side,
        bevel=0.009,
    )
# Pocket and small embroidered sprig, kept readable at 400x240.
ell("front pocket", (0, -0.213, 0.724), (0.09, 0.012, 0.063), hem, "hips")
ell("pocket patch", (0, -0.225, 0.737), (0.08, 0.009, 0.053), dress, "hips")
line("sprig stem", [(0, -0.163, 0.875), (0, -0.165, 0.93)], 0.006, leaf, "chest")
for sg in [-1, 1]:
    ell(
        "embroidered leaf",
        (sg * 0.023, -0.165, 0.916),
        (0.029, 0.007, 0.013),
        leaf,
        "chest",
        seg=8,
        rings=6,
    )
# Oversized head, ears, sculpted nose and cheeks.
ell("face", (0, -0.008, 1.435), (0.327, 0.273, 0.333), skin, "head", seg=24, rings=16)
for sg in [-1, 1]:
    ell("ear", (sg * 0.317, 0.005, 1.401), (0.059, 0.042, 0.075), skin, "head")
    ell("cheek", (sg * 0.207, -0.217, 1.332), (0.053, 0.008, 0.026), blush, "head")
ell("button nose", (0, -0.285, 1.369), (0.024, 0.030, 0.027), skin, "head")
# A sculpted cap with a forehead opening; back hangs to the shoulders.
verts = []
faces = []
N = 32
R = 9
for j in range(R + 1):
    t = j / R
    for i in range(N):
        a = 2 * math.pi * i / N
        front = max(0, -math.sin(a))
        phi = t * (1.65 - 0.46 * front)
        verts.append(
            (
                0.345 * math.sin(phi) * math.cos(a),
                0.025 + 0.294 * math.sin(phi) * math.sin(a),
                1.465 + 0.345 * math.cos(phi),
            )
        )
for j in range(R):
    for i in range(N):
        a = j * N + i
        b = j * N + (i + 1) % N
        faces.append((a, b, b + N, a + N))
mesh = bpy.data.meshes.new("parted crown")
mesh.from_pydata(verts, [], faces)
o = bpy.data.objects.new("parted crown", mesh)
bpy.context.collection.objects.link(o)
register(o, "parted crown", hair, "head")
# Back hair: a shell, open on the face side, with a scalloped shoulder line.
verts = []
faces = []
N = 20
R = 6
for j in range(R + 1):
    t = j / R
    for i in range(N + 1):
        a = -0.12 + (math.pi + 0.24) * i / N
        verts.append(
            (
                (0.32 + 0.015 * math.sin(t * math.pi)) * math.cos(a),
                0.035 + 0.245 * math.sin(a),
                1.58 - 0.50 * t + 0.018 * math.cos(i * math.pi / 2) * t,
            )
        )
for j in range(R):
    for i in range(N):
        a = j * (N + 1) + i
        faces.append((a, a + 1, a + N + 2, a + N + 1))
mesh = bpy.data.meshes.new("shoulder hair shell")
mesh.from_pydata(verts, [], faces)
o = bpy.data.objects.new("shoulder hair shell", mesh)
bpy.context.collection.objects.link(o)
register(o, "shoulder hair shell", hair, "hair.back")
# Two broad swept locks leave a visible center part and open forehead.
for sg, side in [(1, "L"), (-1, "R")]:
    pts = [
        (sg * 0.025, -0.16, 1.773),
        (sg * 0.14, -0.257, 1.704),
        (sg * 0.25, -0.246, 1.59),
        (sg * 0.292, -0.19, 1.42),
        (sg * 0.296, -0.12, 1.23),
        (sg * 0.26, -0.087, 1.09),
    ]
    # Tapered elliptical rings form a soft ribbon, not spherical beads.
    vs = []
    fs = []
    for j, p in enumerate(pts):
        rw = [0.027, 0.065, 0.077, 0.065, 0.060, 0.013][j]
        for k in range(8):
            a = 2 * math.pi * k / 8
            vs.append((p[0] + rw * math.cos(a), p[1] + 0.024 * math.sin(a), p[2]))
    for j in range(len(pts) - 1):
        for k in range(8):
            a = j * 8 + k
            b = j * 8 + (k + 1) % 8
            fs.append((a, b, b + 8, a + 8))
    fs.extend([tuple(range(7, -1, -1)), tuple(range(40, 48))])
    me = bpy.data.meshes.new("swept lock")
    me.from_pydata(vs, [], fs)
    o = bpy.data.objects.new("swept lock", me)
    bpy.context.collection.objects.link(o)
    register(o, "swept lock " + side, hair, "hair." + side)
    line(
        "hair highlight " + side,
        [(p[0] + sg * 0.015, p[1] - 0.025, p[2]) for p in pts[1:-1]],
        0.005,
        shine,
        "hair." + side,
    )
line(
    "center part",
    [(0, -0.162, 1.782), (0, -0.09, 1.807), (0, 0.02, 1.812)],
    0.004,
    shine,
    "head",
)


# Expressions authored as mesh layers, skin-bound, visible through bone scale.
def smile(name, exp, down=False, width=0.062, z=1.29):
    pts = []
    for k in range(9):
        x = -width + 2 * width * k / 8
        zz = z + (0.028 if down else -0.025) * (1 - (x / width) ** 2)
        pts.append((x, -0.258, zz))
    line(name, pts, 0.007, dark, "face." + exp)


for exp in ["neutral", "happy", "sad", "surprised", "angry", "shy", "sleepy"]:
    bn = "face." + exp
    for sg in [-1, 1]:
        x = sg * 0.123
        if exp in ["happy", "sleepy"]:
            pts = [
                (
                    x - 0.049 + k * 0.098 / 8,
                    -0.266,
                    1.415
                    + (0.028 if exp == "happy" else -0.014) * math.sin(k * math.pi / 8),
                )
                for k in range(9)
            ]
            line("closed eye " + exp, pts, 0.011, dark, bn)
        else:
            sz = 0.055 if exp == "surprised" else (0.039 if exp == "shy" else 0.049)
            ell(
                "eye white " + exp, (x, -0.262, 1.425), (0.057, 0.014, 0.071), cream, bn
            )
            ell("iris " + exp, (x, -0.279, 1.425), (0.038, 0.009, sz), dark, bn)
            ell(
                "eye glint " + exp,
                (x - 0.012, -0.289, 1.448),
                (0.012, 0.004, 0.016),
                white,
                bn,
                seg=8,
                rings=6,
            )
        by = 1.516
        slope = sg * (0.032 if exp == "angry" else (-0.031 if exp == "sad" else 0.005))
        line(
            "eyebrow " + exp,
            [(x - 0.042, -0.25, by - slope), (x + 0.042, -0.25, by + slope)],
            0.009,
            hair,
            bn,
        )
    if exp == "surprised":
        ell("o mouth", (0, -0.265, 1.286), (0.026, 0.01, 0.037), dark, bn)
    elif exp == "happy":
        ell("happy mouth", (0, -0.261, 1.283), (0.055, 0.008, 0.033), dark, bn)
        ell("happy tongue", (0, -0.271, 1.27), (0.035, 0.004, 0.012), blush, bn)
    else:
        smile(
            "mouth " + exp,
            exp,
            exp in ["sad", "angry"],
            0.042 if exp in ["shy", "sleepy"] else 0.054,
        )
    if exp == "shy":
        for sg in [-1, 1]:
            for k in range(3):
                line(
                    "blush stroke",
                    [
                        (sg * (0.16 + k * 0.016), -0.245, 1.362),
                        (sg * (0.17 + k * 0.016), -0.247, 1.337),
                    ],
                    0.004,
                    blush,
                    bn,
                )
for sg in [-1, 1]:
    line(
        "blink eye",
        [(sg * 0.123 - 0.048, -0.285, 1.418), (sg * 0.123 + 0.048, -0.285, 1.418)],
        0.011,
        dark,
        "blink",
    )
line(
    "blink smile",
    [(-0.054, -0.258, 1.29), (0, -0.258, 1.267), (0.054, -0.258, 1.29)],
    0.007,
    dark,
    "blink",
)
# Real editable armature; all pieces have skin vertex groups and modifiers.
bpy.ops.object.armature_add()
rig = bpy.context.object
rig.name = "Mira / body + face rig"
bpy.ops.object.mode_set(mode="EDIT")
rig.data.edit_bones.remove(rig.data.edit_bones[0])
for name in bones:
    h, p = bone_defs[name]
    b = rig.data.edit_bones.new(name)
    b.head = h
    b.tail = (h[0], h[1], h[2] + 0.1)
    if p:
        b.parent = rig.data.edit_bones[p]
bpy.ops.object.mode_set(mode="OBJECT")
for o in CHAR:
    bn = OWN.get(o.name, "head")
    g = o.vertex_groups.new(name=bn)
    g.add(list(range(len(o.data.vertices))), 1, "REPLACE")
    md = o.modifiers.new("Mira skin", "ARMATURE")
    md.object = rig
    o.parent = rig
for pb in rig.pose.bones:
    pb.rotation_mode = "XYZ"
FPS = 24
CLIPS = []
GAITS = {
    "Walk": {"stride": 0.88, "stance": 0.55, "lift": 0.030, "duration": 0.84},
    "Run": {"stride": 1.20, "stance": 0.34, "lift": 0.035, "duration": 0.58},
}


def reset():
    for p in rig.pose.bones:
        p.location = (0, 0, 0)
        p.rotation_euler = (0, 0, 0)
        p.scale = (1, 1, 1)


def r(name, axis, value):
    rig.pose.bones[name].rotation_euler[axis] = value


# Bones point along Blender Z, so bone-local Z corresponds to -world Y.
# Apply world-space X/Z rotations using converted local axes (local X / local Y).
def pose(kind, t, dur):
    reset()
    phase = t * 2 * math.pi / (0.84 if kind == "Walk" else 0.58)
    if kind in ["Idle", "Walk", "Run"]:
        rig.pose.bones["hips"].location[1] = (
            0.012 * math.sin(t * math.pi * 2 / 3)
            if kind == "Idle"
            else 0.024 * (1 - math.cos(phase * 2))
        )
        if kind != "Idle":
            gait = GAITS[kind]
            # A planted foot travels backwards at stride/duration in model
            # space. Runtime advances the clip by actual distance/stride, so
            # the same foot remains still in the world at every stick speed.
            upper = 0.29
            lower = math.hypot(0.235, 0.025)
            bind_angle = math.atan2(0.025, 0.235)
            feet = []
            for sg, side in [(1, "L"), (-1, "R")]:
                u = (t / gait["duration"] + (0 if sg == 1 else 0.5)) % 1
                reach = gait["stride"] * gait["stance"]
                if u < gait["stance"]:
                    forward = reach / 2 - gait["stride"] * u
                    lift = 0
                else:
                    swing = (u - gait["stance"]) / (1 - gait["stance"])
                    ease = swing * swing * (3 - 2 * swing)
                    forward = reach * (ease - 0.5)
                    lift = gait["lift"] * math.sin(math.pi * swing) ** 2
                feet.append((side, u, forward, lift))
            # Keep the supporting leg near extension instead of crouching
            # throughout the cycle. Both soles stay reachable by the rig.
            hip_height = min(0.105 + lift + math.sqrt(
                (upper + lower - 0.004) ** 2 - forward ** 2)
                for _, _, forward, lift in feet)
            rig.pose.bones["hips"].location[1] = hip_height - 0.63
            for side, u, forward, lift in feet:
                # Solve the authored two-link leg in Blender's Y/Z plane.
                down = hip_height - (0.105 + lift)
                distance = math.hypot(forward, down)
                assert abs(upper - lower) < distance < upper + lower
                thigh = math.atan2(-forward, down) - math.acos(
                    (upper * upper + distance * distance - lower * lower) / (2 * upper * distance))
                shin = math.pi - math.acos(
                    (upper * upper + lower * lower - distance * distance) / (2 * upper * lower)) + bind_angle
                r("thigh." + side, 0, thigh)
                r("shin." + side, 0, shin)
                r("foot." + side, 0, -thigh - shin)
                r("upper_arm." + side, 0, math.cos(u * math.tau) * (0.28 if kind == "Walk" else 0.42))
                r("forearm." + side, 0, -0.12 if kind == "Walk" else -0.45)
            r("chest", 0, 0.04 if kind == "Walk" else 0.08)
    if kind in [
        "SitDown",
        "SitIdle",
        "StandUp",
        "BenchSitDown",
        "BenchSitIdle",
        "BenchStandUp",
    ]:
        bench = kind.startswith("Bench")
        kind = kind.removeprefix("Bench")
        a = (
            min(1, t / dur)
            if kind == "SitDown"
            else (1 - min(1, t / dur) if kind == "StandUp" else 1)
        )
        a = a * a * (3 - 2 * a)
        rig.pose.bones["hips"].location[1] = (-0.08 if bench else -0.465) * a
        for side in ["L", "R"]:
            r("thigh." + side, 0, (-1.50 if bench else -1.90) * a)
            r("shin." + side, 0, (1.50 if bench else 1.05) * a)
            r("foot." + side, 0, (0.0 if bench else 0.85) * a)
            r("upper_arm." + side, 0, -0.45 * a)
            r("forearm." + side, 0, -0.65 * a)
        r("chest", 0, 0.07 * a)
    if kind == "Wave":
        a = min(1, t / 0.25, (dur - t) / 0.3)
        a = max(0, a)
        r("upper_arm.R", 2, -2.10 * a)
        r("forearm.R", 2, -0.50 * a)
        r("hand.R", 2, 0.30 * math.sin(t * 14) * a)
        r("head", 2, 0.08 * a)
    if kind == "Cheer":
        a = max(0, min(1, t / 0.22, (dur - t) / 0.3))
        for sg, side in [(1, "L"), (-1, "R")]:
            r("upper_arm." + side, 2, sg * 2.1 * a)
            r("forearm." + side, 0, -0.3 * a)
        rig.pose.bones["hips"].location[1] = abs(math.sin(t * 7)) * 0.10 * a
    for sg, side in [(1, "L"), (-1, "R")]:
        r("hair." + side, 0, 0.028 * math.sin(t * 5 + sg))
        r("hair." + side, 1, 0.025 * math.sin(t * 4 + sg))
    r("hair.back", 0, 0.025 * math.sin(t * 4))


for kind, dur in [
    ("Idle", 3),
    ("Walk", 0.84),
    ("Run", 0.58),
    ("SitDown", 0.65),
    ("SitIdle", 3),
    ("StandUp", 0.55),
    ("Wave", 1.8),
    ("Cheer", 2),
    ("BenchSitDown", 0.65),
    ("BenchSitIdle", 3),
    ("BenchStandUp", 0.55),
]:
    rig.animation_data_clear()
    action = bpy.data.actions.new(kind)
    rig.animation_data_create()
    rig.animation_data.action = action
    count = round(dur * FPS)
    for frame in range(count + 1):
        pose(kind, frame / count * dur, dur)
        for pb in rig.pose.bones:
            if pb.name.startswith("face.") or pb.name == "blink":
                continue
            for prop in ["location", "rotation_euler", "scale"]:
                pb.keyframe_insert(prop, frame=frame + 1, group=pb.name)
    CLIPS.append((kind, dur, action))
rig.animation_data_clear()
reset()
# Separate expression actions remain usable in Blender and GLB.
for exp in ["neutral", "happy", "sad", "surprised", "angry", "shy", "sleepy"]:
    action = bpy.data.actions.new("Expression_" + exp)
    rig.animation_data_create()
    rig.animation_data.action = action
    for name in bones:
        if name.startswith("face.") or name == "blink":
            rig.pose.bones[name].scale = (
                (1, 1, 1) if name == "face." + exp else (0.0001,) * 3
            )
            rig.pose.bones[name].keyframe_insert("scale", frame=1, group=name)
            rig.pose.bones[name].keyframe_insert("scale", frame=25, group=name)
    CLIPS.append(("Expression_" + exp, 1, action))
    rig.animation_data_clear()
    reset()
# Warm little island with a walkable central garden.
for name, z, rx, ry, ma in [
    ("ocean", -0.27, 70, 70, sea),
    ("shallows", -0.24, 12, 10, shallow),
    ("shore", -0.16, 10.8, 8.8, sandedge),
    ("beach", -0.075, 10.7, 8.7, sand),
    ("grass bank", -0.015, 9.25, 7.25, edge),
    ("meadow", 0.045, 9.15, 7.15, grass),
]:
    o = cone(name, (0, 0, z), 1, 1, 0.12, ma, world=True, verts=64)
    o.scale = (rx, ry, 1)
# Meandering flat path tiles.
for i in range(18):
    y = -5.1 + i * 0.56
    x = 0.25 * math.sin(y * 0.65)
    disc(
        "sandy garden path",
        (x, y, 0.106),
        (0.40, 0.28, 0.004),
        pathmat,
        seg=12,
    )
for x in [-3.2, 3.2]:
    for i in range(7):
        disc(
            "side path",
            (x * i / 7, 0.50, 0.106),
            (0.18, 0.20, 0.004),
            pathmat,
            seg=10,
        )
COLLIDERS = []


def tree(x, y, s=1):
    cone("pear trunk", (x, y, 0.68 * s), 0.16 * s, 0.10 * s, 1.3 * s, trunk, world=True)
    for dx, dy, dz, sz, ma in [
        (-0.35, 0, 1.65, 0.67, foliage),
        (0.36, 0.04, 1.71, 0.72, foliage),
        (0, 0, 2.18, 0.75, foliage2),
    ]:
        ell(
            "pear canopy",
            (x + dx * s, y + dy * s, dz * s),
            (sz * s, sz * 0.88 * s, sz * 0.83 * s),
            ma,
            world=True,
            seg=10,
            rings=6,
        )
    for a in [0.2, 2.4, 4.4]:
        ell(
            "peach",
            (x + 0.61 * s * math.cos(a), y + 0.59 * s * math.sin(a), 1.65 * s),
            (0.115 * s,) * 3,
            fruit,
            world=True,
            seg=8,
            rings=4,
        )
    COLLIDERS.append((x, -y, 0.38 * s))


for x, y, s in [
    (-4.6, -3.3, 1.05),
    (-6.8, -0.8, 0.95),
    (5.7, -2.3, 1.08),
    (7, 1, 1),
    (-6, 3.4, 1.12),
    (4.9, 4, 1.1),
    (2.8, 5.4, 0.83),
]:
    tree(x, y, s)
# Cottage at rear of plaza, front faces camera (negative Blender Y).
x, y = -2.9, 3.1
cube("cottage", (x, y, 0.93), (2.5, 1.9, 1.78), wall, world=True, bevel=0.09)
# Gabled triangular roof as editable mesh.
vs = [
    (x + dx, y + dy, z)
    for dy in [-1.12, 1.12]
    for dx, z in [(-1.45, 1.75), (0, 2.65), (1.45, 1.75)]
]
me = bpy.data.meshes.new("gable")
me.from_pydata(vs, [], [(0, 1, 2), (5, 4, 3), (0, 3, 4, 1), (1, 4, 5, 2), (2, 5, 3, 0)])
o = bpy.data.objects.new("gable roof", me)
bpy.context.collection.objects.link(o)
register(o, "gable roof", roof, world=True)
cube("door", (x, y - 0.967, 0.57), (0.55, 0.045, 1.10), teal, world=True, bevel=0.07)
ell("door knob", (x + 0.18, y - 1.01, 0.58), (0.027, 0.022, 0.027), dress, world=True)
for sg in [-1, 1]:
    cube(
        "window frame",
        (x + sg * 0.82, y - 0.98, 1.14),
        (0.58, 0.05, 0.60),
        cream,
        world=True,
        bevel=0.04,
    )
    cube(
        "window glass",
        (x + sg * 0.82, y - 1.015, 1.14),
        (0.45, 0.02, 0.47),
        glass,
        world=True,
    )
    cube(
        "window mullion",
        (x + sg * 0.82, y - 1.03, 1.14),
        (0.035, 0.02, 0.48),
        cream,
        world=True,
    )
    cube(
        "window crossbar",
        (x + sg * 0.82, y - 1.035, 1.14),
        (0.46, 0.02, 0.032),
        cream,
        world=True,
    )
    cube(
        "flower box",
        (x + sg * 0.82, y - 1.10, 0.85),
        (0.62, 0.22, 0.16),
        wood,
        world=True,
        bevel=0.025,
    )
cube(
    "chimney",
    (x - 0.8, y + 0.24, 2.43),
    (0.32, 0.37, 0.87),
    wall,
    world=True,
    bevel=0.03,
)
COLLIDERS.extend([(x - 0.72, -y, 1.15), (x + 0.72, -y, 1.15)])
# Bench on east side of plaza, exact sitting anchor exported with scene layout.
BENCH = (3.05, -0.55, 0.38)
for dy in [-0.14, 0, 0.14]:
    cube(
        "bench seat",
        (3.05, 0.55 + dy, 0.43),
        (1.5, 0.115, 0.09),
        wood,
        world=True,
        bevel=0.025,
    )
for z in [0.78, 1.00]:
    cube(
        "bench back",
        (3.05, 0.80, z),
        (1.52, 0.075, 0.14),
        wood,
        world=True,
        bevel=0.025,
    )
for xx in [2.47, 3.63]:
    for yy in [0.44, 0.72]:
        cube("bench leg", (xx, yy, 0.24), (0.09, 0.09, 0.43), teal, world=True)
COLLIDERS.append((3.05, -0.55, 0.85))
# Dock and small rope bollards at front beach.
for i in range(12):
    cube(
        "dock plank",
        (0.1, -6.8 - i * 0.25, 0.10),
        (1.7, 0.21, 0.12),
        wood,
        world=True,
        bevel=0.02,
    )
for xx in [-0.7, 0.9]:
    for yy in [-6.85, -8.9]:
        cone("dock post", (xx, yy, 0.24), 0.07, 0.07, 0.62, wood, world=True)
# Flowers, meadow tufts, stepping stones and water glints.
for i in range(35):
    a = random.random() * math.tau
    r = random.uniform(3.8, 7.1)
    x = math.cos(a) * r * 1.1
    y = math.sin(a) * r * 0.78
    if y > 1.5 and -4.5 < x < -1:
        continue
    line("flower stem", [(x, y, 0.1), (x, y, 0.34)], 0.014, leaf, world=True)
    for k in range(5):
        ang = k * math.tau / 5
        disc(
            "flower petal",
            (x + 0.075 * math.cos(ang), y + 0.075 * math.sin(ang), 0.36),
            (0.057, 0.057, 0.028),
            pink if i % 3 else cream,
            seg=6,
        )
    disc(
        "flower heart",
        (x, y, 0.382),
        (0.036, 0.036, 0.024),
        dress,
        seg=8,
    )
for i in range(28):
    a = i * math.tau / 28
    rx = 11.5 + (i % 3) * 0.5
    ry = 9.3 + (i % 3) * 0.4
    line(
        "water glint",
        [
            (rx * math.cos(a) - 0.23, ry * math.sin(a), -0.15),
            (rx * math.cos(a) + 0.23, ry * math.sin(a), -0.15),
        ],
        0.026,
        foam,
        world=True,
    )
for xx, yy in [(-7, -3.6), (7, 3.5), (-3.5, -5.4)]:
    ell(
        "shore stone",
        (xx, yy, 0.20),
        (0.47, 0.35, 0.30),
        stone,
        world=True,
        seg=10,
        rings=6,
    )
    COLLIDERS.append((xx, -yy, 0.42))
# Export portable profile: bind-space colored triangles + the actual bone TRS.
C = Matrix(((1, 0, 0, 0), (0, 0, 1, 0), (0, -1, 0, 0), (0, 0, 0, 1)))


def putstr(f, s):
    b = s.encode()
    f.write(struct.pack("<H", len(b)))
    f.write(b)


def trs(m):
    t, q, s = m.decompose()
    return (*t, q.x, q.y, q.z, q.w, *s)


def export_p3m(path, objects, animated):
    rig.animation_data_clear()
    reset()
    bpy.context.view_layer.update()
    names = ["identity"] + (bones if animated else [])
    parents = [-1]
    rests = [Matrix.Identity(4)]
    if animated:
        for n in bones:
            b = rig.data.bones[n]
            parents.append(names.index(b.parent.name) if b.parent else 0)
            g = C @ b.matrix_local @ C.inverted()
            par = (
                C @ b.parent.matrix_local @ C.inverted()
                if b.parent
                else Matrix.Identity(4)
            )
            rests.append(par.inverted() @ g)
    verts = []
    indices = []
    for o in objects:
        # Original mesh with transforms, BEFORE skin deformation.
        me = o.data
        me.calc_loop_triangles()
        ma = o.data.materials[0].diffuse_color[:3]
        base = len(verts)
        joint = names.index(OWN.get(o.name, "head")) if animated else 0
        normal = (C @ o.matrix_world).to_3x3().inverted().transposed()
        for v in me.vertices:
            p = C @ o.matrix_world @ v.co
            n = (normal @ v.normal).normalized()
            verts.append((*p, *n, *ma, joint))
        for tri in me.loop_triangles:
            indices.extend(base + i for i in tri.vertices)
    with open(path, "wb") as f:
        f.write(b"P3M1")
        f.write(
            struct.pack(
                "<IIII",
                len(names),
                len(verts),
                len(indices),
                len(CLIPS) if animated else 0,
            )
        )
        for n, p, m in zip(names, parents, rests):
            putstr(f, n)
            f.write(struct.pack("<i10f", p, *trs(m)))
        for v in verts:
            f.write(struct.pack("<9fH", *v))
        f.write(struct.pack("<" + "I" * len(indices), *indices))
        if animated:
            for kind, dur, action in CLIPS:
                rig.animation_data_create()
                rig.animation_data.action = action
                facial = kind.startswith("Expression_")
                active = [
                    n
                    for n in bones
                    if (n.startswith("face.") or n == "blink") == facial
                ]
                count = 1 if facial else round(dur * FPS) + 1
                samples = {n: [] for n in active}
                for frame in range(count):
                    reset()
                    bpy.context.scene.frame_set(frame + 1)
                    bpy.context.view_layer.update()
                    for n in active:
                        p = rig.pose.bones[n]
                        g = C @ p.matrix @ C.inverted()
                        par = (
                            C @ p.parent.matrix @ C.inverted()
                            if p.parent
                            else Matrix.Identity(4)
                        )
                        samples[n].append(trs(par.inverted() @ g))
                putstr(f, kind)
                f.write(struct.pack("<fI", dur, len(active) * (1 if facial else 3)))
                for n in active:
                    for prop, off, sz in (
                        [(2, 7, 3)] if facial else [(0, 0, 3), (1, 3, 4), (2, 7, 3)]
                    ):
                        f.write(struct.pack("<HBH", names.index(n), prop, count))
                        for k, val in enumerate(samples[n]):
                            f.write(
                                struct.pack(
                                    "<" + "f" * (sz + 1),
                                    dur * k / max(1, count - 1),
                                    *val[off : off + sz],
                                )
                            )
                rig.animation_data_clear()
    return {
        "vertices": len(verts),
        "triangles": len(indices) // 3,
        "bones": len(names),
        "clips": [c[0] for c in CLIPS] if animated else [],
        "bytes": path.stat().st_size,
    }


character = export_p3m(ROOT / "mira.p3m", CHAR, True)
world = export_p3m(ROOT / "island.p3m", WORLD, False)
(ROOT / "layout.rs").write_text(
    "// Generated by assets/build_island.py; world coordinates are Y up.\npub const COLLIDERS: &[(f32,f32,f32)] = &[\n"
    + "".join(f"    ({x:.5f}, {z:.5f}, {r:.5f}),\n" for x, z, r in COLLIDERS)
    + "];\npub const BENCH: (f32,f32,f32) = (3.05, -0.55, 0.38);\n"
    + "".join(f"pub const {name.upper()}_STRIDE: f32 = {gait['stride']};\n" for name, gait in GAITS.items())
)
(ROOT / "manifest.json").write_text(
    json.dumps(
        {
            "author": "PocketJS contributors",
            "license": "MIT",
            "character": "Mira",
            "source": "build_island.py; original geometry and animation",
            "blender": bpy.app.version_string,
            "profile": "P3M1 rigid skin, linear TRS, vertex color",
            "character_asset": character,
            "island_asset": world,
            "expressions": [
                "neutral",
                "happy",
                "sad",
                "surprised",
                "angry",
                "shy",
                "sleepy",
            ],
            "fps": FPS,
            "locomotion": GAITS,
        },
        indent=2,
    )
    + "\n"
)
# GLB retains named actions and the editable skeleton for future avatar tools.
rig.animation_data_clear()
reset()
for n in bones:
    if n.startswith("face.") or n == "blink":
        rig.pose.bones[n].scale = (1, 1, 1) if n == "face.neutral" else (0.0001,) * 3
bpy.ops.object.select_all(action="DESELECT")
rig.select_set(True)
for o in CHAR:
    o.select_set(True)
bpy.context.view_layer.objects.active = rig
for _, _, action in CLIPS:
    action.use_fake_user = True
rig.animation_data_create()
rig.animation_data.action = CLIPS[0][2]
bpy.ops.export_scene.gltf(
    filepath=str(ROOT / "mira.glb"),
    use_selection=True,
    export_format="GLB",
    export_animation_mode="ACTIONS",
    export_force_sampling=True,
    export_frame_range=False,
    export_anim_single_armature=True,
    export_materials="EXPORT",
)
bpy.ops.object.select_all(action="DESELECT")
for o in WORLD:
    o.select_set(True)
bpy.ops.export_scene.gltf(
    filepath=str(ROOT / "island.glb"),
    use_selection=True,
    export_format="GLB",
    export_animations=False,
)
# Render a character portrait and a faithful island composition.
scene = bpy.context.scene
scene.render.engine = "CYCLES"
scene.cycles.samples = 24
scene.world.color = (0.65, 0.65, 0.65)
scene.view_settings.view_transform = "Standard"
scene.render.image_settings.file_format = "PNG"
scene.render.film_transparent = False
bpy.ops.object.light_add(type="AREA", location=(-3, -4, 7))
bpy.context.object.data.energy = 450
bpy.context.object.data.shape = "DISK"
bpy.context.object.data.size = 5
bpy.ops.object.light_add(type="AREA", location=(3, 2, 5))
bpy.context.object.data.energy = 250
bpy.context.object.data.size = 4
bpy.ops.object.camera_add()
cam = bpy.context.object
scene.camera = cam
cam.data.type = "ORTHO"


def camera_at(pos, target, ortho):
    cam.location = pos
    cam.rotation_euler = (
        (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
    )
    cam.data.ortho_scale = ortho


# Posed wave portrait on a solid backdrop.
rig.animation_data_create()
rig.animation_data.action = CLIPS[6][2]
scene.frame_set(18)
for n in bones:
    if n.startswith("face.") or n == "blink":
        rig.pose.bones[n].scale = (1, 1, 1) if n == "face.happy" else (0.0001,) * 3
bpy.context.view_layer.update()
for o in WORLD:
    o.hide_render = True
scene.world.use_nodes = True
scene.world.node_tree.nodes["Background"].inputs[0].default_value = (
    0.55,
    0.67,
    0.60,
    1,
)
scene.world.node_tree.nodes["Background"].inputs[1].default_value = 0.6
camera_at((2.4, -6, 2.4), (0, 0, 0.95), 2.25)
scene.render.resolution_x = 640
scene.render.resolution_y = 640
scene.render.resolution_percentage = 100
scene.render.filepath = str(ROOT / "mira-preview.png")
bpy.ops.render.render(write_still=True)
for o in WORLD:
    o.hide_render = False
rig.animation_data_clear()
reset()
for n in bones:
    if n.startswith("face.") or n == "blink":
        rig.pose.bones[n].scale = (1, 1, 1) if n == "face.neutral" else (0.0001,) * 3
camera_at((0, -11, 8), (0, 0, 0.5), 12.8)
scene.render.resolution_x = 1000
scene.render.resolution_y = 600
scene.render.filepath = str(ROOT / "island-preview.png")
bpy.ops.render.render(write_still=True)
# Save with all actions retained and a pleasant, editable scene on opening.
for _, _, a in CLIPS:
    a.use_fake_user = True
scene.render.engine = "CYCLES"
scene.frame_start = 1
scene.frame_end = 72
scene.render.fps = FPS
bpy.ops.wm.save_as_mainfile(filepath=str(ROOT / "pocket-island.blend"))
print("POCKET_ISLAND_ASSETS", json.dumps({"character": character, "world": world}))
