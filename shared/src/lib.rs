use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

pub type MacAddress = [u8; 6];
const BROADCAST_MAC_ADDRESS: MacAddress = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
const MULTICAST_ADDRESS_START: MacAddress = [0x01, 0x80, 0xC2, 0x00, 0x00, 0x00];

pub fn is_broadcast_or_multicast(mac_adrress: &MacAddress) -> bool {
    if *mac_adrress == BROADCAST_MAC_ADDRESS {
        // Is broadcast
        return true;
    }
    if mac_adrress[0..=2] == MULTICAST_ADDRESS_START[0..=2] {
        // Is multicast
        return true;
    }
    false
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
    Data {
        destination_mac_address: MacAddress,
        source_mac_address: MacAddress,
        payload: Vec<u8>,
    },
}

pub fn send(socket: &UdpSocket, message: &Message) {
    socket.send(&bincode::serialize(message).unwrap()).unwrap();
}

pub fn send_to(socket: &UdpSocket, message: &Message, to_address: &SocketAddr) {
    socket
        .send_to(&bincode::serialize(message).unwrap(), to_address)
        .unwrap();
}

pub struct ReceiveMessage {
    pub message: Message,
    pub source_address: SocketAddr,
}

pub fn receive_until_success(socket: &UdpSocket) -> ReceiveMessage {
    let mut buf = [0; 10000];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((bytes_read, source_address)) => {
                let filled_buf = &mut buf[..bytes_read];

                return ReceiveMessage {
                    message: bincode::deserialize(&filled_buf).unwrap(),
                    source_address,
                };
            }
            Err(_) => {}
        }
    }
}
