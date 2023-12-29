use argh::FromArgs;
use macaddr::MacAddr6;
use shared::{get_mac_addresses, receive_until_success, send, Message};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender};
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
        .expect("Failed to open TAP");
    tap_device.up().expect("Failed to turn on TAP");
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

fn resolve_host(hostname_port: &str) -> Result<SocketAddrV4, String> {
    match hostname_port.to_socket_addrs() {
        Ok(socket_addresses) => {
            let mut has_ipv6_address = false;
            for address in socket_addresses {
                match address {
                    SocketAddr::V4(address) => {
                        return Ok(address);
                    }
                    SocketAddr::V6(_) => has_ipv6_address = true,
                }
            }
            if has_ipv6_address {
                Err("IPv6 is not support yet".to_owned())
            } else {
                Err(format!("Could not find destination {hostname_port}"))
            }
        }
        Err(error) => Err(error.to_string()),
    }
}

/// A simple peer to peer VPN client
#[derive(FromArgs)]
struct Cli {
    /// server ip adrress like localhost:8000
    #[argh(positional, from_str_fn(resolve_host))]
    server: SocketAddrV4,
}

fn main() {
    let config: Cli = argh::from_env();

    println!("Starting up TAP device");
    let tap_device = &setup_tap();
    println!("TAP device started");

    println!("Connecting to server {}", config.server);
    let socket = &setup_socket(&config.server);

    let (register_sender, register_receiver) = mpsc::channel();
    let (pong_sender, pong_receiver) = mpsc::channel();

    thread::scope(|scope| {
        scope.spawn(move || loop {
            handle_message(socket, tap_device, &register_sender, &pong_sender);
        });

        if let Err(error) = register(socket, tap_device, &register_receiver) {
            panic!(
                "Re-register failed: {}",
                match error {
                    Some(reason) => reason,
                    None => "unkown reason".to_string(),
                }
            );
        }

        scope.spawn(|| read_and_send(tap_device, socket));

        scope.spawn(move || loop {
            sleep(Duration::from_secs(5));
            ping(socket, tap_device, &register_receiver, &pong_receiver);
        });
    });
}

enum RegisterResult {
    Success { ip: Ipv4Addr, subnet_mask: Ipv4Addr },
    Fail { reason: String },
}

fn handle_message(
    socket: &UdpSocket,
    tap_device: &Device,
    register_sender: &Sender<RegisterResult>,
    pong_sender: &Sender<()>,
) {
    match receive_until_success(socket).message {
        Message::Data { ethernet_frame } => {
            // println!("received data packet");
            // let time = SystemTime::now();
            match tap_device.write_non_mut(&ethernet_frame) {
                Ok(bytes_written) => {
                    if bytes_written < ethernet_frame.len() {
                        println!(
                            "{} bytes recieved but only {} bytes written to TAP",
                            bytes_written,
                            ethernet_frame.len()
                        );
                    }
                }
                Err(error) => {
                    println!("Can't write to TAP with error: {}", error);
                }
            }
            // println!(
            //     "wrote {} bytes to TAP device in {:?}",
            //     ethernet_frame.len(),
            //     time.elapsed().unwrap()
            // );
        }
        Message::RegisterSuccess { ip, subnet_mask } => {
            register_sender
                .send(RegisterResult::Success { ip, subnet_mask })
                .unwrap();
        }
        Message::RegisterFail { reason } => {
            register_sender
                .send(RegisterResult::Fail { reason })
                .unwrap();
        }
        Message::Pong => {
            pong_sender.send(()).unwrap();
        }
        // Ignore invalid pakcets
        _ => {}
    }
}

fn read_and_send(tap_device: &Device, socket: &UdpSocket) -> ! {
    let mac_address = MacAddr6::from(tap_device.get_mac().unwrap());
    let mtu = tap_device.get_mtu().unwrap_or(1500);
    let mut buffer = vec![0; mtu as usize];
    loop {
        match tap_device.read_non_mut(&mut buffer) {
            Ok(bytes_read) => {
                let ethernet_frame = &buffer[..bytes_read];
                match get_mac_addresses(ethernet_frame) {
                    Ok((source_mac_address, _)) => {
                        if source_mac_address != mac_address {
                            println!("not device source mac? {}", &source_mac_address);
                            continue;
                        };
                        // println!(
                        //     "TAP packet ({} bytes) received (source: {}, dest: {})",
                        //     bytes_read, &source_mac_address, &destination_mac_address
                        // );
                        send(
                            socket,
                            &Message::Data {
                                ethernet_frame: ethernet_frame.to_vec(),
                            },
                        );
                    }
                    Err(_) => {
                        // Invalid packet
                        println!("only {} bytes read from TAP, ignoring", &bytes_read);
                        continue;
                    }
                }
            }
            Err(error) => {
                println!("Can't read from TAP: {}", error);
                continue;
            }
        }
    }
}

fn register(
    socket: &UdpSocket,
    tap_device: &Device,
    register_receiver: &Receiver<RegisterResult>,
) -> Result<(), Option<String>> {
    let mac_address = MacAddr6::from(tap_device.get_mac().unwrap());
    // Retry register for 15 seconds
    while SystemTime::now().elapsed().unwrap() < Duration::from_secs(15) {
        send(socket, &Message::Register { mac_address });
        match register_receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(result) => match result {
                RegisterResult::Success { ip, subnet_mask } => {
                    tap_device
                        .set_ip(ip, subnet_mask)
                        .expect("Failed to set device ip");
                    println!("Connected, assign ip {}", ip);
                    return Ok(());
                }
                RegisterResult::Fail { reason } => {
                    return Err(Some(reason));
                }
            },
            Err(_) => continue,
        };
    }
    return Err(Some("Timeout".to_owned()));
}

fn ping(
    socket: &UdpSocket,
    tap_device: &Device,
    register_receiver: &Receiver<RegisterResult>,
    pong_receiver: &Receiver<()>,
) {
    // Retry ping for 15 seconds
    while SystemTime::now().elapsed().unwrap() < Duration::from_secs(15) {
        send(socket, &Message::Ping);
        clear_receiver(pong_receiver);
        if let Ok(_) = pong_receiver.recv_timeout(Duration::from_secs(5)) {
            // Pong received
            // println!("Pong received");
            return;
        }
    }
    // If didn't get a pong then we probably lost connection to server
    // try re-register
    println!("Lost connection to server, trying to re-register");
    if let Err(error) = register(socket, tap_device, register_receiver) {
        // println!(
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
