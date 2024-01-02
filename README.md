# Simple Peer To Peer VPN

A simple peer to peer VPN for gaming and speeding up WebRTC like applications by using VPN tunnel as relay instead of their server (only works for some)

Some notes:

- Broadcast support, it can be used to play some old games in LAN mode multiplayer
- No encryption, it doesn't provide a secure tunnel like other VPNs
- Currently only support x86_64 Windows and Linux
- No Mac support because I don't own one

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

#### Tips

You can reconnect to your server more easily using a batch file

```batch
cd /D "%~dp0"
client.exe example.com:1234
pause
```

And you can add [this line](https://stackoverflow.com/a/51472107/16993372) at the beginning to the batch file or use a [short cut file](https://superuser.com/a/788929) to run as admin by default

```batch
if not "%1"=="am_admin" (powershell start -verb runas '%0' am_admin & exit /b)
```

## TODO

- [ ] Arm CPU support
- [x] Linux TAP support
- [ ] Encryption
- [ ] Handle errors instead of `unwrap` all over the place
- [ ] Doing IO asynchronously
- [x] IPv6 support
