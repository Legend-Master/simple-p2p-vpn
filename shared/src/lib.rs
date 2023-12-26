use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

pub type MacAddress = [u8; 6];
pub const BROADCAST_MAC_ADDRESS: MacAddress = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff];

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Register {
        mac_address: MacAddress,
    },
    RegisterSuccess {
        ip: Ipv4Addr,
        mask: Ipv4Addr,
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

pub struct ReceiveMessage {
    pub message: Message,
    pub source_address: SocketAddr,
}

pub fn receive(socket: &UdpSocket) -> ReceiveMessage {
    let mut buf = [0; 10000];
    let (bytes_read, source_address) = socket.recv_from(&mut buf).expect("Didn't receive data");
    let filled_buf = &mut buf[..bytes_read];

    ReceiveMessage {
        message: bincode::deserialize(&filled_buf).unwrap(),
        source_address,
    }
}
