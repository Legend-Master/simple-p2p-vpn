use argh::FromArgs;
use macaddr::MacAddr6;
use shared::{get_mac_addresses, receive_until_success, send_to, Message, ReceiveMessage};
use socket2::{Domain, Socket, Type};
use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    sync::Mutex,
    thread::{self, sleep},
    time::{Duration, SystemTime},
};

const SUBNET: Ipv4Addr = Ipv4Addr::new(10, 123, 123, 0);
const SUBNET_MASK: Ipv4Addr = Ipv4Addr::new(255, 255, 255, 0);

struct Connection {
    ip: Ipv4Addr,
    mac_address: MacAddr6,
    socket_address: SocketAddr,
    last_seen: SystemTime,
}

/// A simple peer to peer VPN client
#[derive(FromArgs)]
struct Cli {
    /// listening port
    #[argh(positional)]
    port: u16,
}

fn main() {
    let config: Cli = argh::from_env();

    let ip_pool: Mutex<HashSet<Ipv4Addr>> = Mutex::new(generate_ip_pool());
    let connections: Mutex<HashMap<MacAddr6, Connection>> = Mutex::new(HashMap::new());

    thread::scope(|scope| {
        scope.spawn(|| {
            let socket = Socket::new(Domain::IPV6, Type::DGRAM, None).expect("Can't create socket");
            socket
                .set_only_v6(false)
                .expect("Can't set socket to receive packets from an IPv4-mapped IPv6 address");

            let address = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), config.port);
            socket
                .bind(&address.into())
                .expect(&format!("Can't bind to address {}", address));
            let socket: UdpSocket = socket.into();

            println!("Server listening at [::]:{}", config.port);

            loop {
                handle_message(&socket, &connections, &ip_pool);
            }
        });

        // Purge timed out connections
        scope.spawn(|| loop {
            sleep(Duration::from_secs(100));
            purge_timedout_connections(&connections, &ip_pool);
        });
    });
}

fn generate_ip_pool() -> HashSet<Ipv4Addr> {
    let mut ip_pool: HashSet<Ipv4Addr> = HashSet::new();
    for i in 0..=255 {
        let mut octets = SUBNET.octets();
        *octets.last_mut().unwrap() = i;
        ip_pool.insert(octets.into());
    }
    ip_pool
}

fn get_ip(ip_pool: &Mutex<HashSet<Ipv4Addr>>) -> Option<Ipv4Addr> {
    ip_pool.lock().unwrap().iter().next().cloned()
}

// Reassign ip if it's a reconnection
fn reassign_ip(
    connections: &Mutex<HashMap<MacAddr6, Connection>>,
    mac_address: MacAddr6,
    source_address: SocketAddr,
    socket: &UdpSocket,
) -> bool {
    let mut connections = connections.lock().unwrap();
    match connections.get_mut(&mac_address) {
        Some(connection) => {
            connection.socket_address = source_address;
            connection.last_seen = SystemTime::now();
            send_to(
                socket,
                &Message::RegisterSuccess {
                    ip: connection.ip,
                    subnet_mask: SUBNET_MASK,
                },
                &source_address,
            );
            println!("Reassign IP {} to {}", &connection.ip, &source_address);
            true
        }
        None => false,
    }
}

fn register(
    mac_address: MacAddr6,
    source_address: SocketAddr,
    connections: &Mutex<HashMap<MacAddr6, Connection>>,
    socket: &UdpSocket,
    ip_pool: &Mutex<HashSet<Ipv4Addr>>,
) {
    println!("Incomming client {} from {}", &mac_address, &source_address);

    if reassign_ip(connections, mac_address, source_address, socket) {
        return;
    }

    if let Some(ip) = get_ip(ip_pool) {
        println!("Assign IP {} to {}", &ip, &source_address);
        send_to(
            socket,
            &Message::RegisterSuccess {
                ip,
                subnet_mask: SUBNET_MASK,
            },
            &source_address,
        );
        connections.lock().unwrap().insert(
            mac_address,
            Connection {
                ip,
                mac_address,
                socket_address: source_address,
                last_seen: SystemTime::now(),
            },
        );
        ip_pool.lock().unwrap().remove(&ip);
    } else {
        send_to(
            socket,
            &Message::RegisterFail {
                reason: "No avalible ip left".to_owned(),
            },
            &source_address,
        );
    }
}

fn handle_message(
    socket: &UdpSocket,
    connections: &Mutex<HashMap<MacAddr6, Connection>>,
    ip_pool: &Mutex<HashSet<Ipv4Addr>>,
) {
    let ReceiveMessage {
        message,
        source_address,
    } = receive_until_success(&socket);
    match message {
        Message::Register { mac_address } => {
            register(mac_address, source_address, connections, socket, ip_pool);
        }
        Message::Data { ethernet_frame } => {
            forward_data(ethernet_frame, socket, connections);
            // dbg!(&ethernet_frame);
        }
        Message::Ping => {
            // println!("ping from {}", &source_address);
            if let Some((_, connection)) = connections
                .lock()
                .unwrap()
                .iter_mut()
                .find(|(_, connection)| connection.socket_address == source_address)
            {
                connection.last_seen = SystemTime::now();
                send_to(socket, &Message::Pong, &connection.socket_address);
            }
        }
        // Ignore invalid pakcets
        others => {
            dbg!(others);
        }
    }
}

fn forward_data(
    ethernet_frame: Vec<u8>,
    socket: &UdpSocket,
    connections: &Mutex<HashMap<MacAddr6, Connection>>,
) {
    if let Ok((source_mac_address, destination_mac_address)) = get_mac_addresses(&ethernet_frame) {
        let send = |connection: &Connection| {
            // println!(
            //     "forwarding to {} ({})",
            //     &connection.socket_address, &connection.ip
            // );
            send_to(
                socket,
                &Message::Data {
                    ethernet_frame: ethernet_frame.clone(),
                },
                &connection.socket_address,
            );
        };
        // Broadcast is a special type of multicast
        if destination_mac_address.is_multicast() {
            for (_, connection) in connections.lock().unwrap().iter() {
                if connection.mac_address != source_mac_address {
                    send(connection);
                }
            }
        } else {
            if let Some(connection) = connections.lock().unwrap().get(&destination_mac_address) {
                send(connection);
            }
        }
    }
}

fn purge_timedout_connections(
    connections: &Mutex<HashMap<MacAddr6, Connection>>,
    ip_pool: &Mutex<HashSet<Ipv4Addr>>,
) {
    connections.lock().unwrap().retain(|_, connection| {
        let should_keep = connection.last_seen.elapsed().unwrap() < Duration::from_secs(200);
        if !should_keep {
            // Release ip from peer
            ip_pool.lock().unwrap().insert(connection.ip);
            println!(
                "purged {} from {}",
                connection.ip, connection.socket_address
            );
        }
        return should_keep;
    });
}
