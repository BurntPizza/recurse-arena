
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
    let es = EventSettings::new();
    let mut events = Events::new(es);

    // state
    let mut rng = Rng::new_seeded(44642246, 4645753);
    let mut buttons_down = HashMap::new();
    let mut listeners: HashMap<Button, Vec<Listener>> = HashMap::new();

    let mut entities: Vec<Entity> = vec![];

    let mut state = State {
        pos: Vector::new(1.0, 1.0),
        dir: Vector::new(1.0, 0.0),
        mouse_screen: Vector::new(0.0, 0.0),
    };

    let side_len: f64 = 1.0;
    let small_side_len: f64 = side_len * 0.5;

    // relative to player position
    let muzzle = |dir: Vector| {
        let len = small_side_len / 2.0;
        dir * len as f32
    };

    let begin_time = time::Instant::now();

    while let Some(e) = events.next(&mut window) {
        match e {
            Input::Render(a) => {
                gl.draw(a.viewport(), |c, g| {
                    clear(WHITE, g);

                    let elapsed = (into_secs(begin_time.elapsed())).min(1.0) as f32;

                    let centered = c.transform
                        .trans(a.width as f64 / 2.0, a.height as f64 / 2.0)
                        .zoom(ez::expo_in(elapsed) as f64 * 300.0);

                    let tracking = centered.trans(f(-state.pos.x), f(-state.pos.y));


                    for y in 0..LOGO_HEIGHT {
                        for x in 0..LOGO_WIDTH {
                            let color = logo(x, y).color();
                            let x = x as f64;
                            let y = y as f64;

                            let tile = rectangle::centered_square(x, y, side_len / 2.0);
                            rectangle(color, tile, tracking, g);
                        }
                    }

                    for e in &entities {
                        e.draw(g, tracking);
                    }

                    let player_box = rectangle::centered_square(0.0, 0.0, small_side_len / 2.0);

                    circle_arc(RED, 0.03, 0.0, TAU, player_box, centered, g);

                    let end_point = muzzle(state.dir);
                    line(RED,
                         0.01,
                         [0.0, 0.0, f(end_point.x), f(end_point.y)],
                         centered,
                         g);
                });
            }
            Input::Update(a) => {
                // every tick

                cull_entities(&mut entities);

                let move_amount = |start: time::Instant| {
                    let t = into_secs(start.elapsed());
                    let d = a.dt * 2.0;
                    (d / (t + 1.5) + d) as f32
                };

                if let Some(&t) = buttons_down.get(&Button::Keyboard(Key::A)) {
                    state.pos.x -= move_amount(t);
                }
                if let Some(&t) = buttons_down.get(&Button::Keyboard(Key::D)) {
                    state.pos.x += move_amount(t);
                }
                if let Some(&t) = buttons_down.get(&Button::Keyboard(Key::W)) {
                    state.pos.y -= move_amount(t);
                }
                if let Some(&t) = buttons_down.get(&Button::Keyboard(Key::S)) {
                    state.pos.y += move_amount(t);
                }

                if let Some((width, height)) = window.window.get_inner_size_pixels() {
                    let center = Vector::new(width as f32, height as f32) / 2.0;
                    let mouse = state.mouse_screen - center;
                    state.dir = (mouse - state.pos).normalize();
                } else {
                    println!("window inner size error?");
                }

                for e in entities.iter_mut() {
                    e.update();
                }

                window.set_title(format!("{}", entities.len()));
            }
            Input::Press(button) => {
                if !buttons_down.contains_key(&button) {
                    // for listener in listeners.entry(button).or_insert(vec![]) {
                    //     listener.fire();
                    // }

                    buttons_down.insert(button, time::Instant::now());

                    if button == Button::Mouse(MouseButton::Left) {
                        let b = Bullet {
                            pos: state.pos + muzzle(state.dir),
                            vel: state.dir * 0.15,
                        };
                        entities.push(Entity::Bullet(b));
                    }
                }
            }
            Input::Release(button) => {
                buttons_down.remove(&button);
            }
            Input::Move(Motion::MouseCursor(x, y)) => {
                state.mouse_screen = Vector::new(x as f32, y as f32);
            }
            _ => {}
        }
    }
}

#[derive(Copy, Clone)]
struct State {
    pos: Vector,
    dir: Vector,
    mouse_screen: Vector,
}

enum Entity {
    Bullet(Bullet),
}

impl Entity {
    fn draw(&self, g: &mut GlGraphics, t: Matrix2d) {
        match *self {
            Entity::Bullet(ref b) => {
                let x = f(b.pos.x);
                let y = f(b.pos.y);
                let vo = b.vel.normalize() * 0.1;
                let vx = f(vo.x);
                let vy = f(vo.y);

                line(RED, 0.005, [0.0, 0.0, vx, vy], t.trans(x, y), g);
            }
        }
    }
    fn update(&mut self) {
        match *self {
            Entity::Bullet(ref mut b) => {
                b.pos += b.vel;
            }
        }
    }
    fn is_dead(&self) -> bool {
        match *self {
            Entity::Bullet(ref b) => b.pos.magnitude() > 20.0,
        }
    }
}

struct Bullet {
    pos: Vector,
    vel: Vector,
}

struct Listener(Box<FnMut()>);
impl Listener {
    fn fire(&mut self) {
        self.0()
    }
}

enum Pixel {
    Black,
    White,
    Green,
    Grey,
}

impl Pixel {
    fn color(self) -> Color {
        match self {
            Pixel::Black => BLACK,
            Pixel::White => WHITE,
            Pixel::Green => GREEN,
            Pixel::Grey => color::grey(0.1),
        }
    }
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

fn cull_entities(entities: &mut Vec<Entity>) {
    entities.retain(|e| !e.is_dead());
}
