use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

pub type MacAddress = [u8; 6];
const BROADCAST_MAC_ADDRESS: MacAddress = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff];

pub fn is_broadcast(mac_adrress: &MacAddress) -> bool {
    *mac_adrress == BROADCAST_MAC_ADDRESS
}

// Broadcast is a special type of multicast
pub fn is_multicast(mac_adrress: &MacAddress) -> bool {
    // [xxxxxxx1][xxxxxxxx][xxxxxxxx][xxxxxxxx][xxxxxxxx][xxxxxxxx]
    //         ↑
    mac_adrress[0] & 1 == 1
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Register {
        mac_address: MacAddress,
    },
    RegisterSuccess {
        ip: Ipv4Addr,
        subnet_mask: Ipv4Addr,
    },
    RegisterFail {
        reason: String,
    },
    Ping,
    Pong,
    Data {
        source_mac_address: MacAddress,
        destination_mac_address: MacAddress,
        payload: Vec<u8>,
    },
}

pub fn send(socket: &UdpSocket, message: &Message) {
    let payload = &bincode::serialize(message).unwrap();
    let mut bytes_written = 0;
    while bytes_written < payload.len() {
        bytes_written += socket.send(payload).unwrap();
    }
    // let bytes_written = socket.send(payload).unwrap();
    // if bytes_written < payload.len() {
    //     println!(
    //         "should send {} bytes but only {} bytes sent",
    //         payload.len(),
    //         &bytes_written
    //     );
    // }
}

pub fn send_to(socket: &UdpSocket, message: &Message, to_address: &SocketAddr) {
    let payload = &bincode::serialize(message).unwrap();
    let mut bytes_written = 0;
    while bytes_written < payload.len() {
        bytes_written += socket.send_to(payload, to_address).unwrap();
    }
    // let bytes_written = socket.send_to(payload, to_address).unwrap();
    // if bytes_written < payload.len() {
    //     println!(
    //         "should send {} bytes but only {} bytes sent",
    //         payload.len(),
    //         &bytes_written
    //     );
    // }
}

pub struct ReceiveMessage {
    pub message: Message,
    pub source_address: SocketAddr,
}

pub fn receive(socket: &UdpSocket) -> Result<ReceiveMessage, std::io::Error> {
    let mut buffer = [0; 10000];
    socket
        .recv_from(&mut buffer)
        .map(|(bytes_read, source_address)| ReceiveMessage {
            message: bincode::deserialize(&buffer[..bytes_read]).unwrap(),
            source_address,
        })
}

pub fn receive_until_success(socket: &UdpSocket) -> ReceiveMessage {
    let mut buffer = [0; 10000];
    loop {
        if let Ok((bytes_read, source_address)) = socket.recv_from(&mut buffer) {
            return ReceiveMessage {
                message: bincode::deserialize(&buffer[..bytes_read]).unwrap(),
                source_address,
            };
        }
        // else {
        //     println!("recv_from error");
        //     sleep(Duration::from_millis(100));
        // }
    }
}
