#![allow(non_snake_case)]

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

use ra::{GameState, LOGO, LOGO_WIDTH, LOGO_HEIGHT, PLAYER_RADIUS, BULLET_RADIUS, CSquare, IntoSecs};

use std::collections::HashMap;
use std::time;
use std::net::TcpStream;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;


use piston::window::*;
use piston::event_loop::*;
use piston::input::*;
use glutin_window::GlutinWindow;
use opengl_graphics::{GlGraphics, OpenGL};
use opengl_graphics::glyph_cache::GlyphCache;
use graphics::*;
use graphics::types::{Color, Matrix2d};
use ludomath::vec2d::*;
use ludomath::rng::Rng;

const ADDR: &str = "localhost:8000";

const BLACK: Color = [0.0, 0.0, 0.0, 1.0];
const GREEN: Color = [0.0, 0.9, 0.0, 1.0];
const WHITE: Color = [1.0; 4];
const RED: Color = [1.0, 0.0, 0.0, 1.0];

const TAU: f64 = 6.2831853;


const SPARK_RADIUS: f32 = 0.01;

static FONT: &[u8] = include_bytes!("../assets/FiraSans-Regular.ttf");

fn main() {
    let mut stream = connect();
    let opengl = OpenGL::V3_2;
    let (full_width, full_height) = glutin::get_primary_monitor().get_dimensions();

    let mut window = WindowSettings::new("Recurse Arena", [full_width, full_height])
        .opengl(opengl)
        .exit_on_esc(true)
        .build()
        .unwrap();

    let mut cache = GlyphCache::from_bytes(FONT).unwrap();
    let mut gl = GlGraphics::new(opengl);
    let mut events = Events::new(EventSettings::new());
    let (sender, reciever) = channel();

    let menu = Menu {
        buttons_down: HashMap::new(),
        mouse_screen: Vector::default(),
    };

    let mut stage = Stage::Menu(menu);


    while let Some(e) = events.next(&mut window) {
        stage = step(e,
                     stage,
                     &mut gl,
                     &mut cache,
                     &mut window,
                     &mut stream,
                     &sender,
                     &reciever);
    }
}

fn step(e: Input,
        stage: Stage,
        gl: &mut GlGraphics,
        mut cache: &mut GlyphCache,
        window: &mut GlutinWindow,
        mut stream: &mut TcpStream,
        sender: &Sender<GameState>,
        reciever: &Receiver<GameState>)
        -> Stage {
    match stage {
        Stage::Menu(mut menu) => {
            // menu screen game loop
            match e {
                Input::Render(a) => {
                    gl.draw(a.viewport(), |_c, g| { clear(WHITE, g); });
                }
                Input::Update(_) => {
                    if true |
                       menu.buttons_down
                           .contains_key(&Button::Keyboard(Key::Space)) {
                        // begin game

                        let player_name = "Josh".to_owned();

                        println!("Attempting to receive player_id from server...");
                        let player_id = match bc::deserialize_from(&mut stream, bc::Infinite)
                                  .unwrap() {
                            ra::FromServerMsg::Welcome(id) => id,
                            _ => unreachable!(),
                        };

                        println!("Got player_id from server: {}", player_id.0);

                        let msg = ra::ToServerMsg::Login(player_id, player_name.clone());
                        bc::serialize_into(&mut stream, &msg, bc::Infinite).unwrap();

                        println!("Sent login request");

                        println!("Spawning listener thread...");
                        let sender = sender.clone();
                        let mut stream_clone = stream.try_clone().unwrap();

                        thread::spawn(move || {
                            loop {
                                // try to read new state from server
                                match bc::deserialize_from(&mut stream_clone, bc::Infinite) {
                                    Ok(msg) => {
                                        match msg {
                                            ra::FromServerMsg::Update(gs) => {
                                                sender.send(gs).unwrap()
                                            }
                                            _ => panic!("Protocol error / unimplemented"),
                                        }
                                    }
                                    Err(e) => {
                                        println!("Listener thread: Error: {}", e);
                                        break;
                                    }
                                }
                            }
                            println!("Lister thread: Stopping...");
                        });

                        let mut state = State {
                            game_state: GameState {
                                players: HashMap::new(),
                                bullets: vec![],
                                events: vec![],
                            },
                            player_id,
                            player_name: player_name.clone(),
                            window_size: (0, 0),
                            mouse_screen: Vector::default(),
                            buttons_down: HashMap::new(),
                            particles: vec![],
                            player_pos: Vector::default(),
                            player_dir: Vector::default(),
                            last_tick: time::Instant::now(),
                            rng: Rng::new(),
                            begin_time: time::Instant::now(),
                        };

                        let player = ra::Player {
                            id: player_id,
                            name: player_name,
                            dir: Vector::default(),
                            pos: Vector::new(1.5, 1.5),
                            vel: Vector::default(),
                            force: Vector::default(),
                        };

                        state.game_state.players.insert(player_id, player);

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

                        let elapsed = into_secs(state.begin_time.elapsed()).min(1.0) as f32;
                        let centered = c.transform
                            .trans(a.width as f64 / 2.0, a.height as f64 / 2.0)
                            .zoom(ez::expo_in(elapsed) as f64 * 300.0);

                        let (px, py) = as_f64s(state.game_state.players[&state.player_id].pos);

                        let tracking = centered.trans(-px, -py);

                        let transforms = Transforms { centered, tracking };

                        let mut ctx = RenderContext {
                            transforms: &transforms,
                            g,
                            cache: &mut cache,
                        };

                        state.draw(&mut ctx);
                    });
                }
                Input::Update(_) => {
                    let dt = state.last_tick.elapsed().into_secs();
                    state.last_tick = time::Instant::now();

                    let Size { width, height } = window.draw_size();
                    state.window_size = (width, height);

                    let mut events = vec![];

                    for gs in reciever.try_iter() {
                        state.game_state = gs;
                        events.extend(state.game_state.events.drain(..));
                    }

                    for event in events {
                        match event {
                            ra::Event::BulletHitWall(pos, vel) => {
                                // spawn sparks
                                let n = state.rng.rand_int(5, 10);
                                for _ in 0..n {
                                    let life = state.rng.rand_float(0.5, 1.0);
                                    let angle = state.rng.rand_float(-30.0, 30.0);
                                    let spin = state.rng.rand_float(-5.0, 5.0);
                                    let vel = -vel.normalize().rotate_deg(angle) * 0.06;
                                    let spark = Spark {
                                        pos,
                                        vel,
                                        life,
                                        spin,
                                    };

                                    state.particles.push(Particle::Spark(spark));
                                }
                            }
                        }
                    }

                    for i in (0..state.particles.len()).rev() {
                        if state.particles[i].update(dt) {
                            state.particles.remove(i);
                        }
                    }
                }
                Input::Press(button) => {
                    if !state.buttons_down.contains_key(&button) {
                        state.buttons_down.insert(button, time::Instant::now());

                        if let Some(button) = convert_button(button) {
                            let msg = ra::ToServerMsg::Input(state.player_id,
                                                             ra::Input::Press(button,
                                                                              state.player_dir));
                            send_input(stream, &msg);
                        }
                    }
                }
                Input::Release(button) => {
                    if let Some(button) = convert_button(button) {
                        let msg = ra::ToServerMsg::Input(state.player_id,
                                                         ra::Input::Release(button));
                        send_input(stream, &msg);
                    }
                    state.buttons_down.remove(&button);
                }
                Input::Move(Motion::MouseCursor(x, y)) => {
                    let x = x as f32;
                    let y = y as f32;
                    state.mouse_screen = Vector::new(x, y);

                    let (width, height) = state.window_size;
                    let center = Vector::new(width as f32, height as f32) / 2.0;
                    let mouse = state.mouse_screen - center;
                    let player_pos = state.game_state.players[&state.player_id].pos;
                    state.player_dir = (mouse - player_pos).normalize();
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

struct State {
    game_state: GameState,
    player_id: ra::PlayerId,
    player_name: String,
    window_size: (u32, u32),
    mouse_screen: Vector,
    buttons_down: HashMap<Button, time::Instant>,
    particles: Vec<Particle>,
    player_pos: Vector,
    player_dir: Vector,
    last_tick: time::Instant,
    rng: Rng,
    begin_time: time::Instant,
}

impl State {

    fn draw(&mut self, ctx: &mut RenderContext) {
        for y in 0..LOGO_HEIGHT {
            for x in 0..LOGO_WIDTH {
                let color = logo(x, y).color();
                let r = rectangle::square(x as f64, y as f64, 1.0);
                rectangle(color, r, ctx.transforms.tracking, ctx.g);
            }
        }

        // TODO: add bullet = player colors
        for &ra::Bullet { pos, vel, .. } in &self.game_state.bullets {
            let vo = pos + vel.normalize() * -0.1;
            let shape = line::Line::new([1.0, 0.2, 0.0, 0.9], 0.005);
            let ds = DrawState::default().blend(draw_state::Blend::Add);
            shape.draw(as_line(vo, pos), &ds, ctx.transforms.tracking, ctx.g);
        }

        for &ra::Player { pos, dir, ref name, .. } in self.game_state.players.values() {
            let (px, py) = as_f64s(pos);
            let player_box = rectangle::centered_square(px, py, PLAYER_RADIUS as f64);
            let end_point = pos + self.player_dir * PLAYER_RADIUS;

            circle_arc(RED,
                       0.03,
                       0.0,
                       TAU,
                       player_box,
                       ctx.transforms.tracking,
                       ctx.g);
            line(RED,
                 0.01,
                 as_line(pos, end_point),
                 ctx.transforms.tracking,
                 ctx.g);
            text(RED,
                 20,
                 name,
                 ctx.cache,
                 ctx.transforms
                     .centered
                     .trans(-PLAYER_RADIUS as f64, -PLAYER_RADIUS as f64 * 1.3)
                     .zoom(1.0 / 300.0),
                 ctx.g);
        }

        for p in &self.particles {
            p.draw(self, ctx);
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

    fn update(&mut self, dt: f32) -> bool {
        match *self {
            Particle::Spark(ref mut s) => {
                s.pos += s.vel;
                s.vel *= s.life.max(0.1);
                s.vel.rotate_deg_mut(s.spin * s.life);
                s.life -= dt;
                s.life < 0.0
            }
        }
    }
}

#[derive(PartialEq)]
struct Spark {
    pos: Vector,
    vel: Vector,
    spin: f32,
    life: f32,
}

struct Transforms {
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
