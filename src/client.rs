
extern crate piston;
extern crate graphics;
extern crate glutin_window;
extern crate glutin;
extern crate opengl_graphics;
extern crate find_folder;
extern crate ludomath;
extern crate ezing as ez;
extern crate bincode as bc;
extern crate recurse_arena as ra;

use ra::GameState;

use std::collections::HashMap;
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
                        "bbbbbffbbbbb",
                        "wwwwbffbwwww",
                        "wbbbbffbbbbw",
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
const BULLET_RADIUS: f32 = 0.02;
const SPARK_RADIUS: f32 = 0.01;

static FONT: &[u8] = include_bytes!("../assets/FiraSans-Regular.ttf");

fn main() {
    assert!(LOGO.iter().all(|r| r.len() == LOGO_WIDTH));
    assert_eq!(LOGO.len(), LOGO_HEIGHT);

    let mut stream = connect();

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

    let menu = Menu {
        buttons_down: HashMap::new(),
        mouse_screen: Vector::default(),
    };

    let mut stage = Stage::Menu(menu);

    while let Some(e) = events.next(&mut window) {
        stage = step(e, stage, &mut gl, &mut cache, &mut window, &mut stream);
    }
}

fn step(e: Input,
        stage: Stage,
        gl: &mut GlGraphics,
        mut cache: &mut GlyphCache,
        window: &mut Window,
        mut stream: &mut TcpStream)
        -> Stage {
    match stage {
        Stage::Menu(mut menu) => {
            // menu screen game loop
            match e {
                Input::Render(a) => {
                    gl.draw(a.viewport(), |c, g| { clear(WHITE, g); });
                }
                Input::Update(_) => {
                    if true ||
                       menu.buttons_down
                           .contains_key(&Button::Keyboard(Key::Space)) {
                        // begin game

                        let player_name = if cfg!(debug_assertions) {
                                "Josh"
                            } else {
                                unimplemented!()
                            }
                            .to_owned();


                        let player_id = match bc::deserialize_from(&mut stream, bc::Infinite)
                                  .unwrap() {
                            ra::FromServerMsg::Welcome(id) => id,
                            _ => unreachable!(),
                        };

                        let msg = ra::ToServerMsg::Login(player_id, player_name.clone());
                        bc::serialize_into(&mut stream, &msg, bc::Infinite).unwrap();

                        let mut state = State {
                            game_state: GameState {
                                players: vec![],
                                bullets: vec![],
                            },
                            player_id: player_id.0,
                            player_name: player_name.clone(),
                            window_size: (0, 0),
                            mouse_screen: Vector::default(),
                            buttons_down: HashMap::new(),
                            background: vec![],
                            particles: vec![],
                            entities: vec![],
                            player_pos: Vector::default(),
                            player_dir: Vector::default(),
                            last_tick: time::Instant::now(),
                            entity_id_counter: 1,
                            collisions: CollisionMap::new(),
                            rng: Rng::new(),
                            begin_time: time::Instant::now(),
                        };

                        let player = Player {
                            name: player_name,
                            dir: Vector::default(),
                            pos: Vector::new(1.5, 1.5),
                            vel: Vector::default(),
                            force: Vector::default(),
                        };

                        state.entities.push(Entity::Player(state.player_id, player));

                        for y in 0..LOGO_HEIGHT {
                            for x in 0..LOGO_WIDTH {
                                let p = logo(x, y);
                                match p {
                                    Pixel::Black | Pixel::Green => {
                                        let wall = Wall {
                                            pos: (x, y),
                                            pixel: p,
                                        };
                                        let id = state.new_entitiy_id();
                                        state.entities.push(Entity::Wall(id, wall));
                                    }
                                    Pixel::White | Pixel::Grey => {
                                        let tile = Background::Floor(x as f64, y as f64, p);
                                        state.background.push(tile);
                                    }
                                }
                            }
                        }

                        return Stage::Playing(state);
                    }
                }
                Input::Press(button) => {
                    if !menu.buttons_down.contains_key(&button) {
                        menu.buttons_down.insert(button, time::Instant::now());
                    }
                }
                Input::Release(button) => {
                    menu.buttons_down.remove(&button);
                }
                Input::Move(Motion::MouseCursor(x, y)) => {
                    menu.mouse_screen = Vector::new(x as f32, y as f32);
                }
                _ => {}
            }

            Stage::Menu(menu)
        }
        Stage::Playing(mut state) => {
            // main game loop
            match e {
                Input::Render(a) => {
                    gl.draw(a.viewport(), |c, g| {
                        clear(WHITE, g);

                        let bias = if cfg!(debug_assertions) { 1.0 } else { 0.0 };
                        let elapsed = (into_secs(state.begin_time.elapsed()) + bias).min(1.0) as
                                      f32;

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
                    if let Some(wh) = window.window.get_inner_size_pixels() {
                        state.window_size = wh;
                    } else {
                        println!("window inner size error?");
                    }



                    state.update();
                }
                Input::Press(button) => {
                    if !state.buttons_down.contains_key(&button) {
                        state.buttons_down.insert(button, time::Instant::now());

                        if let Some(button) = convert_button(button) {
                            let msg = ra::ToServerMsg::Input(ra::PlayerId(state.player_id),
                                                             ra::Input::Press(button, state.player_dir));
                            send_input(stream, &msg);
                        }

                        // if button == Button::Mouse(MouseButton::Left) {
                        //     let b = Bullet {
                        //         pid: state.player_id,
                        //         pos: state.player_pos + state.player_dir * PLAYER_RADIUS,
                        //         vel: state.player_dir * 10.0,
                        //     };
                        //     let id = state.new_entitiy_id();
                        //     state.entities.push(Entity::Bullet(id, b));
                        // }
                    }
                }
                Input::Release(button) => {
                    if let Some(button) = convert_button(button) {
                        let msg = ra::ToServerMsg::Input(ra::PlayerId(state.player_id),
                                                         ra::Input::Release(button));
                        send_input(stream, &msg);
                    }
                    state.buttons_down.remove(&button);
                }
                Input::Move(Motion::MouseCursor(x, y)) => {
                    let x = x as f32;
                    let y = y as f32;
                    state.mouse_screen = Vector::new(x, y);
                }
                _ => {}
            }

            Stage::Playing(state)
        }
    }
}

fn send_input(stream: &mut TcpStream, msg: &ra::ToServerMsg) {
    if let Err(e) = bc::serialize_into(stream, &msg, bc::Infinite) {
        println!("Error while sending input: {}", e);
    }
}

fn convert_button(b: Button) -> Option<ra::Button> {
    // give a compile error if I forget a variant below
    match ra::Button::W {
        ra::Button::W => {}
        ra::Button::A => {}
        ra::Button::S => {}
        ra::Button::D => {}
        ra::Button::LeftMouse => {}
    }

    match b {
        Button::Keyboard(Key::W) => Some(ra::Button::W),
        Button::Keyboard(Key::A) => Some(ra::Button::A),
        Button::Keyboard(Key::S) => Some(ra::Button::S),
        Button::Keyboard(Key::D) => Some(ra::Button::D),
        Button::Mouse(MouseButton::Left) => Some(ra::Button::LeftMouse),
        _ => None,
    }
}

enum Stage {
    Menu(Menu),
    Playing(State),
}

struct Menu {
    buttons_down: HashMap<Button, time::Instant>,
    mouse_screen: Vector,
}

fn connect() -> TcpStream {
    TcpStream::connect(ADDR).unwrap_or_else(|e| panic!("Failed to connect: {}", e))
}

type CollisionMap = HashMap<u32, Vec<Collision>>;

struct State {
    game_state: GameState,
    player_id: u32,
    player_name: String,
    window_size: (u32, u32),
    mouse_screen: Vector,
    buttons_down: HashMap<Button, time::Instant>,
    background: Vec<Background>,
    particles: Vec<Particle>,
    entities: Vec<Entity>,
    player_pos: Vector,
    player_dir: Vector,
    last_tick: time::Instant,
    entity_id_counter: u32,
    collisions: CollisionMap,
    rng: Rng,
    begin_time: time::Instant,
}

impl State {
    fn new_entitiy_id(&mut self) -> u32 {
        let id = self.entity_id_counter;
        self.entity_id_counter += 1;
        id
    }

    fn draw(&mut self, ctx: &mut RenderContext) {
        for b in &self.background {
            b.draw(self, ctx);
        }

        self.entities
            .sort_by(|a, b| match (a, b) {
                         (&Entity::Wall(..), &Entity::Wall(..)) => Ordering::Equal,
                         (&Entity::Wall(..), _) => Ordering::Less,
                         (_, &Entity::Wall(..)) => Ordering::Greater,
                         (&Entity::Player(..), _) => Ordering::Greater,
                         (_, &Entity::Player(..)) => Ordering::Less,
                         _ => Ordering::Equal,
                     });

        for e in &self.entities {
            e.draw(self, ctx);
        }

        for p in &self.particles {
            p.draw(self, ctx);
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

        let particles = mem::replace(&mut self.particles, vec![]);

        for mut p in particles {
            p.update(self, dt);
            if !p.is_dead() {
                self.particles.push(p);
            }
        }
    }
}

enum Background {
    Floor(f64, f64, Pixel),
}

impl Background {
    fn draw(&self, _state: &State, ctx: &mut RenderContext) {
        let RenderContext {
            transforms,
            ref mut g,
            ..
        } = *ctx;

        match *self {
            Background::Floor(x, y, pixel) => {
                let r = rectangle::square(x, y, 1.0);
                rectangle(pixel.color(), r, transforms.tracking, *g);
            }
        }
    }
}

enum Particle {
    Spark(Spark),
}

impl Particle {
    fn draw(&self, _state: &State, ctx: &mut RenderContext) {
        let RenderContext {
            transforms,
            ref mut g,
            ..
        } = *ctx;

        match *self {
            Particle::Spark(Spark { pos, life, .. }) => {
                let (x, y) = as_f64s(pos);
                let shape = ellipse::Ellipse::new([1.0, 1.0 * life, 0.0, life]);
                let tile = rectangle::centered_square(x, y, (SPARK_RADIUS * 2.0 * life) as f64);
                let ds = DrawState::default().blend(draw_state::Blend::Add);

                shape.draw(tile, &ds, transforms.tracking, *g);
            }
        }
    }

    fn update(&mut self, _state: &mut State, dt: f32) {
        match *self {
            Particle::Spark(ref mut s) => {
                s.pos += s.vel;
                s.vel *= s.life.max(0.1);
                s.vel.rotate_deg_mut(s.spin * s.life);
                s.life -= dt;
            }
        }
    }

    fn is_dead(&self) -> bool {
        match *self {
            Particle::Spark(ref s) => s.life < 0.0,
        }
    }
}

#[derive(PartialEq)]
enum Entity {
    Bullet(u32, Bullet),
    Player(u32, Player),
    Wall(u32, Wall),
}

impl Entity {
    fn draw(&self, _state: &State, ctx: &mut RenderContext) {
        let RenderContext {
            transforms,
            ref mut g,
            ..
        } = *ctx;

        match *self {
            Entity::Bullet(_, Bullet { pos, vel, .. }) => {
                let vo = pos + vel.normalize() * -0.1;

                line(RED, 0.005, as_line(pos, vo), transforms.tracking, *g);
            }
            Entity::Player(_, Player { pos, dir, ref name, .. }) => {
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
            Entity::Wall(_, Wall { pos: (x, y), pixel }) => {
                let color = pixel.color();
                let tile = rectangle::square(x as f64, y as f64, 1.0);
                rectangle(color, tile, transforms.tracking, *g);
            }
        }
    }

    fn update(&mut self, state: &mut State, collisions: &CollisionMap, dt: f32) {
        match *self {
            Entity::Bullet(id, ref mut b) => {
                if let Some(c) = collisions.get(&id) {
                    if !c.is_empty() {
                        // spawn sparks
                        let n = state.rng.rand_int(5, 10);
                        for _ in 0..n {
                            let pos = b.pos;
                            let life = state.rng.rand_float(0.5, 1.0);
                            let vel = -b.vel
                                           .normalize()
                                           .rotate_deg(state.rng.rand_float(-30.0, 30.0)) *
                                      0.06;
                            let spin = state.rng.rand_float(-5.0, 5.0);
                            state
                                .particles
                                .push(Particle::Spark(Spark {
                                                          pos,
                                                          vel,
                                                          life,
                                                          spin,
                                                      }));
                        }

                        // kill this entity via culling in is_dead
                        b.pos.x = 100.0;
                    }
                }
                b.pos += b.vel * dt;
            }

            Entity::Player(id, ref mut player) => {
                if player.name == state.player_name {
                    player.force = Vector::new(0.0, 0.0);
                    update_player(player, state, dt);

                    if let Some(collisions) = collisions.get(&id) {
                        for c in collisions {
                            if let Collision::PlayerWall(wp) = *c {
                                let wc = wp + Vector::new(0.5, 0.5);

                                player.force += (player.pos - wc) * 100.0;
                            }
                        }
                    }

                    player.vel += player.force * dt;
                    player.vel = player.vel.normalize() * player.vel.magnitude().min(40.0) * 0.9;
                    player.pos += player.vel * dt;
                }
            }

            Entity::Wall { .. } => {}
        }
    }
    fn is_dead(&self) -> bool {
        match *self {
            Entity::Bullet(_, ref b) => b.pos.magnitude() > 20.0,
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

            let w_bounds = CSquare::new(Vector::new(wx, wy), 1.0);
            let p_bounds = CSquare::new_centered(pos, PLAYER_RADIUS * 1.1);

            if w_bounds.intersects(p_bounds) {
                Some((Collision::PlayerWall(Vector::new(wx, wy)), a1, a2))
            } else {
                None
            }
        }

        match (self, other) {
            (&Entity::Player(_, Player { pos, .. }),
             &Entity::Wall(_,
                           Wall {
                               pos: (wx, wy),
                               pixel,
                           })) => {
                if pixel == Pixel::Black || pixel == Pixel::Green {
                    player_wall(wx as f32, wy as f32, pos, true, false)
                } else {
                    None
                }
            }

            (&Entity::Wall(_,
                           Wall {
                               pos: (wx, wy),
                               pixel,
                           }),
             &Entity::Player(_, Player { pos, .. })) => {
                if pixel == Pixel::Black || pixel == Pixel::Green {
                    player_wall(wx as f32, wy as f32, pos, false, true)
                } else {
                    None
                }
            }

            (&Entity::Wall(..), &Entity::Wall(..)) => None,
            (&Entity::Bullet(..), &Entity::Bullet(..)) => None,

            (&Entity::Wall(_, Wall { pos: wpos, pixel }),
             &Entity::Bullet(_, Bullet { pos, vel, .. })) |
            (&Entity::Bullet(_, Bullet { pos, vel, .. }),
             &Entity::Wall(_, Wall { pos: wpos, pixel })) => {
                match pixel {
                    Pixel::Black | Pixel::Green => {
                        let wp = Vector::new(wpos.0 as f32, wpos.1 as f32);
                        let w_bounds = CSquare::new(wp, 1.0);
                        let vn = vel.normalize() * -0.1;
                        let b_bounds1 = CCircle::new(pos, BULLET_RADIUS);
                        let b_bounds2 = CCircle::new(pos - vn, BULLET_RADIUS);
                        let b_bounds3 = CCircle::new(pos + vn, BULLET_RADIUS);

                        if circle_intersects_square(b_bounds1, w_bounds) ||
                           circle_intersects_square(b_bounds2, w_bounds) ||
                           circle_intersects_square(b_bounds3, w_bounds) {
                            Some((Collision::BulletWall(wp), true, true))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }

            (&Entity::Player(pid, Player { pos: ppos, .. }),
             &Entity::Bullet(_, Bullet { pos, pid: from, .. })) |
            (&Entity::Bullet(_, Bullet { pos, pid: from, .. }),
             &Entity::Player(pid, Player { pos: ppos, .. })) => {
                if pid == from {
                    return None;
                }

                let p_bounds = CCircle::new(ppos, PLAYER_RADIUS);
                let b_bounds = CCircle::new(pos, BULLET_RADIUS);

                if p_bounds.intersects(b_bounds) {
                    Some((Collision::BulletPlayer(pid), true, true))
                } else {
                    None
                }
            }
            (&Entity::Player(..), &Entity::Player(..)) => unimplemented!(),
        }
    }
}

#[derive(Copy, Clone)]
enum Collision {
    // wall position
    PlayerWall(Vector),
    BulletWall(Vector),
    // player id
    BulletPlayer(u32),
}

#[derive(Copy, Clone)]
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

    fn intersects(&self, other: CSquare) -> bool {
        self.top_left.x < other.bottom_right.x && self.bottom_right.x > other.top_left.x &&
        self.top_left.y < other.bottom_right.y && self.bottom_right.y > other.top_left.y
    }

    fn contains(&self, p: Vector) -> bool {
        self.top_left.x < p.x && self.bottom_right.x > p.x && self.top_left.y < p.y &&
        self.bottom_right.y > p.y
    }
}

#[derive(Copy, Clone)]
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
    dir: Vector,
    pos: Vector,
    vel: Vector,
    force: Vector,
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
    pid: u32,
}

#[derive(PartialEq)]
struct Spark {
    pos: Vector,
    vel: Vector,
    spin: f32,
    life: f32,
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
            Pixel::White => color::grey(0.9),
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

    let move_force = |start: time::Instant| {
        let t = into_secs(start.elapsed()) as f32;
        let d = dt * 2.0;
        (d / (t + 1.5) + d);
        40.0
    };

    p.force = Vector::default();

    if let Some(&t) = state.buttons_down.get(&Button::Keyboard(Key::A)) {
        p.force.x -= move_force(t);
    }
    if let Some(&t) = state.buttons_down.get(&Button::Keyboard(Key::D)) {
        p.force.x += move_force(t);
    }
    if let Some(&t) = state.buttons_down.get(&Button::Keyboard(Key::W)) {
        p.force.y -= move_force(t);
    }
    if let Some(&t) = state.buttons_down.get(&Button::Keyboard(Key::S)) {
        p.force.y += move_force(t);
    }

    let (width, height) = state.window_size;
    let center = Vector::new(width as f32, height as f32) / 2.0;
    let mouse = state.mouse_screen - center;
    p.dir = (mouse - p.pos).normalize();

    state.player_pos = p.pos;
    state.player_dir = p.dir;
}
