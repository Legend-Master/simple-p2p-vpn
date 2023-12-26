use clap::Parser;
use shared::{receive, MacAddress, Message, ReceiveMessage, BROADCAST_MAC_ADDRESS};
use std::{
    collections::HashSet,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    sync::Mutex,
    thread::{self, sleep},
    time::{Duration, SystemTime},
};

const MASK: Ipv4Addr = Ipv4Addr::new(255, 255, 255, 0);
const SUBNET: Ipv4Addr = Ipv4Addr::new(10, 123, 123, 0);

struct Connection {
    ip: Ipv4Addr,
    mac_address: MacAddress,
    socket_address: SocketAddr,
    last_seen: SystemTime,
}

fn get_ip(ip_pool: &Mutex<HashSet<Ipv4Addr>>) -> Option<Ipv4Addr> {
    ip_pool.lock().unwrap().iter().next().cloned()
}

/// A simple peer to peer VPN client
#[derive(Parser, Debug)]
struct Cli {
    /// Server ip adrress like localhost:8000
    #[arg(
        // short,
        // long,
        // env,
        value_name = "port",
        help = "Listening port",
    )]
    port: u16,
}

fn main() {
    let config = Cli::parse();

    let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), config.port))
        .expect("couldn't bind to address");
    println!("Server listening at 0.0.0.0:{}", config.port);

    let mut ip_pool: HashSet<Ipv4Addr> = HashSet::new();
    for i in 0..=255 {
        let mut octets = SUBNET.octets();
        *octets.last_mut().unwrap() = i;
        ip_pool.insert(octets.into());
    }
    let ip_pool: Mutex<HashSet<Ipv4Addr>> = Mutex::new(ip_pool);
    let connections: Mutex<Vec<Connection>> = Mutex::new(Vec::new());

    thread::scope(|scope| {
        scope.spawn(|| loop {
            sleep(Duration::from_secs(100));
            connections.lock().unwrap().retain(|connection| {
                let should_keep =
                    connection.last_seen.elapsed().unwrap() < Duration::from_secs(200);
                if !should_keep {
                    // Release ip from peer
                    ip_pool.lock().unwrap().insert(connection.ip);
                }
                return should_keep;
            });
        });

        scope.spawn(|| {
            loop {
                let ReceiveMessage {
                    message,
                    source_address,
                } = receive(&socket);
                match message {
                    Message::Register { mac_address } => {
                        println!("In comming client from {}", source_address);
                        if let Some(ip) = get_ip(&ip_pool) {
                            socket
                                .send_to(
                                    &bincode::serialize(&Message::RegisterSuccess {
                                        ip,
                                        mask: MASK,
                                    })
                                    .unwrap(),
                                    source_address,
                                )
                                .expect("can't send back response");
                            connections.lock().unwrap().push(Connection {
                                ip,
                                mac_address,
                                socket_address: source_address,
                                last_seen: SystemTime::now(),
                            });
                            ip_pool.lock().unwrap().remove(&ip);
                        } else {
                            socket
                                .send_to(
                                    &bincode::serialize(&Message::RegisterFail {
                                        reason: "No avalible ip left".to_owned(),
                                    })
                                    .unwrap(),
                                    source_address,
                                )
                                .expect("can't send back response");
                        }
                    }
                    Message::Data {
                        payload,
                        destination_mac_address,
                        source_mac_address,
                    } => {
                        for connection in connections.lock().unwrap().iter() {
                            if destination_mac_address == BROADCAST_MAC_ADDRESS
                                || connection.mac_address == destination_mac_address
                            {
                                socket
                                    .send_to(
                                        &bincode::serialize(&Message::Data {
                                            destination_mac_address: source_mac_address,
                                            source_mac_address: destination_mac_address,
                                            payload: payload.clone(),
                                        })
                                        .unwrap(),
                                        connection.socket_address,
                                    )
                                    .expect("can't send back response");
                            }
                        }
                        // dbg!(&payload);
                    }
                    Message::Ping => {
                        for connection in connections.lock().unwrap().iter_mut() {
                            if connection.socket_address == source_address {
                                connection.last_seen = SystemTime::now();
                                break;
                            }
                        }
                    }
                    // Ignore invalid pakcets
                    others => {
                        dbg!(others);
                    }
                }
            }
        });
    });
}
