use std::{io, net::Ipv4Addr};

use macaddr::MacAddr6;

pub trait TapDevice {
    fn open_or_create(name: &str) -> io::Result<Self>
    where
        Self: Sized;
    fn up(&self) -> io::Result<()>;
    fn get_mac(&self) -> io::Result<MacAddr6>;
    fn get_mtu(&self) -> io::Result<u32>;
    fn set_ip(&self, address: impl Into<Ipv4Addr>, mask: impl Into<Ipv4Addr>) -> io::Result<()>;
    fn read_non_mut(&self, buf: &mut [u8]) -> io::Result<usize>;
    fn write_non_mut(&self, buf: &[u8]) -> io::Result<usize>;
}

#[cfg(target_os = "windows")]
pub struct Device(tap_windows::Device);

#[cfg(target_os = "windows")]
impl TapDevice for Device {
    fn open_or_create(name: &str) -> io::Result<Self> {
        tap_windows::Device::open(tap_windows::HARDWARE_ID, name)
            .or_else(|_| {
                let device = tap_windows::Device::create(tap_windows::HARDWARE_ID)?;
                device.set_name(name)?;
                Ok(device)
            })
            .map(Into::into)
    }

    fn up(&self) -> io::Result<()> {
        self.0.up()
    }

    fn get_mac(&self) -> io::Result<MacAddr6> {
        Ok(self.0.get_mac()?.into())
    }

    fn get_mtu(&self) -> io::Result<u32> {
        self.0.get_mtu()
    }

    fn set_ip(&self, address: impl Into<Ipv4Addr>, mask: impl Into<Ipv4Addr>) -> io::Result<()> {
        self.0.set_ip(address, mask)
    }

    fn read_non_mut(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read_non_mut(buf)
    }

    fn write_non_mut(&self, buf: &[u8]) -> io::Result<usize> {
        self.0.write_non_mut(buf)
    }
}

#[cfg(target_os = "windows")]
impl From<tap_windows::Device> for Device {
    fn from(device: tap_windows::Device) -> Self {
        Self(device)
    }
}

#[cfg(target_os = "linux")]
fn ip_command(args: &[&str]) -> Result<(), io::Error> {
    use std::process::Command;
    Command::new("ip").args(args).status()?;
    Ok(())
}

#[cfg(target_os = "linux")]
pub struct Device(tun_tap::Iface);

#[cfg(target_os = "linux")]
impl TapDevice for Device {
    fn open_or_create(name: &str) -> io::Result<Self> {
        let device = tun_tap::Iface::without_packet_info(name, tun_tap::Mode::Tap)?;
        Ok(Device(device))
    }

    fn up(&self) -> io::Result<()> {
        ip_command(&["link", "set", "up", "dev", self.0.name()])
    }

    fn get_mac(&self) -> io::Result<MacAddr6> {
        for interface in nix::ifaddrs::getifaddrs()? {
            if interface.interface_name != self.0.name() {
                continue;
            }
            let mac_address = (|| interface.address?.as_link_addr()?.addr())();
            if let Some(mac_address) = mac_address {
                return Ok(mac_address.into());
            }
        }
        unreachable!()
    }

    fn get_mtu(&self) -> io::Result<u32> {
        // TODO: actually get the mtu instead of hard code a default value
        return Ok(1500);
    }

    fn set_ip(&self, address: impl Into<Ipv4Addr>, mask: impl Into<Ipv4Addr>) -> io::Result<()> {
        let address: Ipv4Addr = address.into();
        let mask: Ipv4Addr = mask.into();

        let mut cidr_suffix = 0;
        for octet in mask.octets() {
            cidr_suffix += octet.leading_ones();
            if octet < 255 {
                break;
            }
        }

        ip_command(&[
            "addr",
            "add",
            "dev",
            self.0.name(),
            &format!("{address}/{cidr_suffix}"),
        ])
    }

    fn read_non_mut(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.recv(buf)
    }

    fn write_non_mut(&self, buf: &[u8]) -> io::Result<usize> {
        self.0.send(buf)
    }
}

#[cfg(target_os = "linux")]
const INTERFACE_NAME: &str = "simple_p2p";
#[cfg(target_os = "windows")]
const INTERFACE_NAME: &str = "Simple Peer To Peer";

pub fn setup_tap() -> Device {
    // Try to open_or_create the device
    let tap_device = Device::open_or_create(INTERFACE_NAME).expect("Failed to open or create TAP");
    tap_device.up().expect("Failed to turn on TAP");
    tap_device
}
