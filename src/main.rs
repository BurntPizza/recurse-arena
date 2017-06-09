
extern crate piston;
extern crate graphics;
extern crate glutin_window;
extern crate glutin;
extern crate opengl_graphics;
extern crate find_folder;
extern crate ludomath;
extern crate ezing as ez;
// extern crate ncollide;

use std::collections::{HashMap, HashSet};
use std::time;
use std::mem;
use std::cmp::Ordering;

use piston::window::*;
use piston::event_loop::*;
use piston::input::*;
// use piston::input::

use glutin_window::GlutinWindow as Window;
use opengl_graphics::{GlGraphics, OpenGL};
use opengl_graphics::glyph_cache::GlyphCache;
use graphics::*;
use graphics::types::{Color, Matrix2d};

use ludomath::vec2d::*;
use ludomath::rng::Rng;


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

fn main() {
    assert!(LOGO.iter().all(|r| r.len() == LOGO_WIDTH));
    assert_eq!(LOGO.len(), LOGO_HEIGHT);

    let (full_width, full_height) = glutin::get_primary_monitor().get_dimensions();

    let opengl = OpenGL::V3_2;

    let mut window: Window = WindowSettings::new("Game", [full_width, full_height])
        .opengl(opengl)
        .exit_on_esc(true)
        .build()
        .unwrap();

    let assets = find_folder::Search::ParentsThenKids(3, 3)
        .for_folder("assets")
        .unwrap();

    let font = &assets.join("FiraSans-Regular.ttf");
    let mut cache = GlyphCache::new(font).unwrap();

    let mut gl = GlGraphics::new(opengl);
    let mut events = Events::new(EventSettings::new());

    let player_name = (into_secs(time::SystemTime::now()
                                     .duration_since(time::UNIX_EPOCH)
                                     .expect("Sytem time error")) *
                       1000.0) as u64;

    let player_name = player_name.to_string();

    let mut state = State::new(player_name.clone());

    let player = Player {
        name: player_name,
        pos: Vector::new(1.5, 1.5),
        dir: Vector::default(),
    };

    state.entities.push(Entity::Player(player));

    for y in 0..LOGO_HEIGHT {
        for x in 0..LOGO_WIDTH {
            let wall = Wall {
                pos: (x, y),
                pixel: logo(x, y),
            };
            state.entities.push(Entity::Wall(wall));
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

                    let ctx = RenderContext {
                        transforms: &transforms,
                        g,
                    };

                    state.draw(ctx);
                });
            }
            Input::Update(a) => {
                // every tick

                if let Some(wh) = window.window.get_inner_size_pixels() {
                    state.window_size = wh;
                } else {
                    println!("window inner size error?");
                }

                state.update();

                window.set_title(format!("{}", state.entities.len()));
            }
            Input::Press(button) => {
                if !state.buttons_down.contains_key(&button) {
                    state.buttons_down.insert(button, time::Instant::now());

                    if button == Button::Mouse(MouseButton::Left) {
                        let b = Bullet {
                            pos: state.player_pos + state.player_dir * PLAYER_RADIUS,
                            vel: state.player_dir * 0.15,
                        };

                        state.entities.push(Entity::Bullet(b));
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

struct State {
    player_name: String,
    window_size: (u32, u32),
    mouse_screen: Vector,
    buttons_down: HashMap<Button, time::Instant>,
    entities: Vec<Entity>,
    player_pos: Vector,
    player_dir: Vector,
    last_tick: time::Instant,
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
        }
    }

    fn draw(&mut self, mut ctx: RenderContext) {
        self.entities
            .sort_by(|a, b| match (a, b) {
                         (&Entity::Wall(_), _) => Ordering::Less,
                         (_, &Entity::Wall(_)) => Ordering::Greater,
                         (&Entity::Player(_), _) => Ordering::Greater,
                         (_, &Entity::Player(_)) => Ordering::Less,
                         _ => Ordering::Equal,
                     });

        for e in &self.entities {
            e.draw(self, &mut ctx);
        }
    }

    fn update(&mut self) {
        let dt = into_secs(self.last_tick.elapsed()) as f32;
        self.last_tick = time::Instant::now();

        let entities = mem::replace(&mut self.entities, vec![]);

        for mut e in entities {
            e.update(self, dt);
            if !e.is_dead() {
                self.entities.push(e);
            }
        }
    }
}

enum Entity {
    Bullet(Bullet),
    Player(Player),
    Wall(Wall),
}

impl Entity {
    fn draw(&self, state: &State, ctx: &mut RenderContext) {
        let RenderContext { transforms, ref mut g } = *ctx;

        match *self {
            Entity::Bullet(Bullet { pos, vel, .. }) => {
                let vo = pos + vel.normalize() * 0.1;

                line(RED, 0.005, as_line(pos, vo), transforms.tracking, *g);
            }
            Entity::Player(Player { pos, dir, .. }) => {
                let (px, py) = as_f64s(pos);
                let player_box = rectangle::centered_square(px, py, PLAYER_RADIUS as f64);
                let end_point = pos + dir * PLAYER_RADIUS;

                circle_arc(RED, 0.03, 0.0, TAU, player_box, transforms.tracking, *g);
                line(RED, 0.01, as_line(pos, end_point), transforms.tracking, *g);
            }
            Entity::Wall(Wall {
                             pos: (x, y),
                             pixel,
                         }) => {
                let color = pixel.color();
                let tile = rectangle::square(x as f64, y as f64, 1.0);
                rectangle(color, tile, transforms.tracking, *g);
            }
        }
    }
    fn update(&mut self, state: &mut State, dt: f32) {
        match *self {
            Entity::Bullet(ref mut b) => {
                b.pos += b.vel;
            }
            Entity::Player(Player {
                               ref name,
                               ref mut pos,
                               ref mut dir,
                           }) => {
                if *name == state.player_name {
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
            }
            Entity::Wall { .. } => {}
        }
    }
    fn is_dead(&self) -> bool {
        match *self {
            Entity::Bullet(ref b) => b.pos.magnitude() > 20.0,
            Entity::Wall { .. } => false,
            Entity::Player { .. } => false, //TODO
        }
    }
}

struct Player {
    name: String,
    pos: Vector,
    dir: Vector,
}
struct Wall {
    pos: (usize, usize),
    pixel: Pixel,
}

struct Bullet {
    pos: Vector,
    vel: Vector,
}

struct Transforms {
    original: Matrix2d,
    centered: Matrix2d,
    tracking: Matrix2d,
}

#[derive(Copy, Clone)]
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

struct RenderContext<'a> {
    transforms: &'a Transforms,
    g: &'a mut GlGraphics,
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
