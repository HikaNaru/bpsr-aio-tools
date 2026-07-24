# CLAUDE.md

Guidance for Claude Code working in this repo.

## Project

BPSR AIO Tools — an all-in-one Linux companion tool for the game *Blue Protocol: Star Resonance*, unifying several separate community tools (DPS meter, chat log, gear optimizer, auto-fishing, etc.) into one app. Mostly a Linux port of [BPSR-ZDPS](https://github.com/Blue-Protocol-Source/BPSR-ZDPS) (Windows-only). See [README.md](README.md) for user-facing details, features, and Linux setup requirements.

## Architecture

Rust Cargo workspace. Binary crate is the root package (`bpsr-aio-tools`), entrypoint at `src/main.rs` + `src/app.rs`, built on `egui`/`eframe`.

Workspace members (`crates/`):

| Crate | Purpose |
|---|---|
| `core` | Shared `AppConfig`, `Module` trait, error types, Discord webhook reporting, data-path helpers |
| `capture` | Live packet capture (`pcap`/`etherparse`) and game protocol decode into `GameEvent`s |
| `game` | Shared domain types: `GameState`, `Entity`, `GameEvent` — the common event bus |
| `dps-meter` | DPS/HPS meter + encounter history browser |
| `auto-fishing` | Screen-capture + OCR + simulated-input fishing bot |
| `modules-optimizer` | Gear/module loadout optimizer |
| `ui` | Shared egui theme, icons, widget library used by other feature crates |
| `encounter-store` | Persists/loads past DPS encounters to disk (zstd + JSON) |
| `chat` | In-game chat log viewer with channel tabs |
| `cooldown-tracker` | Manual player/skill cooldown timer (requires manual per-skill setup) |
| `bptimer` | WebSocket client for a field-boss spawn-timer service (disabled by default) |
| `threat-meter` | Threat/aggro table UI (no data source yet — see README) |

`crates/character/` exists on disk but is **not** a workspace member and is not wired into `app.rs`. Treat it as dead code, not an active feature, unless it's explicitly revived.

## Commands

```bash
cargo build --release
cargo run --release
cargo test
cargo clippy
```

Packet capture requires `CAP_NET_RAW`. The binary runs without it but capture-dependent features get no data — see README for `setcap` usage.

## Workflow before committing

Always, before creating a commit in this repo:

1. Run `git status` / `git diff` and review the actual changes.
2. If the change affects features, usage, or dependencies described in [README.md](README.md), update README.md first.
3. Bump `version` under `[workspace.package]` in the root `Cargo.toml`.
