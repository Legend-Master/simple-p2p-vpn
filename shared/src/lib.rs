use chrono::Local;
use macaddr::MacAddr6;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Register { mac_address: MacAddr6 },
    RegisterSuccess { ip: Ipv4Addr, subnet_mask: Ipv4Addr },
    RegisterFail { reason: String },
    Ping,
    Pong,
    Data { ethernet_frame: Vec<u8> },
}

pub fn get_mac_addresses(ethernet_frame: &[u8]) -> Result<(MacAddr6, MacAddr6), ()> {
    if ethernet_frame.len() < 12 {
        return Err(());
    }
    // Ethernet header
    let destination_mac_address: [u8; 6] = ethernet_frame[0..=5].try_into().unwrap();
    let source_mac_address: [u8; 6] = ethernet_frame[6..=11].try_into().unwrap();
    Ok((source_mac_address.into(), destination_mac_address.into()))
}

pub fn send(socket: &UdpSocket, message: &Message) {
    let payload = &bincode::serialize(message).unwrap();
    let mut bytes_written = 0;
    while bytes_written < payload.len() {
        bytes_written += socket.send(payload).unwrap();
    }
    // let bytes_written = socket.send(payload).unwrap();
    // if bytes_written < payload.len() {
    //     log!(
    //         "should send {} bytes but only {bytes_written} bytes sent",
    //         payload.len()
    //     );
    // }
}

pub fn send_to(socket: &UdpSocket, message: &Message, to_address: &SocketAddr) {
    let payload = &bincode::serialize(message).unwrap();
    let mut bytes_written = 0;
    while bytes_written < payload.len() {
        bytes_written += socket.send_to(payload, to_address).unwrap();
    }
    // let bytes_written = socket.send(payload).unwrap();
    // if bytes_written < payload.len() {
    //     log!(
    //         "should send {} bytes but only {bytes_written} bytes sent",
    //         payload.len()
    //     );
    // }
}

pub struct ReceiveMessage {
    pub message: Message,
    pub source_address: SocketAddr,
}

// pub fn receive(socket: &UdpSocket) -> Result<ReceiveMessage, std::io::Error> {
//     let mut buffer = [0; 10000];
//     socket
//         .recv_from(&mut buffer)
//         .map(|(bytes_read, source_address)| ReceiveMessage {
//             message: bincode::deserialize(&buffer[..bytes_read]).unwrap(),
//             source_address,
//         })
// }

pub fn receive_until_success(socket: &UdpSocket) -> ReceiveMessage {
    let mut buffer = [0; 10000];
    loop {
        if let Ok((bytes_read, source_address)) = socket.recv_from(&mut buffer) {
            match bincode::deserialize(&buffer[..bytes_read]) {
                Ok(message) => {
                    return ReceiveMessage {
                        message,
                        source_address,
                    }
                }
                Err(error) => {
                    log!("Can't decode packet with bincode, error: {error}");
                }
            }
        }
        // else {
        //     log!("recv_from error");
        //     sleep(Duration::from_millis(100));
        // }
    }
}

pub fn get_formatted_time() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        let now = get_formatted_time();
        println!("[{now}] {}", format_args!($($arg)*));
    };
}

pub fn setup_panic_logging_hook() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let now = get_formatted_time();
        println!("[{now}] Panic:");
        default_panic(info);
    }));
}
