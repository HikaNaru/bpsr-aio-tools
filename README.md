# BPSR AIO Tools

An all-in-one companion tool for **Blue Protocol: Star Resonance**, built for Linux.

This project exists to bring several separate community tools together into a single package. It is mostly a translation/port of [BPSR-ZDPS](https://github.com/Blue-Protocol-Source/BPSR-ZDPS), which is Windows-only, so that the same functionality can run natively on Linux.

> This project was built entirely with AI assistance ([Claude Code](https://claude.com/claude-code), latest Sonnet model). Expect rough edges and review the code yourself before trusting it with your account.

## Features

### Working

- **DPS Meter** — real-time damage/healing meter parsed from live game network traffic, plus an **Encounter History** browser for past fights.
- **Chat** — multi-channel chat log (World/Scene/Team/Union/Private/Group), raid warnings, countdown detection.
- **Modules Optimizer** — gear/module loadout optimizer.
- **Auto-Fishing** — screen-capture + OCR based fishing bot with simulated input.
  - Set the game to **Windowed 1600x900** for best detection results. Fullscreen is likely buggy/unreliable with the current detector.

### Not working yet

- **Cooldowns** — the tracker UI works and receives real combat events, but every tracked player and skill ID has to be added manually. There's no built-in skill database yet, so it's not really usable out of the box.
- **BPTimer / Spawn Tracker** — the WebSocket client is fully implemented, but it's disabled by default and has no configured backend server to connect to.
- **Threat Meter** — the UI exists, but the game's threat packets haven't been identified yet, so no data ever reaches it. It will always be empty for now.

## Development environment

This project is developed on **CachyOS** with **KDE Plasma (Wayland)**. Other distros/desktop environments/compositors should mostly work but haven't been tested, especially the Wayland-specific input handling used by Auto-Fishing.

## Prerequisites (Linux)

Package names below are for Arch/CachyOS (`pacman`). Adjust for your distro if different.

**Build-time libraries** (linked via pkg-config):

```
sudo pacman -S libpcap alsa-lib dbus pkgconf
```

- `libpcap` — packet capture for the DPS meter
- `alsa-lib` — audio playback (`rodio`)
- `dbus` — used by the screen-capture crate (`xcap`)
- `pkgconf` — resolves the above at build time

**Runtime CLI tools** (invoked as external processes, must be on `PATH`):

```
sudo pacman -S xdotool ydotool wmctrl
```

- `xdotool` — keyboard input, mouse movement, window queries (works via XWayland)
- `ydotool` — mouse clicks at the evdev level, required for real pointer-lock input on Wayland. Its daemon, `ydotoold`, must be running (e.g. as a systemd service) before using Auto-Fishing.
- `wmctrl` — fallback window finder if `xdotool search` fails

**Optional screenshot fallback** (only needed if the built-in `xcap` capture fails):

- `spectacle` (ships with KDE Plasma) — used first
- `grim` (wlroots compositors) or `scrot`/`imagemagick` (X11) — further fallbacks

**GUI/windowing runtime libraries** — normally already present on a full KDE Plasma Wayland install: `wayland`, `libxkbcommon`, `libglvnd`, `libxcb`/`libx11`.

## Usage

```bash
git clone git@github.com:HikaNaru/bpsr-aio-tools.git
cd bpsr-aio-tools
cargo build --release

# Packet capture needs CAP_NET_RAW — either run as root, or grant the capability once:
sudo setcap cap_net_raw+eip ./target/release/bpsr-aio-tools

# Make sure ydotoold is running before using Auto-Fishing, e.g.:
sudo systemctl enable --now ydotool.service

./target/release/bpsr-aio-tools
```

The app launches without `CAP_NET_RAW`, but capture-dependent features (DPS meter, chat, etc.) won't receive any data until the capability is granted.

Configuration is stored at `~/.config/bpsr/config.json` and is created automatically on first run.

## Windows users

This project targets Linux. If you're on Windows, use one of the original tools instead:

- [BPSR-ZDPS](https://github.com/Blue-Protocol-Source/BPSR-ZDPS)
- [ok-star-resonance](https://github.com/Sanheiii/ok-star-resonance)

## License

No license has been chosen yet.
