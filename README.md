# Simple Peer To Peer VPN

A simple peer to peer VPN for gaming and speeding up WebRTC like applications by using VPN tunnel as relay instead of their server (only works for some)

Some notes:

- Broadcast support, it can be used to play some old games in LAN mode multiplayer
- No encryption, it doesn't provide a secure tunnel like other VPNs
- Currently client is only supported on x86_64 Windows, and server is only supported on x86_64 Windows and x86_64 Linux

> Currently working in progress, just a proof of concept

## Usage

1. Download the pre-built executable from [latest release](https://github.com/Legend-Master/simple-p2p-vpn/releases/latest)
2. Run server on a machine that has a publicly accessible IP
   ```powershell
   # server <port>
   server 1234
   ```
3. Run client with administrator permission (required for setting up TAP device)
   ```powershell
   # client <server ip/domain>:<server port>
   client example.com:1234
   ```

### Running Client On Windows

You'll need to install [TAP Windows driver](https://build.openvpn.net/downloads/releases/latest.bak/tap-windows-latest-stable.exe) from OpenVPN first

## TODO

- [ ] Arm CPU support
- [ ] Linux TAP support
- [ ] Encryption
- [ ] Handle errors instead of `unwrap` all over the place
- [ ] Doing IO asynchronously
