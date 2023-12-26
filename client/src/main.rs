use shared::{receive, MacAddress, Message, ReceiveMessage};
use std::io::{Read, Write};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Duration;
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

fn setup_socket() -> UdpSocket {
    let socket = UdpSocket::bind("0.0.0.0:0").expect("couldn't bind to address");
    socket
        .connect("localhost:8000")
        .expect("couldn't connect to address");
    return socket;
}

fn main() {
    let tap_device = Mutex::new(setup_tap());
    println!("TAP device started");
    let socket = setup_socket();

    println!("Connecting to server...");
    socket
        .send(
            &bincode::serialize(&Message::Register {
                mac_address: tap_device.lock().unwrap().get_mac().unwrap(),
            })
            .unwrap(),
        )
        .unwrap();

    let ReceiveMessage {
        message,
        source_address: _,
    } = receive(&socket);
    match message {
        Message::RegisterSuccess { ip, mask } => {
            // Set the device ip
            tap_device
                .lock()
                .unwrap()
                .set_ip(ip, mask)
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

                let data = Message::Data {
                    source_mac_address,
                    destination_mac_address,
                    payload: (&buf[..bytes_read]).to_vec(),
                };
                // dbg!(&data);
                socket.send(&bincode::serialize(&data).unwrap()).unwrap();
            }
        });

        scope.spawn(|| loop {
            sleep(Duration::from_secs(10));
            socket
                .send(&bincode::serialize(&Message::Ping).unwrap())
                .unwrap();
        });

        scope.spawn(|| loop {
            let ReceiveMessage {
                message,
                source_address: _,
            } = receive(&socket);

            match message {
                Message::Data {
                    payload,
                    destination_mac_address: _,
                    source_mac_address: _,
                } => {
                    tap_device.lock().unwrap().write(&payload).unwrap();
                }
                // Ignore invalid pakcets
                _ => {}
            }
        });
    });
}
