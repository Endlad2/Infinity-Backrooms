# bi-lan protocol

## Process control

`bi-lan` writes one JSON object per line to stdout. Launcher commands are JSON lines on host stdin.

Host events:

- `host_ready`: contains LAN IP and ports.
- `player_count`: lobby occupancy.
- `start`: emitted after host start command.

Join events:

- `connected`: WS health/lobby check succeeded and QUIC is connected.
- `start`: host started the session.
- `error`: recoverable protocol error.

Host command:

```json
{"command":"start","level":"level-id"}
```

## WS lobby

`ws://HOST:37600/lobby?nickname=NAME`

The first server frame is `welcome`, containing the QUIC port and the host's ephemeral certificate encoded as base64. The host refuses the sixth player (host counts as one of five).

## QUIC game relay

Game state is treated as opaque UTF-8 JSON datagrams. Host broadcasts received datagrams to all connected QUIC peers and to its own local client bridge. Participants send local client datagrams to host and pass host datagrams back to the local client.

## Local client bridge

Every role exposes `ws://127.0.0.1:37610/client`. The Bevy client talks only to this endpoint, so it never needs to know whether it is host or participant.
