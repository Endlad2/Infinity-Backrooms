# Backrooms Infinity

Open-source Backrooms-inspired game prototype written in Rust/Bevy with a Flutter launcher.

Repository: `Endlad2/Infinity-Backrooms`.

## Layout

- `client/` — Bevy game client. Single-player and multiplayer use the same executable.
- `bi-lan/` — separate LAN sidecar. WebSocket lobby/control + QUIC realtime relay.
- `launcher/` — Flutter GUI launcher.
- `levels.json` — remote level catalogue used by the launcher.
- `levels/level-0.zip` — first playable sample level.
- `docs/LEVEL_FORMAT.md` — XML level format and tags.
- `docs/LUA_API.md` — sandboxed LuaJIT API available to level scripts.
- `.github/workflows/` — Windows/Linux CI and tagged-release packaging.
- `install.ps1`, `install.sh` — installers that fetch the latest GitHub release.

## Runtime data

All downloaded/runtime data is kept under `.backrooms_infinity`:

- Windows: `%APPDATA%\.backrooms_infinity\`
- Linux/macOS: `$HOME/.backrooms_infinity/`

Expected installed layout:

```text
.backrooms_infinity/
  bin/
    client[.exe]
    bi-lan[.exe]
    backrooms_infinity_launcher[.exe]
  levels/
    level-0/
      level.xml
      assets/
  levels.json
```

## Level packages

A level is distributed as a ZIP whose archive root contains:

```text
level.xml
assets/
  textures/
  models/
  audio/
  scripts/
  ...
```

The launcher downloads and extracts the selected archive. The client can also accept an extracted level directory or a `.zip` directly.

`level.xml` is parsed by a custom project-owned parser in `client/src/level/xml.rs`; no XML parsing crate is used. The level schema supports metadata, player/world settings, asset catalogues, chunk streaming, primitive geometry, glTF/GLB models, textured PBR materials, point/spot/directional lights, pickups, triggers, interactables, and Lua scripts.

Lua scripting uses `mlua` with the vendored **LuaJIT** backend. Scripts run in a restricted VM and interact with Bevy through the `bi` API. See `docs/LEVEL_FORMAT.md` and `docs/LUA_API.md`.

## Level 0

The repository includes `levels/level-0.zip`, an original Level 0 interpretation based on the supplied visual references: yellow patterned wallpaper, noisy carpet, fluorescent ceiling grids, repeating columns, broad office-like spaces, and a darker section with unstable-looking holes.

The sample demonstrates:

- four connected streamed chunks;
- generated wallpaper/carpet/ceiling textures;
- lights and unlit fluorescent panels;
- player collision with walls/columns;
- pickups;
- trigger volumes;
- interactive scripted points;
- a LuaJIT script that changes sanity, gives items, toggles a light panel, displays messages, and teleports the player out of void zones.

The attached screenshots were used as visual references for palette, lighting and spatial motifs; the distributed level uses newly generated textures and an original preview rather than bundling those reference images.

## Client CLI

```text
client --mode <singleplayer|multiplayer> --nickname <name> --level <level-id-or-path-or-zip> [--bridge-ws <url>] [--data-dir <path>]
```

Multiplayer deliberately has **no host/join client flag**. Both roles start `client` with the same multiplayer argument shape. The local `bi-lan` process hides whether this machine is the host or a participant.

## LAN protocol

Default ports:

- WebSocket lobby: `37600/tcp`
- QUIC realtime: `37601/udp`
- local game bridge: `127.0.0.1:37610/tcp`

Host:

```bash
bi-lan host --nickname Host
```

The host process writes JSON events to stdout. Send a JSON command to stdin to start:

```json
{"command":"start","level":"level-0"}
```

Join:

```bash
bi-lan join --host 192.168.1.50 --nickname Player2
```

The join process verifies the lobby via WebSocket, receives the temporary QUIC certificate, connects QUIC, emits `connected`, and waits for `start`.

Lobby limit is 5 total players including the host.

## Gameplay foundation

The client contains a playable 3D foundation: WASD movement, mouse look, level collision, pickup prompt on `F`, 10-slot backpack, 8-slot hotbar, 4 crafting input slots, drag/drop inventory UI, dropped 3D pickup entities, hunger, sanity, consuming selected items, recipe matching, chunked level content, LuaJIT scripting, and a local multiplayer state bridge.

The visible HUD shows only the currently selected hotbar item in the bottom-right; mouse wheel changes the selected slot.

## `levels.json`

The root catalogue currently contains Level 0:

```json
[
  {
    "name": "Level 0",
    "url": "https://github.com/Endlad2/Infinity-Backrooms/releases/latest/download/level-0.zip"
  }
]
```

The release workflow publishes `level-0.zip` as a release asset so the launcher URL remains stable.

## Build

### Rust

```bash
cargo build --release --workspace
```

### Flutter launcher

Flutter platform wrappers are generated reproducibly:

```bash
cd launcher
flutter pub get
flutter create --platforms=windows,linux --project-name backrooms_infinity_launcher .
flutter build windows --release   # Windows
flutter build linux --release     # Linux
```

CI performs Windows and Linux Rust checks/builds plus Flutter desktop analysis/builds.

## Development run

Build the workspace, then either start the launcher or run the client directly against the included level archive:

```bash
cargo run -p client -- --mode singleplayer --nickname Wanderer --level levels/level-0.zip
```

For authoring, extract a level ZIP, edit `level.xml` and assets, and point `--level` at that directory for fast iteration.

## Security notes

Level ZIP extraction rejects paths that escape the destination. Every asset/script path referenced by `level.xml` must stay inside the level package.

Lua uses mlua's safe VM and the client removes host-facing libraries/module entry points including `os`, `io`, `package`, `debug`, `require`, `dofile`, `loadfile`, `ffi`, and `jit`. This reduces the privileges of downloaded level scripts, though all untrusted content should still be reviewed before distribution.

LAN sessions use a per-host self-signed certificate delivered through the already-established WebSocket lobby handshake. This is intended for trusted local-network sessions, not hostile Internet matchmaking.
