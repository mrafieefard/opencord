# Music

Every voice channel has a music player. Its sound comes from Lavalink, an audio server built for Discord bots, which Opencord talks to as if it were Discord's voice servers: Lavalink connects to Opencord's **audio bridge** and sends it Opus, and the bridge hands the frames, unchanged, to everyone in the channel as the channel's music stream. Nothing joins the channel; there is no bot.

This guide grows with Phase 3. What is here so far:

- [How the bridge works](#how-the-bridge-works)
- [Ports](#ports)
- [Settings](#settings)
- [Lavalink nodes](#lavalink-nodes)
- [Permissions](#permissions)
- [Music settings](#music-settings)
- [When a node goes down](#when-a-node-goes-down)
- [Voice on other machines](#voice-on-other-machines)
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
allow_unsafe_sources = false
```

| Key | Environment variable | Meaning |
|---|---|---|
| `enabled` | `OPENCORD_MUSIC_ENABLED` | Music in voice channels |
| `audio_udp_port` | `OPENCORD_MUSIC_UDP_PORT` | UDP port Lavalink sends music to |
| `bridge_domain` | | The domain Lavalink connects to at first: `primary`, or another active domain with a trusted certificate. Music settings can change it later |
| `allow_unsafe_sources` | | Lets members play any `http` or `https` address and, by its path, any file on the Lavalink's machine (Lavalink's `http` and `local` sources). Off unless you trust everyone who can add music |

## Lavalink nodes

Music plays on Lavalink nodes that the server's admins add in **Server settings → Music** (needs Manage server). A node can be your own Lavalink, version 4 or newer, or a public one. Opencord connects to it; people's apps never do, and its password stays on the server.

For each node you give:

| Field | Meaning |
|---|---|
| Name | Shown in settings and node events |
| Address and port | Where the server reaches Lavalink (`server.port` in its `application.yml`, 2333 by default) |
| TLS | On when the node is reached over https and wss |
| Password | Lavalink's `lavalink.server.password` |
| Priority | Preferred, Normal or Backup; Backup nodes play only when no other node can |
| Max players | How many voice channels it plays for at once; empty for no limit |
| Strict address check | On: audio is accepted only from the node's own addresses. Turn it off for a public node whose address changes |

At the top of the page, **What Lavalink needs** shows where Lavalink connects back to (`wss://<domain>:<port>`, and whether its certificate is one Lavalink trusts), where it sends audio, and whether the router has the music port mapped.

Each node shows its state:

| State | Meaning |
|---|---|
| Connecting | The first connection is being made |
| Healthy | Connected and coping |
| Degraded | Connected but struggling; the reason is shown (CPU load, missing audio frames, slow answers, failing tracks). New channels avoid it while a healthy node can take them |
| Unavailable | Not reachable, or the password is wrong; the reason is shown. Opencord keeps retrying |
| Draining | Set by you before maintenance: it gets no new channels |
| Disabled | Turned off |

A node that comes back from Unavailable waits 30 seconds before it gets new channels. New channels go to the healthy node with the best priority and the least load. Give every node the same sources, so a track can move to another node.

**Test** runs three steps: Opencord reaches Lavalink and the password is accepted; Lavalink's WebSocket is ready; a temporary player in Lavalink reaches this server's audio bridge. A failed step says what to fix, for example:

- *Lavalink refused the password*: copy `lavalink.server.password` again.
- *Nothing is listening on …*: check the port, and that Lavalink is running.
- *… does not use TLS: turn TLS off for this node*, or the other way round.
- *Lavalink couldn't verify Opencord's certificate*: activate a domain with a certificate in Domains & network, or, for a Lavalink you run, import Opencord's certificate into its trust store.
- *Lavalink signed in, but didn't send audio to UDP …*: open the music port for this node.

A row's menu also has **Move players now**, while the node plays for any channel: each channel moves to the best other node at once, keeping its place in the track. **Drain** moves them more gently, at their next track change. Deleting a node moves its channels first.

## Permissions

| Permission | Allows | `@everyone` by default |
|---|---|---|
| Add music | Search and add tracks (playing at once when nothing plays); remove your own queued tracks; skip your own track | On |
| Control playback | Pause, resume, seek, next, previous, repeat, shuffle | On |
| Manage the queue | Reorder, remove anyone's tracks, clear the queue, Play next and Play now | Off |
| Manage music | Stop the player, block members from music, go past the track length and per-member limits, control music without being in the channel, music settings | Off |

Adding and controlling music needs being connected to that voice channel, except with Manage music. Channel permission overwrites apply, so music can be kept to one channel. The server checks all of it; the app only hides what you can't do.

**Blocking from music** is in a member's menu (Manage music, and a higher role than theirs). Blocked members still hear the music but can't add tracks or control it. Server settings → Music lists them with the reason and an Unblock button.

## Music settings

Server settings → Music, for members with Manage music. Lavalink nodes on the same page need Manage server.

| Setting | Default |
|---|---|
| Music enabled | On. Turning it off ends the music in every channel |
| Search sources | Every source a node offers. Only the sources turned on can be searched or linked |
| Default source | YouTube Music |
| Allow links | On: `https` links to an enabled source's sites. Addresses, `localhost` and private names are always refused |
| Allow live streams | On |
| Longest track | No limit (10 min, 30 min, 1 h or 3 h) |
| Tracks per member in the queue | 20 |
| Tracks added from one playlist | 100 |
| Queue limit | 500 |
| Vote skip | Off, 50 % of those listening |
| Stop after everyone leaves | 2 min |
| Disconnect after the queue ends | 5 min |
| Bridge domain | The primary domain |

You choose, and are responsible for, the sources you enable.

When everyone leaves a channel, its music pauses at once. After *Stop after everyone leaves* its Lavalink player is let go; the queue is kept for 30 minutes, and music carries on from the same place when someone comes back. When the queue ends, the player is let go after *Disconnect after the queue ends*.

## Search, links and artwork

Members search the sources turned on in the music settings that a node offers now: YouTube Music, YouTube, SoundCloud, Bandcamp, and Spotify, Apple Music and Deezer through the LavaSrc plugin. A search finds up to 15 tracks. Members get opaque result ids, good for 15 minutes and for them only. The app never sends Lavalink an encoded track or a raw identifier, so it can't make Lavalink play anything a search didn't find. 10 searches per 10 seconds per member; a search is at most 200 characters.

**Links** work when *Allow links* is on: `https` only, to the sites of an enabled source (`youtube.com`, `youtu.be`, `music.youtube.com`, `soundcloud.com`, `open.spotify.com`, `music.apple.com`, `deezer.com`, `*.bandcamp.com` and a few more). Addresses, `localhost`, private names and links with a user name are refused, and so are tracks Lavalink would play from its `http` or `local` source, since those make Lavalink fetch any address or file. A playlist link shows as one card with Add all, up to the playlist limit.

**Artwork** comes from Opencord, never straight from YouTube or other sites. Opencord fetches only the artwork addresses Lavalink gave in results: https, public hosts, at most 5 MB in 5 seconds. It makes a 128 px and a 512 px copy, encoded again without metadata, and keeps them for 7 days in `data/music-artwork` (200 MB at most, the least recently used going first). Artwork that can't be fetched shows a music-note placeholder, and isn't tried again for 10 minutes. The app keeps what it showed in its own 100 MB cache.

YouTube needs the youtube-source plugin in Lavalink; Lavalink's own YouTube source no longer works. Its own SoundCloud source fails to search with Lavalink 4.2.2 ("Did not detect any supported formats"). With the plugin, add to `application.yml`:

```yaml
plugins:
  youtube:
    enabled: true
    allowSearch: true
    clients:
      - MUSIC
      - WEB
      - WEBEMBEDDED
      - ANDROID_VR
lavalink:
  plugins:
    - dependency: "dev.lavalink.youtube:youtube-plugin:1.18.2"
      snapshot: false
  server:
    sources:
      youtube: false
```

The plugin's own documentation explains what YouTube needs from time to time, such as a refresh token or a different client.

## Music in the app

In a voice channel, the **Music** button (in the voice panel at the bottom left, and in the call's control bar) opens the music panel where the users list is; press it again for the people. The right-side panel switches between **People** and **Music** while a voice channel is open or you are in one. People lists who is in the channel first, under a card of what plays; a click on the card opens the music panel, and a right-click has your music volume and mute.

The music panel shows the cover, the track, who added it and its progress, with shuffle, previous, play or pause, next and repeat (off, the queue, the track). Under them, **Add a song** searches the source chosen beside it, or takes a link; results replace Up next until **Back to queue**. Up next can be reordered by dragging with Manage the queue, and each row's menu has Play next, Move up, Move down, Remove, Copy link and Open link. **Recently played** lists the last 50 tracks, each with Add again. Dropping a result or a recently played track on the cover plays it now. **Expand** in the panel's menu shows the music in the middle of the call, with the people in a strip below.

Members not in the channel see the panel read-only, with a Join button; controls a member may not use are off, and say why. Music isn't a participant: it has no tile and no speaking ring. Where music isn't set up, the button shows only for members who manage the server, with **Set up music**.

**Your music volume** (0–200 %) and **Mute music** are the same on every server and stay on this device: in the panel, on the card, and in User settings → Voice & audio → Music, with **Lower music while people talk** (10 dB, back 300 ms after the last voice). Muted, the voice node stops sending you the music.

## When a node goes down

Each channel stays on its node while it plays. When that node becomes unavailable, its channels move to the best other node that offers the track's source, 5 at a time, each from where it was in the track, with a new token at the bridge so nothing more is accepted from the old node. The music panel says "Switched to another music server", and the node events say how many channels moved where. When no node can play, the music pauses with "Music server unavailable — retrying" and carries on once one can.

A node that comes back has its old players destroyed, so nothing plays twice. A channel whose node starts struggling or is drained moves at its next track change.

**Silent players.** While a track plays, the bridge expects audio. When none comes for 2 seconds although Lavalink is connected, Opencord asks the node for `/v4/info` and gives it 2 seconds. If it answers, only that player is started again, at most once every 30 seconds. If it doesn't, the node is marked unavailable ("Lavalink stopped sending audio and doesn't answer") and the channel moves at once.

At the top of the node list, the bridge shows how many Lavalinks are connected, the packets it took, its jitter, and packets it dropped or could not decrypt.

Opencord's own restarts keep the music playing: the queue is saved every few seconds, Lavalink keeps its players while Opencord is away, and each channel goes back to its node and carries on.

## Voice on other machines

When voice runs on external voice nodes, the music for a channel enters at the node serving it, so the listeners get it straight from there. Turn the bridge on in each node's `opencord-voice-node.toml`:

```toml
[music]
enabled = true
audio_udp_port = 7713
```

Lavalink connects to the node's own endpoint (`wss://<node>:<port>/?v=8`), so the node's certificate must be one Lavalink trusts, and it sends audio to the node's music port. The main server tells the node which player to let in, and the node reports back what its bridge sees. The node list in Server settings → Music lists each node's bridge.

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

The same Lavalink also runs the server's node test, which adds it as a node and runs its connection test:

```sh
cargo test -p opencord-server --test lavalink a_real -- --ignored --nocapture
```

It needs `OPENCORD_TEST_LAVALINK`, `OPENCORD_TEST_LAVALINK_PASSWORD` and `OPENCORD_TEST_BRIDGE_TLS`, set as above.

### Search and artwork from the real sites

With the youtube-source plugin set up as in [Search, links and artwork](#search-links-and-artwork), and the internet, this searches YouTube Music and YouTube through a real Lavalink, loads a found track's link again, and fetches its artwork through Opencord's proxy:

```sh
cargo test -p opencord-server --test music_search a_real -- --ignored --nocapture
```

It needs `OPENCORD_TEST_LAVALINK` and `OPENCORD_TEST_LAVALINK_PASSWORD`. `OPENCORD_TEST_SEARCH_SOURCES` changes the sources it tries, for example `youtube_music,youtube,spotify` with LavaSrc.

### Failover between two nodes

Start a second Lavalink from another directory with the same `application.yml`, its `server.port` changed to 2334:

```sh
mkdir second && cp application.yml second/ && cd second
sed -i 's/port: 2333/port: 2334/' application.yml
java -Djavax.net.ssl.trustStore=../truststore.jks -Djavax.net.ssl.trustStorePassword=changeit -jar ../Lavalink.jar
```

Lavalink can't seek in Ogg files, so make an MP3 track for this test:

```sh
ffmpeg -f lavfi -i sine=frequency=330:duration=80 -ac 2 -ar 48000 -c:a libmp3lame -b:a 128k track.mp3
```

Then, from the repository, with the variables above:

```sh
export OPENCORD_TEST_LAVALINK_2=http://127.0.0.1:2334
export OPENCORD_TEST_SEEKABLE_TRACK=/path/to/lavalink/track.mp3
cargo test -p opencord-server --test music two_real -- --ignored --nocapture
```

It plays the track on the first node, turns that node off, checks that the channel plays on the second within 5 seconds from where it was, then restarts the server and checks that the music carries on there.
