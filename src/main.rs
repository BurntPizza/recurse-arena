
extern crate piston;
extern crate graphics;
extern crate glutin_window;
extern crate glutin;
extern crate opengl_graphics;
extern crate find_folder;
extern crate ludomath;
extern crate ezing as ez;

use std::collections::{HashMap, HashSet};
use std::time;
use std::mem;
use std::cmp::Ordering;
use std::net::TcpStream;

use piston::window::*;
use piston::event_loop::*;
use piston::input::*;

use glutin_window::GlutinWindow as Window;
use opengl_graphics::{GlGraphics, OpenGL};
use opengl_graphics::glyph_cache::GlyphCache;
use graphics::*;
use graphics::types::{Color, Matrix2d};

use ludomath::vec2d::*;
use ludomath::rng::Rng;

const ADDR: &str = "localhost:8000";

const LOGO: &[&str] = &["bbbbbbbbbbbb",
                        "bwwwwwwwwwwb",
                        "bwffffffffwb",
                        "bwgfgfgfffwb",
                        "bwffffffffwb",
                        "bwfggfggffwb",
                        "bwffffffffwb",
                        "bwffffffffwb",
                        "bwwwwwwwwwwb",
                        "bbbbffffbbbb",
                        "wwwwbffbwwww",
                        "wbbbffffbbbw",
                        "bfbwfwfwbwfb",
                        "bfwfwbwfwffb",
                        "bbbbbbbbbbbb"];

const LOGO_WIDTH: usize = 12;
const LOGO_HEIGHT: usize = 15;

const BLACK: Color = [0.0, 0.0, 0.0, 1.0];
const GREEN: Color = [0.0, 0.9, 0.0, 1.0];
const WHITE: Color = [1.0; 4];
const RED: Color = [1.0, 0.0, 0.0, 1.0];

const TAU: f64 = 6.2831853;

const PLAYER_RADIUS: f32 = 0.2;

static FONT: &[u8] = include_bytes!("../assets/FiraSans-Regular.ttf");

fn main() {
    assert!(LOGO.iter().all(|r| r.len() == LOGO_WIDTH));
    assert_eq!(LOGO.len(), LOGO_HEIGHT);

    let (full_width, full_height) = glutin::get_primary_monitor().get_dimensions();

    let opengl = OpenGL::V3_2;

    let mut window: Window = WindowSettings::new("Recurse Arena", [full_width, full_height])
        .opengl(opengl)
        .exit_on_esc(true)
        .build()
        .unwrap();

    let mut cache = GlyphCache::from_bytes(FONT).unwrap();
    let mut gl = GlGraphics::new(opengl);
    let mut events = Events::new(EventSettings::new());

    let player_name = into_secs(time::SystemTime::now()
                                    .duration_since(time::UNIX_EPOCH)
                                    .expect("Sytem time error")) as u64;

    let player_name = player_name.to_string();

    // let connection = connect();

    let mut state = State::new(player_name.clone());

    let player_id = state.new_entitiy_id();
    let player = Player {
        name: player_name,
        pos: Vector::new(1.5, 1.5),
        dir: Vector::default(),
    };

    state.entities.push(Entity::Player(player_id, player));

    for y in 0..LOGO_HEIGHT {
        for x in 0..LOGO_WIDTH {
            let wall = Wall {
                pos: (x, y),
                pixel: logo(x, y),
            };
            let id = state.new_entitiy_id();
            state.entities.push(Entity::Wall(id, wall));
        }
    }

    let begin_time = time::Instant::now();

    while let Some(e) = events.next(&mut window) {
        match e {
            Input::Render(a) => {
                gl.draw(a.viewport(), |c, g| {
                    clear(WHITE, g);

                    let bias = if cfg!(debug_assertions) { 1.0 } else { 0.0 };
                    let elapsed = (into_secs(begin_time.elapsed()) + bias).min(1.0) as f32;

                    let original = c.transform;
                    let centered = original
                        .trans(a.width as f64 / 2.0, a.height as f64 / 2.0)
                        .zoom(ez::expo_in(elapsed) as f64 * 300.0);

                    let (px, py) = as_f64s(state.player_pos);

                    let tracking = centered.trans(-px, -py);

                    let transforms = Transforms {
                        original,
                        centered,
                        tracking,
                    };

                    let mut ctx = RenderContext {
                        transforms: &transforms,
                        g,
                        cache: &mut cache,
                    };

                    state.draw(&mut ctx);
                });
            }
            Input::Update(_) => {
                // every tick

                if let Some(wh) = window.window.get_inner_size_pixels() {
                    state.window_size = wh;
                } else {
                    println!("window inner size error?");
                }

                state.update();

                window.set_title(format!("{:?}", state.player_pos));
            }
            Input::Press(button) => {
                if !state.buttons_down.contains_key(&button) {
                    state.buttons_down.insert(button, time::Instant::now());

                    if button == Button::Mouse(MouseButton::Left) {
                        let b = Bullet {
                            pos: state.player_pos + state.player_dir * PLAYER_RADIUS,
                            vel: state.player_dir * 0.15,
                        };
                        let id = state.new_entitiy_id();
                        state.entities.push(Entity::Bullet(id, b));
                    }
                }
            }
            Input::Release(button) => {
                state.buttons_down.remove(&button);
            }
            Input::Move(Motion::MouseCursor(x, y)) => {
                state.mouse_screen = Vector::new(x as f32, y as f32);
            }
            _ => {}
        }
    }
}

fn connect() -> TcpStream {
    TcpStream::connect(ADDR).unwrap_or_else(|e| panic!("Failed to connect: {}", e))
}

type CollisionMap = HashMap<u32, Vec<Collision>>;

struct State {
    player_name: String,
    window_size: (u32, u32),
    mouse_screen: Vector,
    buttons_down: HashMap<Button, time::Instant>,
    entities: Vec<Entity>,
    player_pos: Vector,
    player_dir: Vector,
    last_tick: time::Instant,
    entity_id_counter: u32,
    collisions: CollisionMap,
}

impl State {
    fn new(player_name: String) -> Self {
        State {
            player_name,
            window_size: (0, 0),
            mouse_screen: Vector::default(),
            buttons_down: HashMap::new(),
            entities: vec![],
            player_pos: Vector::default(),
            player_dir: Vector::default(),
            last_tick: time::Instant::now(),
            entity_id_counter: 0,
            collisions: CollisionMap::new(),
        }
    }

    fn new_entitiy_id(&mut self) -> u32 {
        let id = self.entity_id_counter;
        self.entity_id_counter += 1;
        id
    }

    fn draw(&mut self, ctx: &mut RenderContext) {
        self.entities
            .sort_by(|a, b| match (a, b) {
                         (&Entity::Wall(..), _) => Ordering::Less,
                         (_, &Entity::Wall(..)) => Ordering::Greater,
                         (&Entity::Player(..), _) => Ordering::Greater,
                         (_, &Entity::Player(..)) => Ordering::Less,
                         _ => Ordering::Equal,
                     });

        for e in &self.entities {
            e.draw(self, ctx);
        }

        if cfg!(debug_assertions) {
            let mut rng = ludomath::rng::Rng::new_seeded(234356, 2635656);

            for e in &self.entities {
                if let Some(ref cs) = self.collisions.get(&e.id()) {
                    for c in *cs {
                        let color = [rng.rand_float(0.0, 1.0),
                                     rng.rand_float(0.0, 1.0),
                                     rng.rand_float(0.0, 1.0),
                                     1.0];

                        match *e {
                            Entity::Player(_, Player { pos, .. }) => {
                                let Collision::PlayerWall(d) = *c;
                                let l = as_line(pos, pos + d);
                                line(color, 0.02, l, ctx.transforms.tracking, ctx.g);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    fn update(&mut self) {
        let dt = into_secs(self.last_tick.elapsed()) as f32;
        self.last_tick = time::Instant::now();

        let entities = mem::replace(&mut self.entities, vec![]);
        self.collisions.clear();

        for i in 0..entities.len() {
            for j in (i + 1)..entities.len() {
                let e = &entities[i];
                let other = &entities[j];

                if let Some((c, a1, a2)) = e.get_collision(other) {
                    if a1 {
                        self.collisions.entry(e.id()).or_insert(vec![]).push(c);
                    }
                    if a2 {
                        self.collisions.entry(other.id()).or_insert(vec![]).push(c);
                    }
                }
            }
        }

        let collisions = mem::replace(&mut self.collisions, CollisionMap::new());

        for mut e in entities {
            e.update(self, &collisions, dt);
            if !e.is_dead() {
                self.entities.push(e);
            }
        }

        mem::replace(&mut self.collisions, collisions);
    }
}

#[derive(PartialEq)]
enum Entity {
    Bullet(u32, Bullet),
    Player(u32, Player),
    Wall(u32, Wall),
}

impl Entity {
    fn draw(&self, state: &State, ctx: &mut RenderContext) {
        let RenderContext {
            transforms,
            ref mut g,
            ..
        } = *ctx;

        match *self {
            Entity::Bullet(id, Bullet { pos, vel, .. }) => {
                let vo = pos + vel.normalize() * 0.1;

                line(RED, 0.005, as_line(pos, vo), transforms.tracking, *g);
            }
            Entity::Player(id, Player { pos, dir, ref name }) => {
                let (px, py) = as_f64s(pos);
                let player_box = rectangle::centered_square(px, py, PLAYER_RADIUS as f64);
                let end_point = pos + dir * PLAYER_RADIUS;

                circle_arc(RED, 0.03, 0.0, TAU, player_box, transforms.tracking, *g);
                line(RED, 0.01, as_line(pos, end_point), transforms.tracking, *g);
                text(RED,
                     20,
                     &format!("{}", name),
                     ctx.cache,
                     transforms
                         .centered
                         .trans(-PLAYER_RADIUS as f64, -PLAYER_RADIUS as f64 * 1.3)
                         .zoom(1.0 / 300.0),
                     *g);
            }
            Entity::Wall(id, Wall { pos: (x, y), pixel }) => {
                let color = pixel.color();
                let tile = rectangle::square(x as f64, y as f64, 1.0);
                rectangle(color, tile, transforms.tracking, *g);
            }
        }
    }
    fn update(&mut self, state: &mut State, collisions: &CollisionMap, dt: f32) {
        match *self {
            Entity::Bullet(id, ref mut b) => {
                b.pos += b.vel;
            }
            Entity::Player(id, ref mut player) => {
                if player.name == state.player_name {
                    update_player(player, state, dt);
                }

                if let Some(collisions) = collisions.get(&id) {
                    for c in collisions {
                        match *c {
                            Collision::PlayerWall(displacement) => {
                                player.pos -= displacement;
                            }
                        }
                    }
                }
            }
            Entity::Wall { .. } => {}
        }
    }
    fn is_dead(&self) -> bool {
        match *self {
            Entity::Bullet(id, ref b) => b.pos.magnitude() > 20.0,
            Entity::Wall { .. } => false,
            Entity::Player { .. } => false, //TODO
        }
    }

    fn id(&self) -> u32 {
        match *self {
            Entity::Bullet(id, ..) => id,
            Entity::Player(id, ..) => id,
            Entity::Wall(id, ..) => id,
        }
    }

    fn get_collision(&self, other: &Entity) -> Option<(Collision, bool, bool)> {
        fn player_wall(wx: f32,
                       wy: f32,
                       pos: Vector,
                       a1: bool,
                       a2: bool)
                       -> Option<(Collision, bool, bool)> {

            // let w_bounds = CSquare::new(Vector::new(wx, wy), 1.0);
            let w_center = Vector::new(wx + 0.5, wy + 0.5);
            let w_bounds = CSquare::new_centered(w_center, 0.5);
            let p_bounds = CCircle::new(pos, PLAYER_RADIUS);

            // points from player center to wall center
            let v = (p_bounds.center - w_center).normalize();
            let outer_point = p_bounds.center + p_bounds.radius * v;

            if w_bounds.contains(outer_point) {
                let w_surface_point = (p_bounds.center)
                    .clamp_to_rect(w_bounds.top_left, w_bounds.bottom_right);

                Some((Collision::PlayerWall(w_surface_point - outer_point), a1, a2))
            } else {
                None
            }
        }

        match (self, other) {
            (&Entity::Player(id, Player { pos, .. }),
             &Entity::Wall(wid,
                           Wall {
                               pos: (wx, wy),
                               pixel,
                           })) => {
                if pixel != Pixel::White {
                    player_wall(wx as f32, wy as f32, pos, true, false)
                } else {
                    None
                }
            }
            (&Entity::Wall(wid,
                           Wall {
                               pos: (wx, wy),
                               pixel,
                           }),
             &Entity::Player(id, Player { pos, .. })) => {
                if pixel != Pixel::White {
                    player_wall(wx as f32, wy as f32, pos, false, true)
                } else {
                    None
                }
            }
            (&Entity::Wall(..), &Entity::Wall(..)) => None,
            _ => unimplemented!(),
        }
    }
}

#[derive(Copy, Clone)]
enum Collision {
    PlayerWall(Vector),
}

struct CSquare {
    top_left: Vector,
    bottom_right: Vector,
}

impl CSquare {
    fn new(top_left: Vector, size: f32) -> Self {
        CSquare {
            top_left,
            bottom_right: top_left + Vector::new(size, size),
        }
    }
    fn new_centered(center: Vector, apothem: f32) -> Self {
        let offset = Vector::new(apothem, apothem);
        CSquare {
            top_left: center - offset,
            bottom_right: center + offset,
        }
    }

    // fn intersection(&self, other: &CSquare) -> Option<CSquare> {
    //     if self.top_left.x < other.bottom_right.x && self.bottom_right.x > other.top_left.x &&
    //        self.top_left.y < other.bottom_right.y &&
    //        self.bottom_right.y > other.top_left.y {
    //         let top_left = Vector::new(other.top_left.x, self.top_left.y);
    //         let size = self.bottom_right - other.top_left;
    //         Some(CSquare::new(top_left, size))
    //     } else {
    //         None
    //     }
    // }

    fn contains(&self, p: Vector) -> bool {
        self.top_left.x < p.x && self.bottom_right.x > p.x && self.top_left.y < p.y &&
        self.bottom_right.y > p.y
    }
}

struct CCircle {
    center: Vector,
    radius: f32,
}

impl CCircle {
    fn new(center: Vector, radius: f32) -> Self {
        CCircle { center, radius }
    }

    fn intersects(self, other: Self) -> bool {
        self.center.dist(other.center) < self.radius + other.radius
    }
}

fn circle_intersects_square(c: CCircle, s: CSquare) -> bool {
    let s_center = s.top_left + (s.bottom_right - s.top_left) * 0.5;
    let v = (c.center - s_center).normalize();
    let outer_point = c.center + c.radius * v;
    s.contains(outer_point)
}

#[derive(PartialEq)]
struct Player {
    name: String,
    pos: Vector,
    dir: Vector,
}
#[derive(PartialEq)]
struct Wall {
    pos: (usize, usize),
    pixel: Pixel,
}
#[derive(PartialEq)]
struct Bullet {
    pos: Vector,
    vel: Vector,
}

struct Transforms {
    original: Matrix2d,
    centered: Matrix2d,
    tracking: Matrix2d,
}

#[derive(Copy, Clone, PartialEq)]
enum Pixel {
    Black,
    White,
    Green,
    Grey,
}

impl Pixel {
    fn color(&self) -> Color {
        match *self {
            Pixel::Black => BLACK,
            Pixel::White => WHITE,
            Pixel::Green => GREEN,
            Pixel::Grey => color::grey(0.1),
        }
    }
}

struct RenderContext<'a, 'b: 'a> {
    transforms: &'a Transforms,
    g: &'a mut GlGraphics,
    cache: &'a mut GlyphCache<'b>,
}

fn logo(x: usize, y: usize) -> Pixel {
    assert!(x < LOGO_WIDTH);
    assert!(y < LOGO_HEIGHT);

    let row = &LOGO[y];
    let pixel = row.bytes().nth(x).unwrap();

    match pixel {
        b'b' => Pixel::Black,
        b'w' => Pixel::White,
        b'g' => Pixel::Green,
        b'f' => Pixel::Grey,
        _ => unreachable!(),
    }
}

fn into_secs(d: time::Duration) -> f64 {
    let secs = d.as_secs() as f64;
    let nanos = d.subsec_nanos() as f64;
    secs + nanos / 1_000_000_000.0
}

fn f(x: f32) -> f64 {
    x as f64
}

fn as_f64s(v: Vector) -> (f64, f64) {
    (f(v.x), f(v.y))
}

fn as_line(v1: Vector, v2: Vector) -> [f64; 4] {
    [f(v1.x), f(v1.y), f(v2.x), f(v2.y)]
}


fn update_player(p: &mut Player, state: &mut State, dt: f32) {
    let Player {
        ref name,
        ref mut pos,
        ref mut dir,
    } = *p;

    let move_amount = |start: time::Instant| {
        let t = into_secs(start.elapsed()) as f32;
        let d = dt * 2.0;
        (d / (t + 1.5) + d)
    };

    if let Some(&t) = state.buttons_down.get(&Button::Keyboard(Key::A)) {
        pos.x -= move_amount(t);
    }
    if let Some(&t) = state.buttons_down.get(&Button::Keyboard(Key::D)) {
        pos.x += move_amount(t);
    }
    if let Some(&t) = state.buttons_down.get(&Button::Keyboard(Key::W)) {
        pos.y -= move_amount(t);
    }
    if let Some(&t) = state.buttons_down.get(&Button::Keyboard(Key::S)) {
        pos.y += move_amount(t);
    }

    let (width, height) = state.window_size;
    let center = Vector::new(width as f32, height as f32) / 2.0;
    let mouse = state.mouse_screen - center;
    *dir = (mouse - *pos).normalize();

    state.player_pos = *pos;
    state.player_dir = *dir;
}
