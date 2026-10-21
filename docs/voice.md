# Voice and video

How Opencord carries voice, cameras and screen shares, which ports it uses, and how to host it behind a home router. The decisions behind it are in `docs/decisions.md` (D24 to D36); the measurements are in `docs/performance.md`.

## How it fits together

```
 app ──TLS WebSocket──► main server (7710/tcp)      chat, voice states, who may do what
  │                         │
  │                         └── control channel ──► voice node   (inside the server, or
  │                                                  on its own machine: 7712/tcp)
  ├──TLS WebSocket──► voice gateway (/voice on the node's port)   joining a call
  └──DTLS-SRTP over UDP──► voice node's SFU (7711/udp)            the media itself
```

- **Main server.** Joining a voice channel is a request on the main gateway. The server checks permissions (Connect, Speak, Video, Stream and so on), records the voice state, and answers with where to connect: a voice node, and a token signed with its voice-signing key that is good for 60 seconds, once. Server mute, deafen, move, disconnect and AFK moves all happen here, and the node is told.
- **Voice node.** A selective forwarding unit (SFU) built on str0m. It never decodes anything: it forwards each person's Opus audio to the others in their channel, and each camera's and screen share's H.264 layers to whoever watches, choosing for each receiver the layer that fits their tile and their downlink. A server has its own node inside it; more nodes can run on other machines (README, "Voice on other machines"), and a call moves to another node if its node stops.
- **App.** A Rust media engine: capture, echo cancellation, noise suppression, Opus, the jitter buffer and mixer, hardware video encoding and decoding, and the transport. Flutter only draws the textures the engine fills.

Media is encrypted in transit (DTLS-SRTP), but the voice node can read it, as on Discord before its DAVE protocol. The app shows no lock for voice. End-to-end encryption has hooks in place (a frame transform on both sides) and comes later.

## Ports

| Port | Protocol | What | Needed for |
|---|---|---|---|
| 7710 | TCP | The main gateway (TLS WebSocket), the media HTTP endpoints, and the embedded node's voice gateway at `/voice` | Everything |
| 7711 | UDP | Voice, camera and screen share media to and from the embedded node | Calls |
| 7712 | TCP | An external voice node's voice gateway | Only with external nodes |
| 7711 | UDP | An external voice node's media | Only with external nodes |
| 7713 | UDP | Music audio from Lavalink to an external voice node | Only with `[music]` on there (`docs/music.md`) |

All of them are set in `opencord.toml` (or `OPENCORD_*` variables): `[server] bind`, `[voice] udp_port`, and the node's own `opencord-voice-node.toml`.

## Hosting at home, behind a router

1. **Forward the ports** on the router to the server's machine: 7710 TCP and 7711 UDP. Without the UDP port, people join the voice channel but hear nothing; the app's voice panel stays on "Connecting" and then says it found no route.
2. **Set the public host.** `public_host` (`OPENCORD_PUBLIC_HOST`) is the name or address people reach the server at, from outside: a domain pointing at your home address, or the address itself. It goes into invite links and the TLS certificate.
3. **Media follows the main connection.** With `public_address = "auto"` (the default), the app sends media to the host it used to reach the server, so whatever name works for chat works for voice. Set `public_address` only when media must go somewhere else.
4. **People on your own network.** Many home routers do not let a device reach the house's public address from inside ("NAT loopback" or "hairpin NAT"). If friends outside can talk but you cannot from home, add the server from home by its local address (for example `192.168.1.20:7710`) instead.
5. **A slow upload.** `max_egress_mbps` under `[voice]` caps what the node sends in total. When it is reached, people get lower video layers first; voice is never cut.

There is no relay over TCP or TURN yet, so a network that blocks all UDP (some offices, some hotel Wi-Fi) cannot join calls.

## Docker

`deploy/` has a Dockerfile, a compose file and a systemd unit. Each release publishes the server's image as `ghcr.io/mrafieefard/opencord-server` (`latest` for stable, `beta` and `canary` for those channels; docs/releasing.md), which the compose file runs. In `deploy/`:

```bash
echo OPENCORD_PUBLIC_HOST=chat.example.com > .env
docker compose up -d
docker compose logs | grep claim_token
```

The compose file publishes 7710/tcp and 7711/udp, and keeps the config, database, certificate and keys in the `opencord-data` volume. Add the server in the app with the claim token from the log to become its owner, then make invites for others with `docker compose exec opencord opencord-server invite create`.

## When a call does not work

- **The voice panel's bars.** In a call, the bars beside "Voice connected" show how the connection is doing; click "Voice connected" for a graph of the last five minutes of ping (the round trip to the node), with the average and last ping, packet loss and jitter.
- **No sound at all, "No route"**: UDP 7711 does not reach the node. Check the router's forwarding, the server machine's firewall, and that nothing else uses the port. Behind a tunnel or reverse proxy, voice goes to the server's public address instead (docs/domains.md, Cloudflare Tunnel): check that it is known under Domains & network → Public address.
- **Others hear you but you hear nothing, or the reverse**: check the devices under User settings → Voice & audio; the mic test plays back what is sent.
- **Video stays blurry**: the receiver's downlink, or the sender's uplink, is too small for the top layer. The server's screen share cap (Server settings → Voice & video) also limits quality.
- **A voicebot checks a server from the command line**: README, "Voice". Two bots in a channel each report what they heard from the other.
