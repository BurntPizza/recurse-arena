
extern crate recurse_arena as ra;
extern crate piston;
extern crate graphics;
extern crate glutin_window;
extern crate glutin;
extern crate opengl_graphics;
extern crate ludomath;
extern crate ezing as ez;
extern crate bincode as bc;
extern crate image;
extern crate tempfile;
extern crate ears;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;


use ra::{GameState, LOGO, LOGO_WIDTH, LOGO_HEIGHT, PLAYER_HEALTH, PLAYER_RADIUS, IntoSecs};

use std::io;
use std::time::{Instant, Duration};
use std::thread;
use std::process;
use std::io::prelude::*;
use std::net::TcpStream;
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{channel, Receiver};

use piston::window::*;
use piston::event_loop::*;
use piston::input::*;
use glutin_window::GlutinWindow;
use opengl_graphics::{GlGraphics, OpenGL, Texture, TextureSettings};
use opengl_graphics::glyph_cache::GlyphCache;
use graphics::*;
use graphics::draw_state::*;
use graphics::character::CharacterCache;
use graphics::types::{Color, Matrix2d};
use ludomath::vec2d::*;
use ludomath::consts::*;
use ludomath::rng::Rng;
use ears::AudioController;
use structopt::StructOpt;

const BLACK: Color = [0.0, 0.0, 0.0, 1.0];
const GREEN: Color = [0.0, 0.9, 0.0, 1.0];
const WHITE: Color = [1.0; 4];

static FONT: &[u8] = include_bytes!("../assets/Charybdis.ttf");

static GLOW: &[u8] = include_bytes!("../assets/green.png");
static BLUR: &[u8] = include_bytes!("../assets/blur.png");
static PUFF: &[u8] = include_bytes!("../assets/puff.png");
static SPRITE: &[u8] = include_bytes!("../assets/logo.png");

static SHOTS: &[&[u8]] = &[include_bytes!("../assets/Laser_Shoot1.ogg"),
                           include_bytes!("../assets/Laser_Shoot2.ogg"),
                           include_bytes!("../assets/Laser_Shoot3.ogg"),
                           include_bytes!("../assets/Laser_Shoot4.ogg")];

static HURTS: &[&[u8]] = &[include_bytes!("../assets/Hit_Hurt1.ogg"),
                           include_bytes!("../assets/Hit_Hurt2.ogg"),
                           include_bytes!("../assets/Hit_Hurt3.ogg"),
                           include_bytes!("../assets/Hit_Hurt4.ogg")];

static DEATH: &[u8] = include_bytes!("../assets/sfx_sound_shutdown1.ogg");
static SPLAT: &[u8] = include_bytes!("../assets/Splat.ogg");
static HIT: &[u8] = include_bytes!("../assets/Hitmarker.ogg");

static MUSIC: &[u8] = include_bytes!("../assets/Cut and Run.ogg");


#[derive(StructOpt)]
#[structopt(name = "Recurse Arena")]
struct Opt {
    #[structopt(help = "Player name")]
    username: String,
    #[structopt(help = "IP address of the server to connect to")]
    server_ip: String,
}


fn main() {
    let Opt {
        username,
        server_ip,
    } = Opt::from_args();

    let mut stream = connect(server_ip);
    let opengl = OpenGL::V3_2;
    let (full_width, full_height) = glutin::get_primary_monitor().get_dimensions();

    let mut window = WindowSettings::new("Recurse Arena", [full_width, full_height])
        .opengl(opengl)
        .exit_on_esc(true)
        .build()
        .unwrap();

    fn load_image(image: &[u8]) -> Texture {
        let dyn = image::load_from_memory_with_format(image, image::ImageFormat::PNG).unwrap();
        Texture::from_image(&dyn.to_rgba(), &TextureSettings::new())
    }

    fn load_sound(data: &[u8], max_vol: f32) -> ears::Sound {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        io::BufWriter::new(&mut file).write_all(data).unwrap();
        let mut sound = ears::Sound::new(file.path().to_str().unwrap()).unwrap();
        sound.set_max_volume(max_vol);
        sound
    }

    let glow = load_image(GLOW);
    let blur = load_image(BLUR);
    let puff = load_image(PUFF);
    let sprite = load_image(SPRITE);

    let shots = SHOTS.into_iter().map(|s| load_sound(s, 0.2)).collect();
    let hurts = HURTS.into_iter().map(|s| load_sound(s, 0.4)).collect();
    let death = load_sound(DEATH, 0.5);
    let splat = load_sound(SPLAT, 0.6);
    let hitmarker = load_sound(HIT, 0.6);

    let mut music = {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        io::BufWriter::new(&mut file).write_all(MUSIC).unwrap();
        let mut music = ears::Music::new(file.path().to_str().unwrap()).unwrap();
        music.set_looping(true);
        music.set_volume(0.5);
        music
    };

    let cache = GlyphCache::from_bytes(FONT).unwrap();
    let mut gl = GlGraphics::new(opengl);
    let mut events = Events::new(EventSettings::new());
    let (sender, reciever) = channel();

    let mut assets = Assets {
        cache,
        glow,
        blur,
        puff,
        sprite,
        shots,
        hurts,
        death,
        splat,
        hitmarker,
    };

    println!("Attempting to receive player_id from server...");
    let player_id = match bc::deserialize_from(&mut stream, bc::Infinite).unwrap() {
        ra::FromServerMsg::Welcome(id) => id,
        _ => unreachable!(),
    };

    println!("Got player_id from server: {}", player_id.0);

    let msg = ra::ToServerMsg::Login(player_id, username.clone());
    bc::serialize_into(&mut stream, &msg, bc::Infinite).unwrap();

    println!("Sent login request");

    println!("Spawning listener thread...");
    let mut stream_clone = stream.try_clone().unwrap();

    thread::spawn(move || {
        loop {
            // try to read new state from server
            match bc::deserialize_from(&mut stream_clone, bc::Infinite) {
                Ok(msg) => {
                    match msg {
                        ra::FromServerMsg::Update(gs) => {
                            if sender.send(gs).is_err() {
                                break;
                            }
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
    });

    let mut state = State {
        game_state: GameState {
            players: HashMap::new(),
            bullets: vec![],
            events: vec![],
        },
        player_id,
        window_size: (0, 0),
        mouse_screen: Vector::default(),
        buttons_down: HashMap::new(),
        particles: vec![],
        player_dir: Vector::default(),
        last_tick: Instant::now(),
        rng: Rng::new(),
        begin_time: Instant::now(),
        flash: Instant::now() - Duration::from_secs(10),
        messages: VecDeque::new(),
    };

    let player = ra::Player {
        health: PLAYER_HEALTH,
        id: player_id,
        name: username,
        dir: VEC_RIGHT,
        pos: Vector::new(1.5, 1.5),
        vel: VEC_ZERO,
        force: VEC_ZERO,
        respawn_timer: 0.0,
        score: 0,
    };

    state.game_state.players.insert(player_id, player);

    let mut stage = Stage::Playing(state);
    music.play();

    while let Some(e) = events.next(&mut window) {
        stage = step(e,
                     stage,
                     &mut gl,
                     &mut assets,
                     &mut window,
                     &mut stream,
                     &reciever);
    }
}

struct Assets<'a> {
    cache: GlyphCache<'a>,
    glow: Texture,
    blur: Texture,
    puff: Texture,
    sprite: Texture,
    shots: Vec<ears::Sound>,
    hurts: Vec<ears::Sound>,
    death: ears::Sound,
    splat: ears::Sound,
    hitmarker: ears::Sound,
}

fn step(e: Input,
        stage: Stage,
        gl: &mut GlGraphics,
        assets: &mut Assets,
        window: &mut GlutinWindow,
        mut stream: &mut TcpStream,
        reciever: &Receiver<GameState>)
        -> Stage {
    match stage {
        Stage::Playing(mut state) => {
            // main game loop
            match e {
                Input::Render(a) => {
                    gl.draw(a.viewport(), |c, g| {
                        clear(WHITE, g);
                        // Reset on respawn?
                        let elapsed = state.begin_time.elapsed().into_secs().min(1.0);
                        let original = c.transform;
                        let centered = original
                            .trans(a.width as f64 / 2.0, a.height as f64 / 2.0)
                            .zoom(ez::expo_in(elapsed) as f64 * 300.0);

                        let (px, py) = as_f64s(state.player_pos());
                        let tracking = centered.trans(-px, -py);
                        let transforms = Transforms { original, tracking };

                        let mut ctx = RenderContext {
                            transforms: &transforms,
                            g,
                            assets,
                        };

                        state.draw(&mut ctx);
                    });
                }
                Input::Update(_) => {
                    let dt = state.last_tick.elapsed().into_secs();
                    state.last_tick = Instant::now();
                    let Size { width, height } = window.draw_size();
                    state.window_size = (width, height);

                    let mut events = vec![];

                    for gs in reciever.try_iter() {
                        state.game_state = gs;
                        events.extend(state.game_state.events.drain(..));
                    }

                    for event in events {
                        match event {
                            ra::Event::BulletHitWall(ra::Bullet { pos, vel, .. }) => {
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

                                state.play_sound_at(&mut assets.splat, pos);
                            }

                            ra::Event::BulletHitPlayer(b, pid, _damage_fraction) => {
                                if b.pid == state.player_id {
                                    assets.hitmarker.play();
                                }

                                if pid == state.player_id {
                                    state.flash = Instant::now();
                                    let i = state.rng.rand_uint(0, assets.hurts.len() as u64) as
                                            usize;
                                    let sound = &mut assets.hurts[i];
                                    sound.play();
                                }
                            }

                            ra::Event::PlayerDied(killed, killer) => {
                                if state.player_id == killed {
                                    assets.death.play();
                                }


                                let killed = state.game_state.players[&killed].name.clone();
                                let killer = state.game_state.players[&killer].name.clone();
                                let msgs = [format!("{} was killed by {}", killed, killer),
                                            format!("{} got wrecked by {}", killed, killer),
                                            format!("{} was annihilated by {}", killed, killer),
                                            format!("{} didn't see {}", killed, killer)];

                                let i = state.rng.rand_uint(0, msgs.len() as u64) as usize;

                                state.messages.push_front((msgs[i].clone(), Instant::now()));
                            }

                            ra::Event::PlayerRespawned(id) => {
                                if state.player_id == id {
                                    let t = state.game_state.players[&id].respawn_timer;
                                    state.begin_time = Instant::now() -
                                                       Duration::from_millis((t * 1000.0) as u64);
                                }
                            }

                            ra::Event::BulletFired(pos) => {
                                let i = state.rng.rand_uint(0, assets.shots.len() as u64) as usize;
                                state.play_sound_at(&mut assets.shots[i], pos);
                            }

                            ra::Event::PlayerJoined(id) => {
                                if id == state.player_id {
                                    continue;
                                }

                                let name = &state.game_state.players[&id].name;
                                state
                                    .messages
                                    .push_front((format!("{} has joined the game", name),
                                                 Instant::now()));
                            }

                            ra::Event::PlayerLeft(name) => {
                                state
                                    .messages
                                    .push_front((format!("{} left the game", name),
                                                 Instant::now()));
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
                        state.buttons_down.insert(button, Instant::now());

                        if let Some(button) = convert_button(button) {
                            let msg =
                                ra::ToServerMsg::Input(state.player_id,
                                                       ra::Input::Press(button,
                                                                        state.player_dir()));
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
                    let player_pos = state.player_pos();
                    state.player_dir = (mouse - player_pos).normalize();

                    let input = ra::Input::DirChanged(state.player_dir);
                    let msg = ra::ToServerMsg::Input(state.player_id, input);
                    send_input(stream, &msg);
                }
                _ => {}
            }

            Stage::Playing(state)
        }
    }
}

fn send_input(stream: &mut TcpStream, msg: &ra::ToServerMsg) {
    if let Err(e) = bc::serialize_into(stream, &msg, bc::Infinite) {
        panic!("Error while sending input: {}", e);
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
    Playing(State),
}

fn connect(ip: String) -> TcpStream {
    println!("Connecting to {}...", ip);

    match TcpStream::connect(ip) {
        Ok(stream) => stream,
        Err(e) => {
            println!("Failed to connect: {}", e);
            process::exit(-1);
        }
    }
}

struct State {
    game_state: GameState,
    player_id: ra::PlayerId,
    window_size: (u32, u32),
    mouse_screen: Vector,
    buttons_down: HashMap<Button, Instant>,
    particles: Vec<Particle>,
    player_dir: Vector,
    last_tick: Instant,
    rng: Rng,
    begin_time: Instant,
    flash: Instant,
    messages: VecDeque<(String, Instant)>,
}

impl State {
    fn play_sound_at(&self, sound: &mut ears::Sound, pos: Vector) {
        sound.set_relative(true);
        let spos = (pos - self.player_pos()) * 5.0;
        sound.set_position([spos.x, -spos.y, 0.0]);
        sound.play();
    }

    fn player_pos(&self) -> Vector {
        self.game_state.players[&self.player_id].pos
    }

    fn player_dir(&self) -> Vector {
        self.player_dir
    }

    fn draw(&mut self, ctx: &mut RenderContext) {
        for y in 0..LOGO_HEIGHT {
            for x in 0..LOGO_WIDTH {
                let color = logo(x, y);
                let r = rectangle::square(x as f64, y as f64, 1.0);
                rectangle(color, r, ctx.transforms.tracking, ctx.g);
            }
        }

        for &ra::Bullet { pos, vel, pid, .. } in &self.game_state.bullets {
            let mut color = ra::color_for_id(pid);
            color[0] = (color[0] * 2.0).min(1.0);
            color[1] = (color[1] * 2.0).min(1.0);
            color[2] = (color[2] * 2.0).min(1.0);

            let (x, y) = as_f64s(pos);
            let ds = DrawState::default().blend(Blend::Add).blend(Blend::Alpha);

            let transform = ctx.transforms
                .tracking
                .trans(x, y)
                .zoom(1.0 / 600.0)
                .rot_rad(vel.angle_rad() as f64)
                .scale(1.5, 0.7)
                .trans(-44.0, -16.0);
            graphics::image::Image::new_color(color).draw(&ctx.assets.blur, &ds, transform, ctx.g);
        }

        for &ra::Player {
                id,
                pos,
                dir,
                health,
                ref name,
                ..
            } in self.game_state.players.values() {

            let color = ra::color_for_id(id);
            let (px, py) = as_f64s(pos);

            let dir = if id == self.player_id {
                self.player_dir() // use local info
            } else {
                dir
            };

            let ds = DrawState::default().blend(Blend::Add).blend(Blend::Alpha);
            let r = dir.angle_rad() as f64 - TAU as f64 * 0.25;

            let transform = ctx.transforms
                .tracking
                .trans(px, py)
                .zoom(1.0 / 600.0)
                .rot_rad(r)
                .trans(-100.0, -125.0);
            graphics::image::Image::new_color(color)
                .draw(&ctx.assets.sprite, &ds, transform, ctx.g);

            if id != self.player_id {
                text(color,
                     20,
                     &format!("{} | {}", name, health.max(0.0) as u32),
                     &mut ctx.assets.cache,
                     ctx.transforms
                         .tracking
                         .trans(px, py)
                         .trans(-PLAYER_RADIUS as f64, -PLAYER_RADIUS as f64 * 1.3)
                         .zoom(1.0 / 300.0),
                     ctx.g);
            } else if health == 0.0 {
                self.flash = Instant::now();
            }
        }

        for p in &self.particles {
            p.draw(self, ctx);
        }

        for y in 0..LOGO_HEIGHT {
            for x in 0..LOGO_WIDTH {
                if logo(x, y) == GREEN {
                    graphics::image(&ctx.assets.glow,
                                    ctx.transforms
                                        .tracking
                                        .trans(x as f64, y as f64)
                                        .zoom(1.0 / 100.0)
                                        .trans(-14.0, -14.0),
                                    ctx.g)
                }
            }
        }

        let f_duration = 0.3;
        let flash = self.flash.elapsed().into_secs().min(f_duration) / f_duration;
        if flash < 1.0 {
            let color = 1.0 - ez::cubic_out(flash);
            let (w, h) = self.window_size;
            let r = rectangle::rectangle_by_corners(0.0, 0.0, w as f64, h as f64);
            rectangle([color, 0.0, 0.0, 0.2], r, ctx.transforms.original, ctx.g);
        }

        let (w, h) = self.window_size;
        let w = w as f64;
        let h = h as f64;
        let duration = 4.0;
        let size = 20;

        for (i, &(ref msg, s)) in self.messages.iter().enumerate() {
            let time = s.elapsed().into_secs().min(duration) / duration;
            let c = ez::expo_in(time);
            let xo = 10.0;
            let yo = 10.0;
            let rw = ctx.assets.cache.width(size, msg);
            let rh = 30.0;
            let r = [0.0, -rh * 0.75, rw, rh];
            let t = ctx.transforms
                .original
                .trans(xo, h - yo - rh - i as f64 * rh);
            rectangle([1.0, 1.0, 1.0, 0.1 * (1.0 - c)], r, t, ctx.g);
            text([0.0, 0.0, 0.0, 1.0 - c],
                 size,
                 msg,
                 &mut ctx.assets.cache,
                 t,
                 ctx.g);
        }

        let mut scores = vec![];

        for p in self.game_state.players.values() {
            scores.push((&p.name, p.score));
        }

        scores.sort_by(|&(n1, s1), &(n2, s2)| s2.cmp(&s1).then(n1.cmp(n2)));

        for (i, (name, score)) in scores.into_iter().enumerate().take(10) {
            let msg = format!("{} | {}", name, score);
            let xo = 10.0;
            let yo = 10.0;
            let rw = ctx.assets.cache.width(size, &msg);
            let rh = 30.0;
            let r = [0.0, -rh * 0.75, rw, rh];
            let t = ctx.transforms
                .original
                .trans(w - xo - rw, yo + rh + i as f64 * rh);
            rectangle([1.0, 1.0, 1.0, 0.1], r, t, ctx.g);
            text([0.0, 0.0, 0.0, 1.0],
                 size,
                 &msg,
                 &mut ctx.assets.cache,
                 t,
                 ctx.g);
        }

        self.messages
            .retain(|&(_, i)| i.elapsed().into_secs() < duration);

        let health = self.game_state.players[&self.player_id].health / PLAYER_HEALTH;
        let rw = 300.0;
        let rh = 30.0;
        let yo = 10.0;
        let m = 2.0;

        let r = [w / 2.0 - rw / 2.0, h - rh - yo, rw, rh];

        let h = [r[0] + m,
                 r[1] + m,
                 r[2] - m - m - ((1.0 - health as f64) * (rw - m - m)),
                 r[3] - m - m];

        rectangle(color::grey(0.6), r, ctx.transforms.original, ctx.g);
        let c = health * 0.8 + 0.1;
        rectangle([0.9, c, c, 1.0], h, ctx.transforms.original, ctx.g);
    }
}

enum Particle {
    Spark(Spark),
}

impl Particle {
    fn draw(&self, _state: &State, ctx: &mut RenderContext) {
        match *self {
            Particle::Spark(Spark { pos, life, vel, .. }) => {
                let (x, y) = as_f64s(pos);
                let color = [1.0, 1.0 * life, 0.0, life];
                let ds = DrawState::default().blend(Blend::Add).blend(Blend::Alpha);
                let vm = vel.magnitude() as f64;
                let r = (pos.x + pos.y + vel.x + vel.y + life * 10.0) as f64;
                let life = life as f64;

                let transform = ctx.transforms
                    .tracking
                    .trans(x, y)
                    .zoom(1.0 / 600.0)
                    .rot_rad(vel.angle_rad() as f64)
                    .scale(life * (vm * 100.0).max(1.0), life)
                    .rot_rad(r * 10.0)
                    .trans(-16.0, -16.0);

                graphics::image::Image::new_color(color)
                    .draw(&ctx.assets.puff, &ds, transform, ctx.g);
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
    original: Matrix2d,
    tracking: Matrix2d,
}

struct RenderContext<'a, 'b: 'a> {
    transforms: &'a Transforms,
    g: &'a mut GlGraphics,
    assets: &'a mut Assets<'b>,
}

fn logo(x: usize, y: usize) -> Color {
    assert!(x < LOGO_WIDTH);
    assert!(y < LOGO_HEIGHT);

    let row = &LOGO[y];
    let pixel = row.bytes().nth(x).unwrap();

    match pixel {
        b'b' => BLACK,
        b'w' => color::grey(0.8),
        b'i' => WHITE,
        b'g' => GREEN,
        b'f' => color::grey(0.18),
        _ => unreachable!(),
    }
}

fn f(x: f32) -> f64 {
    x as f64
}

fn as_f64s(v: Vector) -> (f64, f64) {
    (f(v.x), f(v.y))
}
