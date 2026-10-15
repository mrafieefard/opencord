# Music

Every voice channel has a music player. Its sound comes from Lavalink, an audio server built for Discord bots, which Opencord talks to as if it were Discord's voice servers: Lavalink connects to Opencord's **audio bridge** and sends it Opus, and the bridge hands the frames, unchanged, to everyone in the channel as the channel's music stream. Nothing joins the channel; there is no bot.

This guide grows with Phase 3. What is here so far:

- [How the bridge works](#how-the-bridge-works)
- [Ports](#ports)
- [Settings](#settings)
- [Testing with a local Lavalink](#testing-with-a-local-lavalink)

## How the bridge works

1. For each voice channel that plays music, the server creates a Lavalink player and gives it a voice server address that points at itself: `<domain>:<port>`, the main TLS port.
2. Lavalink connects to `wss://<domain>:<port>/?v=8` and signs in with a single-use token (Discord's voice protocol, version 8). Lavalink checks the certificate as a browser would, so the domain needs a certificate Lavalink trusts.
3. Lavalink sends Opus over UDP to the music port, encrypted with a key the bridge gave it for this connection.
4. The bridge decrypts it and forwards it, as it is, to the channel's listeners. Each client plays it as stereo sound with its own music volume. A listener who mutes music or deafens stops receiving it.

Lavalink also offers end-to-end encryption (Discord's DAVE); the bridge turns it down, and Lavalink then sends plain Opus inside the transport encryption.

## Ports

| Port | Protocol | Used for | Must be reachable from |
|---|---|---|---|
| 7710 (the main port) | TCP | Lavalink's connection to the bridge, with everything else | Lavalink nodes |
| 7712 | UDP | Music audio from Lavalink | Lavalink nodes |

A Lavalink on the same machine or the same Docker network reaches both without forwarding anything. The bridge tells each Lavalink where to send audio: its loopback address for a node on the same machine, this machine's address on the local network for a node there, and the server's public address for the rest.

## Settings

In `opencord.toml`:

```toml
[music]
enabled = true
audio_udp_port = 7712
bridge_domain = "primary"
```

| Key | Environment variable | Meaning |
|---|---|---|
| `enabled` | `OPENCORD_MUSIC_ENABLED` | Music in voice channels |
| `audio_udp_port` | `OPENCORD_MUSIC_UDP_PORT` | UDP port Lavalink sends music to |
| `bridge_domain` | | The domain Lavalink connects to: `primary`, or another active domain with a trusted certificate |

## Testing with a local Lavalink

`crates/opencord-voicebot/tests/lavalink.rs` plays a minute of music from a real Lavalink into a voice channel and checks that a voicebot there hears all of it. It is skipped unless it is given a Lavalink. To run one locally you need Java 17 or newer, `openssl`, `keytool` (part of Java) and `ffmpeg`.

Download Lavalink 4.2 or newer into an empty directory:

```sh
mkdir lavalink && cd lavalink
curl -LO https://github.com/lavalink-devs/Lavalink/releases/download/4.2.2/Lavalink.jar
```

Make a certificate for `localhost` and a copy of Java's trust store that trusts it:

```sh
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -subj /CN=localhost -addext subjectAltName=DNS:localhost,IP:127.0.0.1 -days 30 -keyout key.pem -out cert.pem
cp "$(dirname "$(readlink -f "$(command -v java)")")/../lib/security/cacerts" truststore.jks
chmod u+w truststore.jks
keytool -importcert -noprompt -alias opencord-test -file cert.pem -keystore truststore.jks -storepass changeit
```

Make a track with 330 Hz on the left and 440 Hz on the right, 80 seconds long:

```sh
ffmpeg -f lavfi -i sine=frequency=330:duration=80 -f lavfi -i sine=frequency=440:duration=80 -filter_complex "[0][1]amerge=inputs=2" -ac 2 -ar 48000 -c:a libopus track.ogg
```

Write `application.yml` next to the jar:

```yaml
server:
  port: 2333
  address: 127.0.0.1
lavalink:
  server:
    password: "testpass"
    sources:
      youtube: false
      bandcamp: false
      soundcloud: false
      twitch: false
      vimeo: false
      nico: false
      http: false
      local: true
```

The `local` source plays files from this machine: never turn it on for a Lavalink that anyone else can reach.

Start Lavalink with the trust store:

```sh
java -Djavax.net.ssl.trustStore=truststore.jks -Djavax.net.ssl.trustStorePassword=changeit -jar Lavalink.jar
```

Then, from the repository, in another terminal:

```sh
export OPENCORD_TEST_LAVALINK=http://127.0.0.1:2333
export OPENCORD_TEST_LAVALINK_PASSWORD=testpass
export OPENCORD_TEST_BRIDGE_TLS=/path/to/lavalink
export OPENCORD_TEST_TRACK=/path/to/lavalink/track.ogg
cargo test -p opencord-voicebot --test lavalink -- --ignored --nocapture
```

It prints what arrived: packets, losses, the longest wait between two packets, each channel's tone, and the bridge's counters.
