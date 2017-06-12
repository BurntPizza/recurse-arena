
extern crate recurse_arena;
extern crate bincode as bc;
extern crate ludomath;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;

use recurse_arena::*;

use std::thread;
use std::io::prelude::*;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::net::{TcpListener, TcpStream};
use std::time::{Instant, Duration};

use ludomath::rng::Rng;
use ludomath::vec2d::*;
use ludomath::consts::*;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(name = "Recurse Arena Server")]
struct Opt {
    #[structopt(help = "Port to listen on", default_value = "8000")]
    port: String,
}


fn main() {
    let Opt { port } = Opt::from_args();

    let addr = format!("0.0.0.0:{}", port);

    println!("Binding to {}...", addr);
    let socket = TcpListener::bind(addr).unwrap();

    println!("Listening on port {}", port);

    let (new_client_sender, new_client_receiver) = channel();
    let (input_sender, input_receiver) = channel();

    thread::spawn(|| listen(socket, input_sender, new_client_sender));

    let mut local_state = LocalState {
        rng: Rng::new(),
        collision_boxes: collision_boxes(),
        clients: HashMap::new(),
    };

    let mut game_state = GameState {
        players: HashMap::new(),
        bullets: vec![],
        events: vec![],
    };

    let ups = 120;
    let desired_delta = Duration::from_millis(1000 / ups);
    let mut last_tick = Instant::now();

    let mut left = vec![];

    loop {
        let start = Instant::now();

        // update game state
        let needs_respawn = game_state.update(&*local_state.collision_boxes,
                                              last_tick.elapsed().into_secs());
        last_tick = Instant::now();

        game_state.events.extend(left.drain(..));

        for id in needs_respawn {
            let pos = spawn_player(&mut local_state, &game_state);
            let p = game_state.players.get_mut(&id).unwrap();
            p.pos = pos;
            p.health = PLAYER_HEALTH;
            game_state.events.push(Event::PlayerRespawned(p.id));
        }

        // get inputs
        {
            for (input, id) in input_receiver.try_iter() {
                match input {
                    Input::Press(b, dir) => {
                        if let Some(client) = local_state.clients.get_mut(&id) {
                            if !client.buttons_down.contains_key(&b) {
                                if let Some(player) = game_state.players.get_mut(&id) {
                                    player.dir = dir;
                                    client.buttons_down.insert(b, Instant::now());

                                    if b == Button::LeftMouse {
                                        // spawn bullet
                                        game_state.events.push(Event::BulletFired(player.pos));
                                        game_state.bullets.push(Bullet::spawn(player));
                                    }
                                }
                            }
                        }
                    }

                    Input::Release(b) => {
                        if let Some(client) = local_state.clients.get_mut(&id) {
                            client.buttons_down.remove(&b);
                        }
                    }

                    Input::DirChanged(new_dir) => {
                        if let Some(player) = game_state.players.get_mut(&id) {
                            player.dir = new_dir;
                        }
                    }
                }
            }

            let move_force = 40.0;

            for (id, lps) in &local_state.clients {
                if let Some(gs_player) = game_state.players.get_mut(&id) {
                    gs_player.force = Vector::default();

                    if let Some(_t) = lps.buttons_down.get(&Button::A) {
                        gs_player.force.x -= move_force;
                    }
                    if let Some(_t) = lps.buttons_down.get(&Button::D) {
                        gs_player.force.x += move_force;
                    }
                    if let Some(_t) = lps.buttons_down.get(&Button::W) {
                        gs_player.force.y -= move_force;
                    }
                    if let Some(_t) = lps.buttons_down.get(&Button::S) {
                        gs_player.force.y += move_force;
                    }
                }

            }
        }

        // send new state to clients
        {
            // first establish any new clients
            for Client {
                    player_id,
                    player_name,
                    stream,
                } in new_client_receiver.try_iter() {

                let player_state = LocalPlayerState {
                    buttons_down: HashMap::new(),
                    stream,
                };

                local_state.clients.insert(player_id, player_state);

                let pos = spawn_player(&mut local_state, &game_state);

                let player = Player {
                    id: player_id,
                    name: player_name,
                    pos,
                    health: 100.0,
                    dir: VEC_RIGHT,
                    force: VEC_ZERO,
                    vel: VEC_ZERO,
                    respawn_timer: 0.0,
                    score: 0,
                };

                game_state.events.push(Event::PlayerJoined(player_id));
                game_state.players.insert(player_id, player);
            }

            let mut data: Vec<u8> = Vec::with_capacity(512);
            let msg = FromServerMsg::Update(game_state);

            bc::serialize_into(&mut data, &msg, bc::Infinite).unwrap();

            game_state = match msg {
                FromServerMsg::Update(gs) => gs,
                _ => unreachable!(),
            };

            let mut to_drop = vec![];

            for (&id, &mut LocalPlayerState { ref mut stream, .. }) in
                local_state.clients.iter_mut() {
                if let Err(e) = stream.write_all(&data[..]) {
                    println!("Game loop on client {}: ERROR: {}", id.0, e);
                    to_drop.push(id);
                }
            }

            for id in to_drop {
                println!("Dropping client {}", id.0);
                let player = game_state.players.remove(&id).unwrap();
                local_state.clients.remove(&id);
                left.push(Event::PlayerLeft(player.name));
            }
        }





        let delta = start.elapsed();
        desired_delta.checked_sub(delta).map(|d| thread::sleep(d));
    }
}

struct LocalState {
    rng: Rng,
    collision_boxes: Vec<CSquare>,
    clients: HashMap<PlayerId, LocalPlayerState>,
}

struct LocalPlayerState {
    buttons_down: HashMap<Button, Instant>,
    stream: TcpStream,
}

struct Client {
    player_id: PlayerId,
    player_name: String,
    stream: TcpStream,
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Client {
            player_id: self.player_id,
            player_name: self.player_name.clone(),
            stream: self.stream.try_clone().unwrap(),
        }
    }
}

fn listen(socket: TcpListener,
          input_sender: Sender<(Input, PlayerId)>,
          new_client_sender: Sender<Client>) {
    loop {
        let (mut client_stream, addr) = socket.accept().unwrap();
        println!("New client connected on {} from {}",
                 addr,
                 client_stream.peer_addr().unwrap());

        // send client a player id
        let id = next_player_id();
        let msg = FromServerMsg::Welcome(id);
        if let Err(e) = bc::serialize_into(&mut client_stream, &msg, bc::Infinite) {
            println!("Error welcoming client {}: {}", id.0, e);
            continue;
        }

        let name = match bc::deserialize_from(&mut client_stream, bc::Infinite) {
            Ok(ToServerMsg::Login(pid, name)) => {
                if pid != id {
                    println!("Wrong player id: {}", pid.0);
                    continue;
                }
                name
            }
            msg => {
                println!("Protocol error: expected Login({}), got {:?}", id.0, msg);
                continue;
            }
        };

        println!("Logging in new client: [{}] {}", id.0, name);

        let client = Client {
            player_id: id,
            player_name: name,
            stream: client_stream,
        };

        new_client_sender.send(client.clone()).unwrap();
        let input_sender = input_sender.clone();
        thread::spawn(move || pump_client(client, input_sender));
    }
}

fn pump_client(mut client: Client, sender: Sender<(Input, PlayerId)>) {
    println!("Pumping client {} for input", client.player_id.0);

    loop {
        match bc::deserialize_from(&mut client.stream, bc::Infinite) {
            Ok(msg) => {
                match msg {
                    ToServerMsg::Input(pid, input) => {
                        assert_eq!(pid, client.player_id);
                        if let Err(e) = sender.send((input, pid)) {
                            println!("Pump thread for client {}: Error: {}", pid.0, e);
                        }
                    }
                    _ => panic!(),
                }
            }
            Err(e) => {
                println!("Pump thread for client {}: Error: {}",
                         client.player_id.0,
                         e);
                break;
            }
        }

    }

    println!("Finished pumping client {}", client.player_id.0);
}

fn next_player_id() -> PlayerId {
    use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};

    static COUNTER: AtomicUsize = ATOMIC_USIZE_INIT;

    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    PlayerId(id as u32)
}

fn spawn_player(ls: &mut LocalState, gs: &GameState) -> Vector {
    let cmp_f = |a: f32, b: f32| a.partial_cmp(&b).unwrap_or(Ordering::Equal);

    let mut empty_tiles = vec![];

    for y in 0..LOGO_HEIGHT {
        for x in 0..LOGO_WIDTH {
            let row = &LOGO[y];
            let pixel = row.bytes().nth(x).unwrap();

            match pixel {
                b'b' | b'g' | b'i' => {}
                b'w' | b'f' => {
                    empty_tiles.push(Vector::new(x as f32 + 0.5, y as f32 + 0.5));
                }
                _ => unreachable!(),
            }
        }
    }

    if gs.players.is_empty() {
        // random tile
        return empty_tiles[ls.rng.rand_uint(0, empty_tiles.len() as u64) as usize];
    }

    // tile furthest away from any player
    let idx = empty_tiles
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, pos)| {
                 let shortest = gs.players
                     .values()
                     .map(|p| p.pos.dist(pos))
                     .min_by(|&a, &b| cmp_f(a, b))
                     .unwrap();
                 (i, shortest)
             })
        .max_by(|&(_, a), &(_, b)| cmp_f(a, b))
        .map(|(i, _)| i)
        .unwrap();

    empty_tiles[idx]
}
