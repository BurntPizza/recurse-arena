
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate ludomath;

use std::time::Duration;
use std::collections::HashMap;

use ludomath::vec2d::*;

pub const LOGO: &[&str] = &["bbbbbbbbbbbb",
                            "bwwwwwwwwwwb",
                            "bwffffffffwb",
                            "bwgfgfgfffwb",
                            "bwffffffffwb",
                            "bwfggfggffwb",
                            "bwffffffffwb",
                            "bwffffffffwb",
                            "bwwwwwwwwwwb",
                            "bbbbffffbbbb",
                            "wwwibffbiwww",
                            "ibbbffffbbbi",
                            "bfbwfwfwbwfb",
                            "bfwfwbwfwffb",
                            "bbbbbbbbbbbb"];

pub const LOGO_WIDTH: usize = 12;
pub const LOGO_HEIGHT: usize = 15;

pub const PLAYER_RADIUS: f32 = 0.2;
pub const BULLET_RADIUS: f32 = 0.05;

#[derive(Copy, Clone)]
pub struct CSquare {
    pub top_left: Vector,
    pub bottom_right: Vector,
}

impl CSquare {
    pub fn new(top_left: Vector, size: f32) -> Self {
        CSquare {
            top_left,
            bottom_right: top_left + Vector::new(size, size),
        }
    }

    pub fn new_centered(center: Vector, apothem: f32) -> Self {
        let offset = Vector::new(apothem, apothem);
        CSquare {
            top_left: center - offset,
            bottom_right: center + offset,
        }
    }

    pub fn intersects(&self, other: CSquare) -> bool {
        self.top_left.x < other.bottom_right.x && self.bottom_right.x > other.top_left.x &&
        self.top_left.y < other.bottom_right.y && self.bottom_right.y > other.top_left.y
    }

    pub fn contains(&self, p: Vector) -> bool {
        self.top_left.x < p.x && self.bottom_right.x > p.x && self.top_left.y < p.y &&
        self.bottom_right.y > p.y
    }
}

#[derive(Copy, Clone)]
pub struct CCircle {
    center: Vector,
    radius: f32,
}

impl CCircle {
    pub fn new(center: Vector, radius: f32) -> Self {
        CCircle { center, radius }
    }

    pub fn intersects(self, other: Self) -> bool {
        self.center.dist(other.center) < self.radius + other.radius
    }

    pub fn intersects_square(self, s: &CSquare) -> bool {
        let s_center = s.top_left + (s.bottom_right - s.top_left) * 0.5;
        let v = (self.center - s_center).normalize();
        let outer_point = self.center + self.radius * v;
        s.contains(outer_point)
    }
}

pub fn collision_boxes() -> Vec<CSquare> {
    assert!(LOGO.iter().all(|r| r.len() == LOGO_WIDTH));
    assert_eq!(LOGO.len(), LOGO_HEIGHT);

    let mut boxes = vec![];

    for y in 0..LOGO_HEIGHT {
        for x in 0..LOGO_WIDTH {
            let row = &LOGO[y];
            let pixel = row.bytes().nth(x).unwrap();

            match pixel {
                b'b' | b'g' | b'i' => {
                    let pos = Vector::new(x as f32, y as f32);
                    boxes.push(CSquare::new(pos, 1.0));
                }
                b'w' | b'f' => {}
                _ => unreachable!(),
            }
        }
    }

    boxes
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameState {
    pub players: HashMap<PlayerId, Player>,
    pub bullets: Vec<Bullet>,
    pub events: Vec<Event>,
}

impl GameState {
    pub fn update(&mut self, collision_boxes: &[CSquare], dt: f32) {
        self.events.clear();

        for p in self.players.values_mut() {
            p.update(collision_boxes, dt);
        }

        for i in (0..self.bullets.len()).rev() {
            if self.bullets[i].update(collision_boxes, dt) {
                self.events
                    .push(Event::BulletHitWall(self.bullets[i].pos, self.bullets[i].vel));
                self.bullets.remove(i);
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    // pos, vel
    BulletHitWall(#[serde(with = "VectorDef")]
                  Vector,
                  #[serde(with = "VectorDef")]
                  Vector),
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Bullet {
    #[serde(with = "VectorDef")]
    pub pos: Vector,
    #[serde(with = "VectorDef")]
    pub vel: Vector,
    pub pid: PlayerId,
}

impl Bullet {
    pub fn spawn(player: &Player) -> Self {
        Bullet {
            pid: player.id,
            pos: player.pos + player.dir * PLAYER_RADIUS,
            vel: player.dir * 10.0,
        }
    }

    // returns if dead
    pub fn update(&mut self, collision_boxes: &[CSquare], dt: f32) -> bool {
        let bounds = CCircle::new(self.pos, BULLET_RADIUS);
        for cb in collision_boxes {
            if bounds.intersects_square(cb) {
                return true;
            }
        }

        self.pos += self.vel * dt;
        self.pos.magnitude() > 20.0
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    #[serde(with = "VectorDef")]
    pub dir: Vector,
    #[serde(with = "VectorDef")]
    pub pos: Vector,
    #[serde(with = "VectorDef")]
    pub vel: Vector,
    #[serde(with = "VectorDef")]
    pub force: Vector,
}

impl Player {
    pub fn update(&mut self, collision_boxes: &[CSquare], dt: f32) {
        let p_bounds = CSquare::new_centered(self.pos, PLAYER_RADIUS * 1.1);

        for cb in collision_boxes {
            if cb.intersects(p_bounds) {
                let wc = cb.top_left + Vector::new(0.5, 0.5);
                self.force += (self.pos - wc) * 100.0;
            }
        }

        self.vel += self.force * dt;

        let vm = self.vel.magnitude();

        // Critical that vm doesn't go denormal/too small!
        // I found that out the hard way after much hardship and long suffering
        if vm > 0.0000001 {
            self.vel = self.vel.normalize() * vm.min(40.0) * 0.9;
        } else {
            self.vel = Vector::default();
        }

        self.pos += self.vel * dt;
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum FromServerMsg {
    // player id
    Welcome(PlayerId),
    Update(GameState),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ToServerMsg {
    Login(PlayerId, String),
    Input(PlayerId, Input),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Input {
    Press(Button,
          #[serde(with = "VectorDef")]
          Vector),
    Release(Button),
    
    DirChanged(#[serde(with = "VectorDef")] Vector),
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Button {
    W,
    A,
    S,
    D,
    LeftMouse,
}

#[derive(Default, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, Copy, Clone, Serialize, Deserialize)]
pub struct PlayerId(pub u32);

pub trait IntoSecs {
    fn into_secs(&self) -> f32;
}

impl IntoSecs for Duration {
    fn into_secs(&self) -> f32 {
        let secs = self.as_secs() as f32;
        let nanos = self.subsec_nanos() as f32;
        secs + nanos / 1_000_000_000.0
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "Vector")]
#[allow(dead_code)]
struct VectorDef {
    x: f32,
    y: f32,
}
