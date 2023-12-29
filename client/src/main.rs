use clap::Parser;
use macaddr::MacAddr6;
use shared::{receive_until_success, send, Message, ReceiveMessage};
use std::io;
use std::net::{SocketAddr, SocketAddrV4, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver};
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
    let tap_device = &setup_tap();
    println!("TAP device started");

    println!("Connecting to server {}", config.server);
    let socket = &setup_socket(&config.server);

    register(&socket, &tap_device).unwrap();

    let (pong_sender, pong_receiver) = mpsc::channel();

    thread::scope(|scope| {
        scope.spawn(|| {
            let mac_address = MacAddr6::from(tap_device.get_mac().unwrap());
            let mtu = tap_device.get_mtu().unwrap_or(1500);
            let mut buffer = vec![0; mtu as usize];
            loop {
                match tap_device.read_non_mut(&mut buffer) {
                    Ok(bytes_read) => {
                        // Invalid packet
                        if bytes_read < 12 {
                            println!("only {} bytes read from TAP, ignoring", &bytes_read);
                            continue;
                        }
                        // Ethernet header
                        let destination_mac_address: [u8; 6] = buffer[0..=5].try_into().unwrap();
                        let source_mac_address: [u8; 6] = buffer[6..=11].try_into().unwrap();
                        let destination_mac_address: MacAddr6 = destination_mac_address.into();
                        let source_mac_address: MacAddr6 = source_mac_address.into();
                        if source_mac_address != mac_address {
                            println!("not device source mac? {:x?}", &source_mac_address);
                            continue;
                        };

                        // println!(
                        //     "TAP packet ({} bytes) received (source: {}, dest: {})",
                        //     bytes_read, &source_mac_address, &destination_mac_address
                        // );
                        send(
                            &socket,
                            &Message::Data {
                                source_mac_address,
                                destination_mac_address,
                                payload: (&buffer[..bytes_read]).to_vec(),
                            },
                        );
                    }
                    Err(error) => {
                        println!("Can't read from TAP: {}", error);
                        continue;
                    }
                }
            }
        });

        scope.spawn(move || {
            loop {
                let ReceiveMessage {
                    message,
                    source_address: _,
                } = receive_until_success(&socket);

                match message {
                    Message::Data {
                        payload,
                        destination_mac_address: _,
                        source_mac_address: _,
                    } => {
                        // println!("received data packet");
                        // if !destination_mac_address.is_multicast() {
                        //     println!(
                        //         "source: {}, dest: {}",
                        //         &source_mac_address, &destination_mac_address
                        //     );
                        // }
                        // let time = SystemTime::now();
                        let len = tap_device.write_non_mut(&payload).unwrap();
                        if len < payload.len() {
                            println!(
                                "{} bytes recieved but only {} bytes written to TAP",
                                len,
                                payload.len()
                            );
                        }
                        // println!(
                        //     "wrote {} bytes to TAP device in {:?}",
                        //     payload.len(),
                        //     time.elapsed().unwrap()
                        // );
                    }
                    Message::Pong => {
                        pong_sender.send(()).unwrap();
                    }
                    // Ignore invalid pakcets
                    _ => {}
                }
            }
        });

        scope.spawn(move || loop {
            sleep(Duration::from_secs(5));
            ping(&socket, &tap_device, &pong_receiver);
        });
    });
}

fn register(socket: &UdpSocket, tap_device: &Device) -> Result<(), Option<String>> {
    send(
        socket,
        &Message::Register {
            mac_address: tap_device.get_mac().unwrap().try_into().unwrap(),
        },
    );
    let ReceiveMessage {
        message,
        source_address: _,
    } = receive_until_success(socket);
    match message {
        Message::RegisterSuccess { ip, subnet_mask } => {
            // Set the device ip
            tap_device
                .set_ip(ip, subnet_mask)
                .expect("Failed to set device ip");
            println!("Connected, assign ip {}", ip);
            Ok(())
        }
        Message::RegisterFail { reason } => Err(Some(reason)),
        _ => Err(None),
    }
}

fn ping(socket: &UdpSocket, tap_device: &Device, pong_receiver: &Receiver<()>) {
    // Retry ping for 15 seconds
    while SystemTime::now().elapsed().unwrap() < Duration::from_secs(15) {
        send(socket, &Message::Ping);
        clear_receiver(&pong_receiver);
        if let Ok(_) = pong_receiver.recv_timeout(Duration::from_secs(5)) {
            // Pong received
            println!("Pong received");
            return;
        }
    }
    // If didn't get a pong then we probably lost connection to server
    // try re-register
    println!("Lost connection to server, trying to re-register");
    if let Err(error) = register(&socket, &tap_device) {
        // println!(
        //     "Re-register failed: {}",
        //     match error {
        //         Some(reason) => reason,
        //         None => "unkown reason".to_string(),
        //     }
        // );
        panic!(
            "Re-register failed: {}",
            match error {
                Some(reason) => reason,
                None => "unkown reason".to_string(),
            }
        );
    }
    println!("Re-register success");
}

fn clear_receiver<T>(receiver: &Receiver<T>) {
    while let Ok(_) = receiver.try_recv() {}
}
