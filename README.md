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

Also, you can use task scheduler to run as admin without UAC prompt

```batch
@REM connect.batch

set task_name="Simple P2P Connect Example"
set command="\"%~dp0client.exe\" example.com:1234"

@REM Check admin: https://stackoverflow.com/a/11995662/16993372
net session >nul 2>&1
if %errorlevel% == 0 (
    @REM is admin
    @REM /sc ONCE: Run once at a specified date and time.
    @REM /st 00:00: Start time for the task (setting it to the past so it never triggers)
    @REM /f: Overwrites existing task with the same name
    @REM /rl HIGHEST: Run as admin
    @REM /tr command: Task command
    schtasks /create /tn %task_name% /sc ONCE /st 00:00 /f /rl HIGHEST /tr %command%
    schtasks /run /tn %task_name%
    exit
)
@REM not admin
schtasks /run /tn %task_name%
if not %errorlevel% == 0 (
    @REM Task not setup yet
    powershell Start-Process '%0' -Verb RunAs
)
```

```batch
@REM cleanup.batch

if not "%1"=="am_admin" (powershell start -verb runas '%0' am_admin & exit /b)

set task_name="Simple P2P Connect Example"
@REM /f for no confirmation
schtasks /delete /tn %task_name% /f
```

## TODO

- [ ] Arm CPU support
- [x] Linux TAP support
- [ ] Encryption
- [x] Handle errors instead of `unwrap` all over the place
- [ ] Doing IO asynchronously
- [x] IPv6 support
- [ ] Support `--version` command line argument
