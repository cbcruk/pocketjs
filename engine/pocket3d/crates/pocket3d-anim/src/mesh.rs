//! P3M1: bounded colored triangle meshes with rigid skeletal weights.
//! Assets use metres, +Y up, +Z forward. All integers/floats are little endian.
use crate::{Channel, ChannelPath, Clip, Interpolation, NodeTrs, Skeleton};
use alloc::{string::String, vec, vec::Vec};
use glam::{Mat4, Quat, Vec3};

#[derive(Debug, PartialEq)]
pub enum AssetError {
    Truncated,
    Format,
    Limit,
    Invalid,
}
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], AssetError> {
        let end = self.offset.checked_add(n).ok_or(AssetError::Limit)?;
        let result = self
            .bytes
            .get(self.offset..end)
            .ok_or(AssetError::Truncated)?;
        self.offset = end;
        Ok(result)
    }
    fn u8(&mut self) -> Result<u8, AssetError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, AssetError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, AssetError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn float(&mut self) -> Result<f32, AssetError> {
        let f = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        if !f.is_finite() {
            return Err(AssetError::Invalid);
        }
        Ok(f)
    }
    fn vec3(&mut self) -> Result<Vec3, AssetError> {
        Ok(Vec3::new(self.float()?, self.float()?, self.float()?))
    }
    fn name(&mut self) -> Result<String, AssetError> {
        let len = self.u16()? as usize;
        if len > 128 {
            return Err(AssetError::Limit);
        }
        Ok(core::str::from_utf8(self.take(len)?)
            .map_err(|_| AssetError::Invalid)?
            .into())
    }
    fn count(&mut self, max: usize) -> Result<usize, AssetError> {
        let n = self.u32()? as usize;
        if n > max {
            Err(AssetError::Limit)
        } else {
            Ok(n)
        }
    }
}
#[derive(Clone, Copy)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub color: Vec3,
    pub joint: usize,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ColorVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

pub struct MeshAsset {
    pub skeleton: Skeleton,
    pub names: Vec<String>,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub clips: Vec<Clip>,
    pub inverse_bind: Vec<Mat4>,
}
impl MeshAsset {
    pub fn decode(bytes: &[u8]) -> Result<Self, AssetError> {
        // A caller may decode an external asset; reject unbounded allocations.
        if bytes.len() > 32 * 1024 * 1024 {
            return Err(AssetError::Limit);
        }
        let mut r = Reader { bytes, offset: 0 };
        if r.take(4)? != b"P3M1" {
            return Err(AssetError::Format);
        }
        let nodes = r.count(256)?;
        let nv = r.count(100_000)?;
        let ni = r.count(300_000)?;
        let nc = r.count(64)?;
        if nodes == 0 || ni % 3 != 0 {
            return Err(AssetError::Invalid);
        }
        let mut names = Vec::with_capacity(nodes);
        let mut parents = Vec::with_capacity(nodes);
        let mut rest = Vec::with_capacity(nodes);
        for node in 0..nodes {
            names.push(r.name()?);
            let p = r.u32()?;
            if p != u32::MAX && p as usize >= node {
                return Err(AssetError::Invalid);
            }
            parents.push(if p == u32::MAX {
                usize::MAX
            } else {
                p as usize
            });
            let translation = r.vec3()?;
            let rotation = Quat::from_xyzw(r.float()?, r.float()?, r.float()?, r.float()?);
            let scale = r.vec3()?;
            if rotation.length_squared() < 0.9
                || rotation.length_squared() > 1.1
                || scale.abs().min_element() < 0.00001
            {
                return Err(AssetError::Invalid);
            }
            rest.push(NodeTrs {
                translation,
                rotation: rotation.normalize(),
                scale,
            });
        }
        let mut vertices = Vec::with_capacity(nv);
        for _ in 0..nv {
            let position = r.vec3()?;
            let normal = r.vec3()?;
            let color = r.vec3()?;
            let joint = r.u16()? as usize;
            if joint >= nodes || color.min_element() < 0.0 || color.max_element() > 1.0 {
                return Err(AssetError::Invalid);
            }
            vertices.push(Vertex {
                position,
                normal,
                color,
                joint,
            });
        }
        let mut indices = Vec::with_capacity(ni);
        for _ in 0..ni {
            let i = r.u32()?;
            if i as usize >= nv {
                return Err(AssetError::Invalid);
            }
            indices.push(i);
        }
        let mut clips = Vec::with_capacity(nc);
        let mut total_keys = 0usize;
        for _ in 0..nc {
            let name = r.name()?;
            let duration = r.float()?;
            let count = r.count(nodes * 3)?;
            if duration <= 0.0 || duration > 3600.0 {
                return Err(AssetError::Invalid);
            }
            let mut channels = Vec::with_capacity(count);
            for _ in 0..count {
                let node = r.u16()? as usize;
                let path = match r.u8()? {
                    0 => ChannelPath::Translation,
                    1 => ChannelPath::Rotation,
                    2 => ChannelPath::Scale,
                    _ => return Err(AssetError::Invalid),
                };
                let keys = r.u16()? as usize;
                total_keys += keys;
                if node >= nodes || keys == 0 || total_keys > 500_000 {
                    return Err(AssetError::Limit);
                }
                let width = if path == ChannelPath::Rotation { 4 } else { 3 };
                let mut times = Vec::with_capacity(keys);
                let mut values = Vec::with_capacity(keys * width);
                for key in 0..keys {
                    let t = r.float()?;
                    if t < 0.0 || t > duration + 0.001 || (key > 0 && t <= times[key - 1]) {
                        return Err(AssetError::Invalid);
                    }
                    times.push(t);
                    for _ in 0..width {
                        values.push(r.float()?);
                    }
                    if path == ChannelPath::Rotation {
                        let off = key * 4;
                        let q = Quat::from_slice(&values[off..off + 4]);
                        if (q.length_squared() - 1.0).abs() > 0.1 {
                            return Err(AssetError::Invalid);
                        }
                    }
                }
                channels.push(Channel {
                    node,
                    path,
                    interpolation: Interpolation::Linear,
                    times,
                    values,
                });
            }
            clips.push(Clip {
                name,
                duration,
                channels,
            });
        }
        if r.offset != bytes.len() {
            return Err(AssetError::Format);
        }
        let skeleton = Skeleton {
            parents,
            rest,
            order: (0..nodes).collect(),
        };
        let mut bind = Vec::new();
        skeleton.globals_from_locals(&skeleton.rest, &mut bind);
        let inverse_bind = bind.into_iter().map(|m| m.inverse()).collect();
        Ok(Self {
            skeleton,
            names,
            vertices,
            indices,
            clips,
            inverse_bind,
        })
    }
    pub fn clip(&self, name: &str) -> Option<usize> {
        self.clips.iter().position(|c| c.name == name)
    }
    /// Skin unique vertices once, then expand indices for backends with a
    /// streaming triangle buffer. Reuse `scratch` and `output` across frames.
    pub fn skin(
        &self,
        globals: &[Mat4],
        model: Mat4,
        scratch: &mut Vec<ColorVertex>,
        output: &mut Vec<ColorVertex>,
    ) {
        let palette: Vec<(Mat4, bool)> = globals
            .iter()
            .zip(&self.inverse_bind)
            .map(|(g, b)| {
                let m = model * *g * *b;
                let visible = m
                    .x_axis
                    .truncate()
                    .length_squared()
                    .max(m.y_axis.truncate().length_squared())
                    .max(m.z_axis.truncate().length_squared())
                    >= 1e-6;
                (m, visible)
            })
            .collect();
        let light_direction = Vec3::new(-0.42, 0.82, 0.38).normalize();
        // Material runs share the same base color. sqrt(c * l) = sqrt(c) *
        // sqrt(l), so keep three color roots per run and one per vertex.
        // This call-local cache also respects callers editing public vertices.
        let mut last_color = Vec3::splat(-1.0);
        let mut display_color = Vec3::ZERO;
        scratch.clear();
        for v in &self.vertices {
            let (m, visible) = palette[v.joint];
            if !visible {
                scratch.push(ColorVertex::default());
                continue;
            }
            let pos = m.transform_point3(v.position);
            let normal = m.transform_vector3(v.normal).normalize_or_zero();
            let diffuse = normal.dot(light_direction).max(0.0);
            let light = 0.69 + 0.31 * diffuse;
            // Blender material factors are linear. PICA's framebuffer has no
            // sRGB conversion; a sqrt transfer preserves the pastel palette.
            if v.color != last_color {
                display_color = v.color.sqrt();
                last_color = v.color;
            }
            let rgb = display_color * Vec3::splat(light).sqrt().x;
            scratch.push(ColorVertex {
                position: pos.to_array(),
                color: [rgb.x, rgb.y, rgb.z, 1.0],
            });
        }
        output.clear();
        for tri in self.indices.as_chunks::<3>().0 {
            if tri.iter().all(|&i| scratch[i as usize].color[3] == 0.0) {
                continue;
            }
            output.extend(tri.iter().map(|&i| scratch[i as usize]));
        }
    }
    pub fn rest_globals(&self) -> Vec<Mat4> {
        let mut g = vec![];
        self.skeleton
            .globals_from_locals(&self.skeleton.rest, &mut g);
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_runs_match_reference_skinning_across_transforms_and_visibility() {
        let mut asset = MeshAsset {
            skeleton: Skeleton {
                parents: vec![usize::MAX; 2],
                rest: vec![NodeTrs::IDENTITY; 2],
                order: vec![0, 1],
            },
            names: vec![],
            clips: vec![],
            inverse_bind: vec![Mat4::IDENTITY; 2],
            indices: (0..6).collect(),
            vertices: (0..6)
                .map(|i| Vertex {
                    position: Vec3::new(i as f32, 0.5, -0.2),
                    normal: Vec3::new(0.3, 0.8, -0.4),
                    color: if i < 4 {
                        Vec3::new(0., 0.25, 1.)
                    } else {
                        Vec3::new(0.8, 0.02, 0.3)
                    },
                    joint: i / 3,
                })
                .collect(),
        };
        for scale in [Vec3::ONE, Vec3::new(0.3, 2., 1.2)] {
            for hidden in [false, true] {
                let model = Mat4::from_rotation_translation(Quat::from_rotation_y(0.8), Vec3::X);
                let globals = [
                    Mat4::from_scale_rotation_translation(
                        scale,
                        Quat::from_rotation_x(-0.4),
                        Vec3::Y,
                    ),
                    Mat4::from_scale(Vec3::splat(if hidden { 0.0001 } else { 1. })),
                ];
                let mut scratch = vec![];
                let mut out = vec![];
                asset.skin(&globals, model, &mut scratch, &mut out);
                assert_eq!(out.len(), if hidden { 3 } else { 6 });
                for (v, actual) in asset.vertices.iter().zip(&out) {
                    let m = model * globals[v.joint];
                    let normal = m.transform_vector3(v.normal).normalize_or_zero();
                    let light =
                        0.69 + 0.31 * normal.dot(Vec3::new(-0.42, 0.82, 0.38).normalize()).max(0.);
                    let expected = (v.color * light).sqrt();
                    assert!(
                        (Vec3::from_array(actual.position) - m.transform_point3(v.position))
                            .length()
                            < 1e-6
                    );
                    assert!(
                        (Vec3::from_slice(&actual.color) - expected)
                            .abs()
                            .max_element()
                            < 2e-7
                    );
                    assert_eq!(actual.color[3], 1.);
                }
                // Editing a public vertex must not reuse stale material data.
                asset.vertices[0].color = Vec3::splat(0.1);
            }
        }
    }
}
