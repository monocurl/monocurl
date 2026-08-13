//! typed, retained Rust scene recipes.

extern crate self as rust_scene;

use std::{
    fmt,
    sync::{Arc, Mutex},
};

pub use rust_scene_macros::live;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}
impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn lerp(self, b: Self, t: f64) -> Self {
        Self::new(self.x + (b.x - self.x) * t, self.y + (b.y - self.y) * t)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}
impl Color {
    pub const BLUE: Self = Self {
        r: 45,
        g: 115,
        b: 245,
    };
    pub const ORANGE: Self = Self {
        r: 245,
        g: 135,
        b: 45,
    };
    pub fn lerp(self, b: Self, t: f64) -> Self {
        let c = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
        Self {
            r: c(self.r, b.r),
            g: c(self.g, b.g),
            b: c(self.b, b.b),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum MeshValue {
    Circle {
        center: Vec2,
        radius: f64,
        color: Color,
    },
    Square {
        center: Vec2,
        side: f64,
        color: Color,
    },
    Group(Vec<MeshValue>),
}
impl MeshValue {
    pub fn circle(center: Vec2, radius: f64, color: Color) -> Self {
        Self::Circle {
            center,
            radius,
            color,
        }
    }
    pub fn square(center: Vec2, side: f64, color: Color) -> Self {
        Self::Square {
            center,
            side,
            color,
        }
    }
    pub fn group(children: impl IntoIterator<Item = MeshValue>) -> Self {
        Self::Group(children.into_iter().collect())
    }
    pub fn translate(self, delta: Vec2) -> Self {
        match self {
            Self::Circle {
                center,
                radius,
                color,
            } => Self::circle(
                Vec2::new(center.x + delta.x, center.y + delta.y),
                radius,
                color,
            ),
            Self::Square {
                center,
                side,
                color,
            } => Self::square(
                Vec2::new(center.x + delta.x, center.y + delta.y),
                side,
                color,
            ),
            Self::Group(children) => Self::Group(
                children
                    .into_iter()
                    .map(|child| child.translate(delta))
                    .collect(),
            ),
        }
    }
    pub fn lerp(&self, b: &Self, t: f64) -> Result<Self> {
        match (self, b) {
            (
                Self::Circle {
                    center: a,
                    radius: ar,
                    color: ac,
                },
                Self::Circle {
                    center: b,
                    radius: br,
                    color: bc,
                },
            ) => Ok(Self::circle(
                a.lerp(*b, t),
                ar + (br - ar) * t,
                ac.lerp(*bc, t),
            )),
            (
                Self::Square {
                    center: a,
                    side: ar,
                    color: ac,
                },
                Self::Square {
                    center: b,
                    side: br,
                    color: bc,
                },
            ) => Ok(Self::square(
                a.lerp(*b, t),
                ar + (br - ar) * t,
                ac.lerp(*bc, t),
            )),
            (Self::Group(a), Self::Group(b)) if a.len() == b.len() => a
                .iter()
                .zip(b)
                .map(|(a, b)| a.lerp(b, t))
                .collect::<Result<Vec<_>>>()
                .map(Self::Group),
            _ => Err(Error::Message("incompatible mesh shapes")),
        }
    }
}
#[derive(Clone, Debug)]
pub struct OperatorEndpoints {
    pub identity: MeshValue,
    pub modified: MeshValue,
}
impl OperatorEndpoints {
    pub fn new(identity: MeshValue, modified: MeshValue) -> Self {
        Self { identity, modified }
    }
}
#[derive(Clone, Debug)]
pub enum Error {
    Message(&'static str),
}
pub type Result<T> = std::result::Result<T, Error>;
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for Error {}

pub trait Recipe: Clone + Send + 'static {
    fn evaluate(&self) -> Result<MeshValue>;
    fn interpolate_from(&self, source: &MeshValue, time: f64) -> Result<MeshValue> {
        source.lerp(&self.evaluate()?, time)
    }
}
#[derive(Clone)]
pub struct Mesh<R: Recipe> {
    state: Arc<Mutex<State<R>>>,
}
#[derive(Clone)]
struct State<R: Recipe> {
    leader: R,
    follower: MeshValue,
}
pub fn mesh<R: Recipe>(leader: R) -> Result<Mesh<R>> {
    let follower = leader.evaluate()?;
    Ok(Mesh {
        state: Arc::new(Mutex::new(State { leader, follower })),
    })
}
pub fn mesh_with_follower<R: Recipe>(leader: R, follower: MeshValue) -> Mesh<R> {
    Mesh {
        state: Arc::new(Mutex::new(State { leader, follower })),
    }
}
impl<R: Recipe> Mesh<R> {
    pub fn set(&self, leader: R) {
        self.state.lock().unwrap().leader = leader;
    }
    pub fn follower(&self) -> MeshValue {
        self.state.lock().unwrap().follower.clone()
    }
    pub fn sync(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.follower = state.leader.evaluate()?;
        Ok(())
    }
    pub fn lerp(&self, time: f64) -> Result<MeshValue> {
        let mut state = self.state.lock().unwrap();
        let frame = state
            .leader
            .interpolate_from(&state.follower, time.clamp(0.0, 1.0))?;
        state.follower = frame.clone();
        Ok(frame)
    }
    pub fn attribute<T: Clone>(&self, get: fn(&R) -> T, set: fn(&mut R, T)) -> Attribute<R, T> {
        Attribute {
            mesh: self.clone(),
            get,
            set,
        }
    }
    pub fn nested<C: Recipe>(&self, get: fn(&R) -> &C, set: fn(&mut R) -> &mut C) -> Nested<R, C> {
        Nested {
            mesh: self.clone(),
            get,
            set,
        }
    }
}
pub struct Attribute<R: Recipe, T: Clone> {
    mesh: Mesh<R>,
    get: fn(&R) -> T,
    set: fn(&mut R, T),
}
impl<R: Recipe, T: Clone> Attribute<R, T> {
    pub fn get(&self) -> T {
        (self.get)(&self.mesh.state.lock().unwrap().leader)
    }
    pub fn set(&self, value: T) {
        (self.set)(&mut self.mesh.state.lock().unwrap().leader, value)
    }
}
pub struct Nested<P: Recipe, C: Recipe> {
    mesh: Mesh<P>,
    get: fn(&P) -> &C,
    set: fn(&mut P) -> &mut C,
}
impl<P: Recipe, C: Recipe> Nested<P, C> {
    pub fn attribute<T: Clone>(
        &self,
        get: fn(&C) -> T,
        set: fn(&mut C, T),
    ) -> NestedAttribute<P, C, T> {
        NestedAttribute {
            mesh: self.mesh.clone(),
            child: self.get,
            child_mut: self.set,
            get,
            set,
        }
    }
}
pub struct NestedAttribute<P: Recipe, C: Recipe, T: Clone> {
    mesh: Mesh<P>,
    child: fn(&P) -> &C,
    child_mut: fn(&mut P) -> &mut C,
    get: fn(&C) -> T,
    set: fn(&mut C, T),
}
impl<P: Recipe, C: Recipe, T: Clone> NestedAttribute<P, C, T> {
    pub fn set(&self, value: T) {
        let mut state = self.mesh.state.lock().unwrap();
        (self.set)((self.child_mut)(&mut state.leader), value)
    }
    pub fn get(&self) -> T {
        let state = self.mesh.state.lock().unwrap();
        (self.get)((self.child)(&state.leader))
    }
}

/// a deliberately small timeline recorder for the static experiment.
///
/// Scene code still mutates typed leaders directly; the recorder only captures
/// frames, keeping the scheduling API independent from recipe typing.
#[derive(Clone, Debug)]
pub struct Scene {
    pub slides: Vec<Slide>,
}

impl Scene {
    pub fn new() -> Self {
        Self { slides: Vec::new() }
    }

    pub fn slide(
        &mut self,
        title: impl Into<String>,
        build: impl FnOnce(&mut Slide) -> Result<()>,
    ) -> Result<()> {
        let mut slide = Slide {
            title: title.into(),
            frames: Vec::new(),
        };
        build(&mut slide)?;
        self.slides.push(slide);
        Ok(())
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct Slide {
    pub title: String,
    pub frames: Vec<SceneFrame>,
}

impl Slide {
    pub fn set<R: Recipe>(&mut self, mesh: &Mesh<R>) -> Result<()> {
        mesh.sync()?;
        self.frames.push(SceneFrame {
            time: 0.0,
            meshes: vec![mesh.follower()],
        });
        Ok(())
    }

    pub fn lerp<R: Recipe>(&mut self, mesh: &Mesh<R>, duration: f64, samples: usize) -> Result<()> {
        let samples = samples.max(1);
        for sample in 1..=samples {
            let time = sample as f64 / samples as f64;
            self.frames.push(SceneFrame {
                time: duration * time,
                meshes: vec![mesh.lerp(time)?],
            });
        }
        Ok(())
    }

    pub fn wait(&mut self, duration: f64, mesh: MeshValue) {
        self.frames.push(SceneFrame {
            time: duration,
            meshes: vec![mesh],
        });
    }
}

#[derive(Clone, Debug)]
pub struct SceneFrame {
    pub time: f64,
    pub meshes: Vec<MeshValue>,
}
