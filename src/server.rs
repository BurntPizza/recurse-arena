
extern crate recurse_arena;
extern crate bincode as bc;
extern crate ludomath;

use recurse_arena::*;

use std::io::prelude::*;
use std::thread;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::net::{TcpListener, TcpStream};
use std::time::{Instant, Duration};

fn main() {
    let port = 8000;
    let socket = TcpListener::bind(format!("0.0.0.0:{}", port)).unwrap();

    println!("Listening on port {}", port);

    let (new_client_sender, new_client_receiver) = channel();
    let (input_sender, input_receiver) = channel();

    thread::spawn(|| listen(socket, input_sender, new_client_sender));

    let mut state = State { clients: HashMap::new() };

    let mut game_state = GameState {
        players: HashMap::new(),
        bullets: vec![],
    };

    let ups = 10;
    let desired_delta = Duration::from_millis(1000 / ups);

    loop {
        let start = Instant::now();

        // send new state to clients
        {
            for Client {
                    player_id,
                    player_name,
                    stream,
                } in new_client_receiver.try_iter() {

                let player_state = LocalPlayerState {
                    buttons_down: HashMap::new(),
                    stream,
                };

                state.clients.insert(player_id, player_state);

                let player = Player {
                    id: player_id,
                    name: player_name,
                    ..Default::default()
                };

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

            for (&id, &mut LocalPlayerState { ref mut stream, .. }) in state.clients.iter_mut() {
                if let Err(e) = stream.write_all(&data[..]) {
                    println!("Game loop on client {}: ERROR: {}", id.0, e);
                    to_drop.push(id);
                }
            }

            for id in to_drop {
                println!("Dropping client {}", id.0);
                state.clients.remove(&id);
            }
        }

        // get inputs
        {
            for (input, id) in input_receiver.try_iter() {
                match input {
                    Input::Press(b, dir) => {
                        if let Some(client) = state.clients.get_mut(&id) {
                            if !client.buttons_down.contains_key(&b) {
                                game_state.players.get_mut(&id).map(|p| p.dir = dir);
                                client.buttons_down.insert(b, Instant::now());
                            }
                        }
                    }
                    Input::Release(b) => {
                        if let Some(client) = state.clients.get_mut(&id) {
                            client.buttons_down.remove(&b);
                        }
                    }
                }
            }
        }

        // update state
        game_state.update();


        let delta = start.elapsed();
        desired_delta.checked_sub(delta).map(|d| thread::sleep(d));
    }
}

struct State {
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
                        sender.send((input, pid)).unwrap();
                    }
                    _ => panic!(),
                }
            }
            Err(e) => {
                println!("Pump thread for client {}: ERROR: {}",
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
