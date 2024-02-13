mod tap_device;

use argh::FromArgs;
use shared::{
    get_formatted_time, get_mac_addresses, log, receive_until_success, send,
    setup_panic_logging_hook, Message,
};
use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::{net::UdpSocket, thread};
use tap_device::{setup_tap, Device, TapDevice};

fn setup_socket(server: &SocketAddr) -> UdpSocket {
    let bind_address = match server {
        SocketAddr::V4(_) => "0.0.0.0:0",
        SocketAddr::V6(_) => "[::]:0",
    };
    let socket = UdpSocket::bind(bind_address).expect("Can't bind to address");
    socket.connect(server).expect("Can't connect to address");
    socket
}

fn resolve_host(hostname_port: &str) -> Result<SocketAddr, String> {
    match hostname_port.to_socket_addrs() {
        Ok(mut socket_addresses) => Ok(socket_addresses.next().unwrap()),
        Err(error) => Err(error.to_string()),
    }
}

/// A simple peer to peer VPN client
#[derive(FromArgs)]
struct Cli {
    /// server ip adrress like localhost:8000
    #[argh(positional, from_str_fn(resolve_host))]
    server: SocketAddr,
}

fn main() {
    let config: Cli = argh::from_env();

    setup_panic_logging_hook();

    log!("Starting up TAP device");
    let tap_device = &setup_tap();
    log!("TAP device started");

    log!("Connecting to server {}", config.server);
    let socket = &setup_socket(&config.server);

    let (register_sender, register_receiver) = mpsc::channel();
    let (pong_sender, pong_receiver) = mpsc::channel();

    thread::scope(|scope| {
        scope.spawn(move || loop {
            handle_message(socket, tap_device, &register_sender, &pong_sender);
        });

        if let Err(reason) = register(socket, tap_device, &register_receiver) {
            panic!("Register failed: {reason}");
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
            // log!("received data packet");
            // let time = Instant::now();
            match tap_device.write_non_mut(&ethernet_frame) {
                Ok(bytes_written) => {
                    if bytes_written < ethernet_frame.len() {
                        log!(
                            "{bytes_written} bytes recieved but only {} bytes written to TAP",
                            ethernet_frame.len()
                        );
                    }
                }
                Err(error) => {
                    log!("Can't write to TAP with error: {error}");
                }
            }
            // log!(
            //     "wrote {} bytes to TAP device in {:?}",
            //     ethernet_frame.len(),
            //     time.elapsed()
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
    let mac_address = tap_device.get_mac().expect("Can't get TAP MAC address");
    let mtu = tap_device.get_mtu().unwrap_or(1500);
    let mut buffer = vec![0; mtu as usize];
    loop {
        match tap_device.read_non_mut(&mut buffer) {
            Ok(bytes_read) => {
                let ethernet_frame = &buffer[..bytes_read];
                match get_mac_addresses(ethernet_frame) {
                    Ok((source_mac_address, _)) => {
                        if source_mac_address != mac_address {
                            log!("Not device source mac? {source_mac_address}");
                            continue;
                        };
                        // log!(
                        //     "TAP packet ({bytes_read} bytes) received (source: {source_mac_address}, dest: {destination_mac_address})"
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
                        log!("Only {bytes_read} bytes read from TAP, ignoring");
                        continue;
                    }
                }
            }
            Err(error) => {
                log!("Can't read from TAP: {error}");
                continue;
            }
        }
    }
}

fn register(
    socket: &UdpSocket,
    tap_device: &Device,
    register_receiver: &Receiver<RegisterResult>,
) -> Result<(), String> {
    let mac_address = tap_device.get_mac().unwrap();
    // Retry register for 15 seconds
    let start_time = Instant::now();
    while start_time.elapsed() < Duration::from_secs(15) {
        send(socket, &Message::Register { mac_address });
        clear_receiver(register_receiver);
        if let Ok(result) = register_receiver.recv_timeout(Duration::from_secs(5)) {
            match result {
                RegisterResult::Success { ip, subnet_mask } => {
                    log!("Connected, server gave us {ip}, setting it to TAP");
                    tap_device
                        .set_ip(ip, subnet_mask)
                        .expect("Failed to set TAP IP");
                    log!("Set TAP IP to {ip} successfully");
                    return Ok(());
                }
                RegisterResult::Fail { reason } => {
                    return Err(reason);
                }
            }
        }
    }
    Err("Timeout".to_owned())
}

fn ping(
    socket: &UdpSocket,
    tap_device: &Device,
    register_receiver: &Receiver<RegisterResult>,
    pong_receiver: &Receiver<()>,
) {
    // Retry ping for 15 seconds
    let start_time = Instant::now();
    while start_time.elapsed() < Duration::from_secs(15) {
        send(socket, &Message::Ping);
        clear_receiver(pong_receiver);
        if pong_receiver.recv_timeout(Duration::from_secs(5)).is_ok() {
            // Pong received
            // log!("Pong received");
            return;
        }
    }
    // If didn't get a pong then we probably lost connection to server
    // try re-register
    log!("Lost connection to server, trying to re-register");
    if let Err(reason) = register(socket, tap_device, register_receiver) {
        // log!("Re-register failed: {reason}");
        panic!("Re-register failed: {reason}");
    }
    log!("Re-register success");
}

fn clear_receiver<T>(receiver: &Receiver<T>) {
    while receiver.try_recv().is_ok() {}
}
