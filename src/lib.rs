
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate ludomath;

use std::collections::HashMap;

use ludomath::vec2d::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameState {
    pub players: HashMap<PlayerId, Player>,
    pub bullets: Vec<Bullet>,
}

impl GameState {
    pub fn update(&mut self) {
        
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Bullet {
    #[serde(with = "VectorDef")]
    pub pos: Vector,
    #[serde(with = "VectorDef")]
    pub vel: Vector,
    pub pid: u32,
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

#[derive(Serialize, Deserialize)]
#[serde(remote = "Vector")]
#[allow(dead_code)]
struct VectorDef {
    x: f32,
    y: f32,
}
