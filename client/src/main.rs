use clap::Parser;
use shared::{
    is_multicast, receive_until_success, send, MacAddress, Message, ReceiveMessage,
    MINIMUM_ETHERNET_FRAME_BYTES,
};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, SocketAddrV4, ToSocketAddrs};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use std::{net::UdpSocket, thread};
use tap_windows::{Device, HARDWARE_ID};

#[cfg(target_os = "windows")]
fn setup_tap() -> Device {
    const INTERFACE_NAME: &str = "Simple Peer To Peer";
    // Try to open the device
    let tap_device = Device::open(HARDWARE_ID, INTERFACE_NAME)
        .or_else(|_| -> std::io::Result<_> {
            // The device does not exists...
            // try creating a new one

            let dev = Device::create(HARDWARE_ID)?;
            dev.set_name(INTERFACE_NAME)?;

            Ok(dev)
        })
        // Everything failed, just panic
        .expect("Failed to open device");
    tap_device.up().expect("Failed to turn on device");
    return tap_device;
}

fn setup_socket(server: &SocketAddrV4) -> UdpSocket {
    // let bind_address = match server {
    //     SocketAddr::V4(_) => "0.0.0.0:0",
    //     SocketAddr::V6(_) => "[::]:0",
    // };
    let bind_address = "0.0.0.0:0";
    let socket = UdpSocket::bind(bind_address).expect("couldn't bind to address");
    socket.connect(server).expect("couldn't connect to address");
    return socket;
}

// https://stackoverflow.com/a/77047863/16993372
fn resolve_host(hostname_port: &str) -> io::Result<SocketAddrV4> {
    for socketaddr in hostname_port.to_socket_addrs()? {
        match socketaddr {
            SocketAddr::V4(address) => {
                return Ok(address);
            }
            SocketAddr::V6(_) => {}
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AddrNotAvailable,
        format!("Could not find destination {hostname_port}"),
    ))
}

/// A simple peer to peer VPN client
#[derive(Parser, Debug)]
struct Cli {
    #[arg(
        // short,
        // long,
        // env,
        value_name = "server",
        help = "Server ip adrress like localhost:8000",
        value_parser = resolve_host,
    )]
    server: SocketAddrV4,
}

fn main() {
    let config = Cli::parse();

    println!("Starting up TAP device");
    let tap_device = Mutex::new(setup_tap());
    println!("TAP device started");

    println!("Connecting to server {}", config.server);
    let socket = setup_socket(&config.server);

    send(
        &socket,
        &Message::Register {
            mac_address: tap_device.lock().unwrap().get_mac().unwrap(),
        },
    );

    let ReceiveMessage {
        message,
        source_address: _,
    } = receive_until_success(&socket);
    match message {
        Message::RegisterSuccess { ip, subnet_mask } => {
            // Set the device ip
            tap_device
                .lock()
                .unwrap()
                .set_ip(ip, subnet_mask)
                .expect("Failed to set device ip");
            println!("Connected, assign ip {}", ip);
        }
        Message::RegisterFail { reason } => {
            panic!("{}", reason);
        }
        _ => unreachable!(),
    }

    thread::scope(|scope| {
        scope.spawn(|| {
            let mtu = tap_device.lock().unwrap().get_mtu().unwrap_or(1500);
            let mut buf = vec![0; mtu as usize];
            loop {
                let bytes_read = tap_device
                    .lock()
                    .unwrap()
                    .read(&mut buf)
                    .expect("Failed to read packet");
                // Invalid packet
                if bytes_read < 12 {
                    continue;
                }
                // Ethernet header
                let destination_mac_address: MacAddress = buf[0..=5].try_into().unwrap();
                let source_mac_address: MacAddress = buf[6..=11].try_into().unwrap();

                println!(
                    "TAP packet ({} bytes) received (source: {:?}, dest: {:?})",
                    bytes_read, &source_mac_address, &destination_mac_address
                );
                send(
                    &socket,
                    &Message::Data {
                        source_mac_address,
                        destination_mac_address,
                        payload: (&buf[..bytes_read]).to_vec(),
                    },
                );
            }
        });

        scope.spawn(|| loop {
            sleep(Duration::from_secs(10));
            send(&socket, &Message::Ping);
        });

        scope.spawn(|| loop {
            let ReceiveMessage {
                message,
                source_address: _,
            } = receive_until_success(&socket);

            match message {
                Message::Data {
                    mut payload,
                    destination_mac_address,
                    source_mac_address,
                } => {
                    println!("received data packet");
                    if !is_multicast(&destination_mac_address) {
                        dbg!((&source_mac_address, &destination_mac_address));
                        dbg!(&payload);
                    }
                    let time = SystemTime::now();
                    if payload.len() < MINIMUM_ETHERNET_FRAME_BYTES.into() {
                        payload.resize(MINIMUM_ETHERNET_FRAME_BYTES.into(), 0);
                    }
                    tap_device.lock().unwrap().write_all(&payload).unwrap();
                    println!(
                        "wrote {} bytes to TAP device in {:?}",
                        payload.len(),
                        time.elapsed().unwrap()
                    );
                }
                // Ignore invalid pakcets
                _ => {}
            }
        });
    });
}
