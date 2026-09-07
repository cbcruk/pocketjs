//! Pocket Island application state. No 3DS SDK types enter the simulation.
#![no_std]
extern crate alloc;
use alloc::{
    collections::VecDeque,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use pocket3d_anim::glam::{Mat4, Quat, Vec3};
use pocket3d_anim::{
    NodeTrs,
    mesh::{ColorVertex, MeshAsset},
};

mod layout {
    include!("../assets/layout.rs");
}
pub const STEP: f32 = 1.0 / 30.0;
pub const MAX_MESSAGE_BYTES: usize = 192;
pub const HISTORY_LIMIT: usize = 32;
pub const BUBBLE_TICKS: u64 = 210;
pub const EXPRESSIONS: [&str; 7] = [
    "neutral",
    "happy",
    "sad",
    "surprised",
    "angry",
    "shy",
    "sleepy",
];
pub const QUICK_MESSAGES: [&str; 4] = [
    "Hello, island!",
    "Let's take a walk.",
    "This is my happy place.",
    "See you by the sea!",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Action {
    Idle,
    Walk,
    Run,
    SitDown,
    SitIdle,
    StandUp,
    Wave,
    Cheer,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Walk => "Walk",
            Self::Run => "Run",
            Self::SitDown => "SitDown",
            Self::SitIdle => "SitIdle",
            Self::StandUp => "StandUp",
            Self::Wave => "Wave",
            Self::Cheer => "Cheer",
        }
    }
    fn looping(self) -> bool {
        matches!(self, Self::Idle | Self::Walk | Self::Run | Self::SitIdle)
    }
}
#[derive(Clone, Copy, Default)]
pub struct Input {
    pub x: f32,
    pub z: f32,
    pub run: bool,
    pub wave: bool,
    pub sit: bool,
    pub cheer: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Local,
    Pending,
    Delivered,
    Failed,
}
#[derive(Clone, Debug)]
pub struct Message {
    pub id: u64,
    pub sender: u32,
    pub sequence: u64,
    pub body: String,
    pub received_tick: u64,
    pub delivery: Delivery,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageError {
    Empty,
    TooLong,
    Control,
    RateLimited,
    UnknownPeer,
    Duplicate,
    HistoryFull,
}

/// A future transport supplies authenticated peer IDs at this boundary.
/// Local mode creates no network connection and claims no remote delivery.
pub struct Chat {
    pub history: VecDeque<Message>,
    pub local_peer: u32,
    next_id: u64,
    last_sent: Option<u64>,
    peers: Vec<(u32, u64)>,
}
impl Chat {
    pub fn new(local_peer: u32) -> Self {
        Self {
            history: VecDeque::new(),
            local_peer,
            next_id: 1,
            last_sent: None,
            peers: vec![],
        }
    }
    pub fn add_peer(&mut self, id: u32) -> bool {
        if id == self.local_peer || self.peers.len() >= 7 || self.peers.iter().any(|p| p.0 == id) {
            return false;
        }
        self.peers.push((id, 0));
        true
    }
    pub fn remove_peer(&mut self, id: u32) {
        self.peers.retain(|p| p.0 != id);
    }
    fn text(body: &str) -> Result<&str, MessageError> {
        let body = body.trim();
        if body.is_empty() {
            return Err(MessageError::Empty);
        }
        if body.len() > MAX_MESSAGE_BYTES {
            return Err(MessageError::TooLong);
        }
        if body
            .chars()
            .any(|c| c.is_control() || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'))
        {
            return Err(MessageError::Control);
        }
        Ok(body)
    }
    fn append(&mut self, m: Message) -> Result<(), MessageError> {
        if self.history.len() == HISTORY_LIMIT {
            // Never discard an unacknowledged outbound message to admit a new one.
            let Some(index) = self
                .history
                .iter()
                .position(|m| m.delivery != Delivery::Pending)
            else {
                return Err(MessageError::HistoryFull);
            };
            self.history.remove(index);
        }
        self.history.push_back(m);
        Ok(())
    }
    pub fn send(&mut self, body: &str, tick: u64, online: bool) -> Result<u64, MessageError> {
        let text = Self::text(body)?.to_string();
        if self.last_sent.is_some_and(|t| tick.saturating_sub(t) < 15) {
            return Err(MessageError::RateLimited);
        }
        let id = self.next_id;
        self.append(Message {
            id,
            sender: self.local_peer,
            sequence: id,
            body: text,
            received_tick: tick,
            delivery: if online {
                Delivery::Pending
            } else {
                Delivery::Local
            },
        })?;
        self.next_id += 1;
        self.last_sent = Some(tick);
        Ok(id)
    }
    pub fn receive(
        &mut self,
        sender: u32,
        sequence: u64,
        body: &str,
        tick: u64,
    ) -> Result<(), MessageError> {
        let body = Self::text(body)?.to_string();
        let index = self
            .peers
            .iter()
            .position(|p| p.0 == sender)
            .ok_or(MessageError::UnknownPeer)?;
        if sequence <= self.peers[index].1 {
            return Err(MessageError::Duplicate);
        }
        self.append(Message {
            id: sequence,
            sender,
            sequence,
            body,
            received_tick: tick,
            delivery: Delivery::Delivered,
        })?;
        self.peers[index].1 = sequence;
        Ok(())
    }
    pub fn acknowledge(&mut self, id: u64, ok: bool) -> bool {
        let Some(m) = self
            .history
            .iter_mut()
            .find(|m| m.id == id && m.sender == self.local_peer && m.delivery == Delivery::Pending)
        else {
            return false;
        };
        m.delivery = if ok {
            Delivery::Delivered
        } else {
            Delivery::Failed
        };
        true
    }
    pub fn bubble(&self, sender: u32, tick: u64) -> Option<&Message> {
        self.history
            .iter()
            .rev()
            .find(|m| m.sender == sender && tick.saturating_sub(m.received_tick) < BUBBLE_TICKS)
    }
}

pub struct Island {
    pub position: Vec3,
    pub yaw: f32,
    pub camera: Vec3,
    pub action: Action,
    pub expression: usize,
    pub action_time: f32,
    pub tick: u64,
    pub chat: Chat,
    pub on_bench: bool,
    pub actor: MeshAsset,
    pub terrain: Vec<ColorVertex>,
    pub character: Vec<ColorVertex>,
    locals: Vec<NodeTrs>,
    old_locals: Vec<NodeTrs>,
    globals: Vec<Mat4>,
    scratch: Vec<ColorVertex>,
    blend: f32,
}
impl Default for Island {
    fn default() -> Self {
        Self::new()
    }
}
impl Island {
    pub fn new() -> Self {
        let actor =
            MeshAsset::decode(include_bytes!("../assets/mira.p3m")).expect("validated Mira asset");
        let world = MeshAsset::decode(include_bytes!("../assets/island.p3m"))
            .expect("validated island asset");
        let mut terrain = vec![];
        world.skin(
            &world.rest_globals(),
            Mat4::IDENTITY,
            &mut vec![],
            &mut terrain,
        );
        let locals = actor.skeleton.rest.clone();
        let mut s = Self {
            position: Vec3::new(0., 0.11, 1.6),
            yaw: 0.,
            camera: Vec3::new(0., 0., 1.6),
            action: Action::Idle,
            expression: 0,
            action_time: 0.,
            tick: 0,
            chat: Chat::new(1),
            on_bench: false,
            actor,
            terrain,
            character: vec![],
            old_locals: locals.clone(),
            locals,
            globals: vec![],
            scratch: vec![],
            blend: 1.,
        };
        s.animate();
        s
    }
    fn clip_name(&self) -> &'static str {
        if self.on_bench {
            match self.action {
                Action::SitDown => "BenchSitDown",
                Action::SitIdle => "BenchSitIdle",
                Action::StandUp => "BenchStandUp",
                _ => self.action.name(),
            }
        } else {
            self.action.name()
        }
    }
    pub fn set_expression(&mut self, e: usize) {
        self.expression = e % EXPRESSIONS.len();
    }
    pub fn send(&mut self, text: &str) -> Result<u64, MessageError> {
        self.chat.send(text, self.tick, false)
    }
    fn change(&mut self, a: Action) {
        if self.action == a {
            return;
        }
        self.old_locals.clone_from(&self.locals);
        self.action = a;
        self.action_time = 0.;
        self.blend = 0.;
    }
    pub fn walkable(x: f32, z: f32) -> bool {
        if !x.is_finite() || !z.is_finite() {
            return false;
        }
        let on_island = x * x / (10.1 * 10.1) + z * z / (8.1 * 8.1) < 1.;
        let on_dock = (-0.52..=0.72).contains(&x) && (6.4..=9.1).contains(&z);
        (on_island || on_dock)
            && !layout::COLLIDERS.iter().any(|&(cx, cz, r)| {
                let d = (x - cx) * (x - cx) + (z - cz) * (z - cz);
                d < (r + 0.23) * (r + 0.23)
            })
    }
    pub fn ground_height(x: f32, z: f32) -> f32 {
        if (-0.70..=0.90).contains(&x) && (6.69..=9.60).contains(&z) {
            0.17
        } else if x * x / (9.15 * 9.15) + z * z / (7.15 * 7.15) < 1.0 {
            0.11
        } else {
            -0.01
        }
    }
    pub fn step(&mut self, input: Input) {
        self.tick += 1;
        self.action_time += STEP;
        let mut dir = Vec3::new(
            if input.x.is_finite() {
                input.x.clamp(-1., 1.)
            } else {
                0.
            },
            0.,
            if input.z.is_finite() {
                input.z.clamp(-1., 1.)
            } else {
                0.
            },
        );
        if dir.length() < 0.16 {
            dir = Vec3::ZERO
        } else {
            dir = dir.clamp_length_max(1.)
        }
        let moving = dir.length_squared() > 0.;
        if input.sit && !matches!(self.action, Action::SitDown | Action::StandUp) {
            if self.action == Action::SitIdle {
                self.change(Action::StandUp)
            } else {
                let (x, z, _) = layout::BENCH;
                if (self.position - Vec3::new(x, 0.09, z)).length() < 1.65 {
                    self.position.x = x;
                    self.position.z = z - 0.02;
                    self.on_bench = true;
                    self.yaw = 0.;
                }
                self.yaw = 0.0;
                self.change(Action::SitDown)
            }
        }
        let duration = self.actor.clips[self.actor.clip(self.clip_name()).unwrap()].duration;
        if self.action_time >= duration {
            match self.action {
                Action::SitDown => self.change(Action::SitIdle),
                Action::StandUp => {
                    self.on_bench = false;
                    self.position.z += if self.near_bench() { 1.35 } else { 0. };
                    self.change(Action::Idle)
                }
                Action::Wave | Action::Cheer => self.change(Action::Idle),
                _ => {}
            }
        }
        if moving && self.action == Action::SitIdle {
            self.change(Action::StandUp)
        }
        if !matches!(
            self.action,
            Action::SitDown | Action::SitIdle | Action::StandUp
        ) {
            if moving {
                self.change(if input.run { Action::Run } else { Action::Walk });
                let step = dir * (if input.run { 2.65 } else { 1.45 }) * STEP;
                let next = self.position + step;
                if Self::walkable(next.x, self.position.z) {
                    self.position.x = next.x;
                }
                if Self::walkable(self.position.x, next.z) {
                    self.position.z = next.z;
                }
                let target = libm::atan2f(dir.x, dir.z);
                let delta =
                    libm::atan2f(libm::sinf(target - self.yaw), libm::cosf(target - self.yaw));
                self.yaw += delta * 0.24;
            } else if matches!(self.action, Action::Walk | Action::Run) {
                self.change(Action::Idle)
            }
            if input.wave {
                self.yaw = 0.0;
                self.change(Action::Wave)
            }
            if input.cheer {
                self.yaw = 0.0;
                self.change(Action::Cheer)
            }
        }
        self.position.y = Self::ground_height(self.position.x, self.position.z);
        let target = Vec3::new(
            self.position.x.clamp(-4.4, 4.4),
            0.,
            self.position.z.clamp(-3.0, 4.5),
        );
        self.camera = self.camera.lerp(target, 0.075);
        self.animate();
    }
    fn near_bench(&self) -> bool {
        let (x, z, _) = layout::BENCH;
        (self.position.x - x).abs() < 0.1 && (self.position.z - z).abs() < 0.15
    }
    pub fn animate(&mut self) {
        let clip = &self.actor.clips[self.actor.clip(self.clip_name()).unwrap()];
        self.actor.skeleton.sample_locals(
            Some(clip),
            self.action_time,
            self.action.looping(),
            &mut self.locals,
        );
        self.blend = (self.blend + STEP / 0.16).min(1.);
        if self.blend < 1. {
            for (i, n) in self.locals.iter_mut().enumerate() {
                let old = self.old_locals[i];
                n.translation = old.translation.lerp(n.translation, self.blend);
                n.rotation = old.rotation.slerp(n.rotation, self.blend);
                n.scale = old.scale.lerp(n.scale, self.blend);
            }
        }
        // Face selection is independent of body action and never resets its clock.
        let blink = self.tick % 123 >= 119 && ![1, 6].contains(&self.expression);
        for (i, name) in self.actor.names.iter().enumerate() {
            if let Some(exp) = name.strip_prefix("face.") {
                self.locals[i].scale =
                    Vec3::splat(if exp == EXPRESSIONS[self.expression] && !blink {
                        1.
                    } else {
                        0.0001
                    });
            }
            if name == "blink" {
                self.locals[i].scale = Vec3::splat(if blink { 1. } else { 0.0001 });
            }
        }
        self.actor
            .skeleton
            .globals_from_locals(&self.locals, &mut self.globals);
        let model = Mat4::from_rotation_translation(Quat::from_rotation_y(self.yaw), self.position);
        self.actor
            .skin(&self.globals, model, &mut self.scratch, &mut self.character);
        let mut shadow = Vec::with_capacity(96);
        let y = Self::ground_height(self.position.x, self.position.z) + 0.002;
        for i in 0..32 {
            let a = i as f32 * core::f32::consts::TAU / 32.0;
            let b = (i + 1) as f32 * core::f32::consts::TAU / 32.0;
            shadow.push(ColorVertex {
                position: [self.position.x, y, self.position.z],
                color: [0.13, 0.19, 0.10, 0.19],
            });
            for angle in [a, b] {
                shadow.push(ColorVertex {
                    position: [
                        self.position.x + 0.34 * libm::cosf(angle),
                        y,
                        self.position.z + 0.22 * libm::sinf(angle),
                    ],
                    color: [0.13, 0.19, 0.10, 0.0],
                });
            }
        }
        // The shadow is at the feet, so drawing it after the avatar remains
        // depth-correct while avoiding a second allocation for the mesh body.
        self.character.extend(shadow);
    }
    /// Head anchor follows the sampled skeleton (including sitting and waving).
    pub fn bubble_anchor(&self) -> Vec3 {
        let anchor = self
            .actor
            .names
            .iter()
            .position(|n| n == "chat.anchor")
            .unwrap();
        self.position + Quat::from_rotation_y(self.yaw) * self.globals[anchor].w_axis.truncate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assets_have_live_motion_and_face_layers() {
        let mut s = Island::new();
        for a in [
            Action::Idle,
            Action::Walk,
            Action::Run,
            Action::SitDown,
            Action::SitIdle,
            Action::StandUp,
            Action::Wave,
            Action::Cheer,
        ] {
            assert!(s.actor.clip(a.name()).is_some());
        }
        let idle = s.character.clone();
        s.change(Action::Walk);
        s.action_time = 0.20;
        s.blend = 1.;
        s.animate();
        let moved = idle
            .iter()
            .zip(&s.character)
            .filter(|(a, b)| {
                Vec3::from_array(a.position).distance(Vec3::from_array(b.position)) > 0.01
            })
            .count();
        assert!(
            moved > 100,
            "walk must deform the actual mesh, moved {moved}"
        );
        for e in EXPRESSIONS {
            assert!(s.actor.clip(&alloc::format!("Expression_{e}")).is_some());
        }
    }
    #[test]
    fn authored_wave_lifts_hand_and_sitting_lowers_hips() {
        let mut s = Island::new();
        let hand = s.actor.names.iter().position(|n| n == "hand.R").unwrap();
        let hips = s.actor.names.iter().position(|n| n == "hips").unwrap();
        let rest_hand = s.globals[hand].w_axis.y;
        let rest_hips = s.globals[hips].w_axis.y;
        s.change(Action::Wave);
        s.action_time = 0.7;
        s.blend = 1.0;
        s.animate();
        assert!(
            s.globals[hand].w_axis.y > rest_hand + 0.25,
            "wave must raise hand: {} vs {}",
            s.globals[hand].w_axis.y,
            rest_hand
        );
        s.change(Action::SitIdle);
        s.action_time = 1.0;
        s.blend = 1.0;
        s.animate();
        assert!(s.globals[hips].w_axis.y < rest_hips - 0.25);
    }
    #[test]
    fn movement_is_bounded_and_frame_independent() {
        let mut s = Island::new();
        let start = s.position;
        for _ in 0..30 {
            s.step(Input {
                x: 1.,
                run: true,
                ..Input::default()
            });
        }
        assert!((s.position.x - start.x - 2.65).abs() < 0.001);
        for _ in 0..600 {
            s.step(Input {
                x: 1.,
                z: 1.,
                run: true,
                ..Input::default()
            });
        }
        assert!(Island::walkable(s.position.x, s.position.z));
        assert!(!Island::walkable(f32::NAN, 0.));
        assert!(!Island::walkable(0., 20.));
        for &(x, z, _) in layout::COLLIDERS {
            assert!(!Island::walkable(x, z));
        }
    }
    #[test]
    fn sit_stand_and_expression_do_not_teleport_or_restart_walk() {
        let mut s = Island::new();
        s.step(Input {
            sit: true,
            ..Input::default()
        });
        for _ in 0..24 {
            s.step(Input::default());
        }
        assert_eq!(s.action, Action::SitIdle);
        s.step(Input {
            x: 1.,
            ..Input::default()
        });
        assert_eq!(s.action, Action::StandUp);
        for _ in 0..20 {
            s.step(Input {
                x: 1.,
                ..Input::default()
            });
        }
        assert_eq!(s.action, Action::Walk);
        let t = s.action_time;
        s.set_expression(3);
        s.step(Input {
            x: 1.,
            ..Input::default()
        });
        assert!(s.action_time > t);
    }
    #[test]
    fn bench_clip_and_exit_obey_scene_geometry() {
        let mut s = Island::new();
        s.position = Vec3::new(3.05, 0.11, 0.80);
        s.step(Input {
            sit: true,
            ..Input::default()
        });
        assert!(s.on_bench);
        assert_eq!(s.clip_name(), "BenchSitDown");
        for _ in 0..30 {
            s.step(Input::default());
        }
        assert_eq!(s.action, Action::SitIdle);
        s.step(Input {
            sit: true,
            ..Input::default()
        });
        for _ in 0..24 {
            s.step(Input::default());
        }
        assert!(!s.on_bench);
        assert!(Island::walkable(s.position.x, s.position.z));
        let feet = s
            .character
            .iter()
            .filter(|v| v.color[3] > 0.9)
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min);
        assert!(
            (0.09..0.15).contains(&feet),
            "idle feet should meet the ground: {feet}"
        );
    }
    #[test]
    fn walk_support_sole_meets_ground_across_the_cycle() {
        let mut s = Island::new();
        s.change(Action::Walk);
        s.blend = 1.0;
        for sample in 0..24 {
            s.action_time = 0.84 * sample as f32 / 24.0;
            s.animate();
            let sole = s
                .character
                .iter()
                .filter(|v| v.color[3] > 0.9)
                .map(|v| v.position[1])
                .fold(f32::INFINITY, f32::min);
            assert!(
                (sole - s.position.y).abs() < 0.035,
                "walk frame {sample}: sole {sole}, floor {}",
                s.position.y
            );
        }
    }
    #[test]
    fn chat_anchor_stays_above_head_through_actions() {
        let mut s = Island::new();
        for action in [Action::Idle, Action::Wave, Action::SitIdle, Action::Cheer] {
            s.change(action);
            s.action_time = 0.7;
            s.blend = 1.0;
            s.animate();
            let max = s
                .character
                .iter()
                .filter(|v| v.color[3] > 0.9)
                .map(|v| v.position[1])
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                s.bubble_anchor().y > max,
                "{action:?}: anchor must clear the avatar"
            );
        }
    }
    #[test]
    fn chat_unicode_bounds_dedup_delivery_and_expiry() {
        let mut c = Chat::new(1);
        assert_eq!(c.send(" \n ", 0, false), Err(MessageError::Empty));
        c.send("你好，小岛 🌴", 0, false).unwrap();
        assert_eq!(c.history[0].body, "你好，小岛 🌴");
        assert_eq!(c.history[0].delivery, Delivery::Local);
        assert_eq!(c.send("spam", 1, false), Err(MessageError::RateLimited));
        assert_eq!(
            c.send(&"海".repeat(65), 30, false),
            Err(MessageError::TooLong)
        );
        assert_eq!(
            c.send("bad\u{202e}text", 30, false),
            Err(MessageError::Control)
        );
        assert!(c.bubble(1, 209).is_some());
        assert!(c.bubble(1, 210).is_none());
        assert_eq!(c.receive(2, 1, "hello", 0), Err(MessageError::UnknownPeer));
        c.add_peer(2);
        c.receive(2, 1, "hello", 30).unwrap();
        assert_eq!(c.receive(2, 1, "retry", 31), Err(MessageError::Duplicate));
        let id = c.send("online", 30, true).unwrap();
        assert!(c.acknowledge(id, false));
        assert!(!c.acknowledge(id, true));
        for i in 3..100 {
            c.receive(2, i, "bounded", i * 30).unwrap();
        }
        assert_eq!(c.history.len(), HISTORY_LIMIT);
    }
    #[test]
    fn malformed_asset_is_rejected() {
        let src = include_bytes!("../assets/mira.p3m");
        for size in [0, 3, 19, 80, src.len() - 1] {
            assert!(MeshAsset::decode(&src[..size]).is_err());
        }
        let mut bad = src.to_vec();
        bad[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(MeshAsset::decode(&bad).is_err());
    }
}
