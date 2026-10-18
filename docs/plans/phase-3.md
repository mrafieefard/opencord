# Opencord — Phase 3 Plan: Domains, Certificates & Music (Lavalink)

> Builds on `opencord-phase1-plan.md` (server, protocol, permissions, invites), `opencord-phase2-voice-video-plan.md` (voice node, media engine, voice UI) and `opencord-desktop-ui-plan.md`. This version replaces every earlier draft of the Phase 3 plan.
>
> **Part A — Domains and certificates:** the server owner attaches domains to the server, Opencord verifies them with DNS, gets Let's Encrypt certificates, and users connect and get invited through those domains. Owners without a domain can use a free subdomain or a Cloudflare Tunnel.
>
> **Part B — Music with Lavalink:** admins attach one or more Lavalink nodes (their own or public ones; public ones work thanks to Part A). Every voice channel has its own built-in music player that everyone with permission can use, from the player UI or with chat commands like `/play`.
>
> **There is no music bot.** Nothing joins the voice channel and nothing appears as a participant. Music is a built-in feature of each voice channel.

---

## 0. Instructions for Claude Code

- Start after Phase 2 milestones V1–V3 (voice node, client audio engine) are done.
- Work through the milestones in §21 in order: **L0** (a technical spike that must pass first), then **D1–D4** (domains), then **L1–L8** (music).
- After every milestone run the usual checks (`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `flutter analyze`, `flutter test`).
- Read the documentation in §22 before implementing each area. Verify behavior against the real systems (Lavalink 4.2.x or newer, Let's Encrypt's test environment, real DNS); don't rely on memory.
- **Clients never talk to Lavalink.** Lavalink passwords, DNS provider tokens, tunnel tokens and private keys never leave the server.
- Record decisions and spike results in `docs/decisions.md`.
- Terminal commands in docs must not contain inline comments.

### 0.1 UI and performance rules (apply to every screen in this plan)
The new UI — the Domains & network page, Lavalink node list, Commands page, command autocomplete and the music panel — must feel like part of the existing app and stay light:

- **Reuse, don't invent.** Build everything from the existing desktop UI plan components: settings sections and rows, dialogs, the right-side panel, the composer's autocomplete popup, toasts, context menus, the design tokens and the monochrome shape language. No new UI packages, no new visual style.
- **No heavy effects.** No blur (`BackdropFilter`, `ImageFilter.blur`), no extra shadows, no gradients, no continuous animations. The only thing that updates on its own is the music progress bar, at most 4 times per second and only while it's visible. Use static icons (for example a static equalizer glyph for "playing").
- **Virtualized lists** for anything that can grow: queue (up to 500), search results, history, domains, nodes, commands, check history.
- **Images:** decode artwork at the size it's shown (`cacheWidth`/`cacheHeight`): 128 px for list thumbnails, 512 px for the main cover. Keep a small in-memory LRU cache.
- **Granular state:** progress ticks rebuild only the progress bar; a queue change rebuilds only the affected rows; a node or domain status change rebuilds only its row.
- **Server does the work.** DNS checks, certificate requests, tunnels and Lavalink health checks run on the server. The UI shows progress from events and never blocks or polls in a loop. Settings pages subscribe to live status only while they're open.
- **Targets (profile mode, mid-range laptop):** music panel opens in under 100 ms; dragging in a 500-track queue stays at 60 fps; settings pages with 20 domains or nodes render in under 50 ms; no frame over 16 ms on these screens; the music panel uses no CPU while music is paused.
- Monochrome as always. Album artwork is user content and keeps its colours, like emoji.

---

## 1. Scope

**In this phase**
- **Domains & network** page for the server owner: public IP, ports, domains with DNS verification, automatic re-checks, active/inactive, primary domain, Let's Encrypt certificates stored in the database, free subdomains (DuckDNS, deSEC), Cloudflare Tunnel, reverse proxies.
- Clients connect through domains with normal certificate checks; invites with a domain dropdown; a small web invite page.
- **Lavalink:** several nodes (own or public) with health checks, best-node selection and failover; an audio bridge that receives Lavalink's audio.
- **Built-in music player per voice channel:** search, queue with drag and drop, Spotify-like controls, synced for everyone, personal music volume.
- **Music panel** in the right-side panel, a Music button next to Camera and Screen share, and a now-playing card in the users list.
- **Chat commands** (`/play`, `/pause`, `/skip`…) with a small command framework.
- Permissions and limits.

**Later (keep extension points)**
Lyrics, audio effects, autoplay/radio, saved playlists, listening statistics, a relay (TURN) so voice works on servers without reachable UDP, commands from third-party bots.

---

## 2. Changes to earlier plans

| Area | Change |
|---|---|
| TLS and trust (Phase 1 §1, §9.2) | Connections through a verified domain with a CA-signed certificate are checked like a website (certificate chain + host name). Connections by IP keep fingerprint pinning (TOFU) (§7.5) |
| Invite links (Phase 1 §9.2) | Domain links `opencord://<domain>[:port]/invite/<code>` without fingerprint, plus web links `https://<domain>[:port]/invite/<code>`; IP links unchanged (§7.6) |
| `Ready` (Phase 1 §5.2) | Gains `server_addresses` (§7.5), `commands` (§17) and the music state needed for the now-playing card |
| Messages (Phase 1 §7) | Messages gain `kind` (`default`, `command_result`) and `interaction { user_id, command }`; ephemeral messages that only the sender sees (§17.3) |
| Permissions (Phase 1 §6.1, Phase 2 §5.1) | New bits 28–31 (§16.1) |
| Voice node (Phase 2 §3, §6) | Audio bridge: WebSocket on the main TLS port, audio on UDP 7712 (§13); a music stream per voice channel (not a participant) |
| Voice gateway (Phase 2 §4.3) | `MusicStream` message; `MediaSinkWants` music entry (§18.4) |
| Desktop UI plan §4.2 (voice panel) | Music button directly right of Camera and Screen (§19.1) |
| Desktop UI plan §4.10 (voice view) | Music button directly right of Camera and Screen share in the control bar (§19.1) |
| Desktop UI plan §4.7 (right-side users list) | Modes **People · Music** (plus Chat in voice channels); a now-playing card at the top of the users list, and only there (§19.2–19.3) |
| Desktop UI plan §4.6 (composer) | `/` opens command autocomplete (§19.6) |
| Desktop UI plan §4.11 (invite dialog) | Domain dropdown when more than one domain is active (§7.6) |
| Desktop UI plan §8.2 (server settings) | New pages: **Domains & network** (owner only), **Commands**; the Music page gains Lavalink nodes |
| Desktop UI plan §2.1 (colours) | Album artwork keeps its colours, like emoji |
| Deploy (Phase 2 §5.5) | Ports per §5.2; `docs/domains.md` and `docs/music.md` |

Earlier Phase 3 drafts are replaced: no compact popover, no bottom-right music chip, no now-playing line in the left sidebar, no separate bridge TCP port.

---

# Part A — Domains and certificates

## 3. Why domains, and who manages them
- A domain lets people connect with a name instead of an IP address, gives the server a trusted certificate (no fingerprint prompt on first connect), and is **required for public Lavalink nodes**, which only connect to servers with trusted certificates.
- **Only the server owner** manages domains, certificates and network settings, on a new page: **Server settings → Domains & network**.
- The page works like the domain tab of Dokploy or similar deployment tools: add a domain, see the DNS records to create, check it, get a certificate, set it as primary.

## 4. Domain types

| Type | Use it when | DNS | Certificate | Voice and music audio (UDP) |
|---|---|---|---|---|
| **Own domain** | You have a domain and it can point to this server | A/AAAA record → this server's IP, checked with DNS | Let's Encrypt from the domain item, or an uploaded certificate | Directly to the server |
| **Free subdomain** | You have no domain | Managed by Opencord through DuckDNS or deSEC | Let's Encrypt, automatic (DNS challenge) | Directly to the server |
| **Cloudflare Tunnel** | The server can't accept incoming connections (no port forwarding, CGNAT) | Managed by Cloudflare | Cloudflare's | **Not through the tunnel** (§9.2) |
| **Reverse proxy / other tunnel** | You run Caddy, Nginx, Traefik, or a tunnel such as ngrok or Tailscale Funnel | Points to the proxy | Handled by the proxy | Directly to the server |

---

## 5. Public address and ports (Domains & network → Network)

### 5.1 Public IP
- Detected at start-up and every 6 hours from `https://api.ipify.org` (IPv4) and `https://api6.ipify.org` (IPv6, only if the server has IPv6).
- The owner can override it with fixed addresses, or turn automatic detection off (for example to avoid contacting ipify).
- When the IP changes: re-check every domain right away (§6.4), update free subdomains (§9.1), and notify the owner.

### 5.2 Ports

| Port | Protocol | Used for | Must be reachable from |
|---|---|---|---|
| 7710 (configurable) | TCP | Everything over TLS: chat, voice signalling, media files, web invite pages, and Lavalink's connection to the audio bridge | Users, Lavalink nodes |
| 7711 | UDP | Voice and video | Users |
| 7712 | UDP | Music audio from Lavalink | Lavalink nodes |
| 80 | TCP | Let's Encrypt HTTP challenge, only while a certificate is requested or renewed | The internet |
| 7719 | TCP, local only | Plain local listener for tunnels and reverse proxies (§9.2–9.3); off unless such a domain exists | The same machine |

- The main port can be set to 443, so addresses and invite links need no port number.
- The page shows each port with its state (listening, in use by another program, mapping on router).
- **Optional: "Open ports on my router automatically (UPnP)"**, off by default, for home servers. Uses UPnP IGD (`igd-next`), shows the result per port, renews mappings and removes them on shutdown.
- Binding ports below 1024 (80, 443) on Linux needs `CAP_NET_BIND_SERVICE` or a Docker port mapping; `docs/domains.md` explains both.

---

## 6. Domains (Domains & network → Domains)

### 6.1 Adding a domain
1. **Add domain** opens a two-step dialog:
   - **Step 1:** host name (for example `chat.example.com`; internationalised names are converted to punycode), type (§4), public port (default: the main port; 443 for tunnel and reverse-proxy types).
   - **Step 2:** the exact DNS records to create, with copy buttons: `A  chat.example.com → 203.0.113.10` and, if the server has IPv6, `AAAA → 2001:db8::10`. If the domain uses Cloudflare DNS, a note says to set the record to **DNS only (grey cloud)**: Cloudflare's proxy only forwards web traffic on a few ports and no UDP.
2. The domain is saved as **Pending verification**. Up to 20 domains per server.

### 6.2 Checking a domain
Each domain row has a **✓ Check** button that runs the check right away. The row shows "Checking…" and then the result inline; there's no extra dialog.

**DNS check (own domains and free subdomains)** — plain DNS packets over UDP port 53, built with `hickory-proto` and sent with a normal UDP socket (not the operating system's resolver and not DNS-over-HTTPS):
1. Find the domain's authoritative name servers (NS query through public resolvers; defaults 1.1.1.1, 8.8.8.8 and 9.9.9.9, changeable in Advanced for networks that block them).
2. Ask the authoritative servers directly for A and AAAA (recursion off), following CNAMEs up to 8 steps. Also ask one public resolver, to warn about stale caches.
3. **Pass** if the records contain the server's public IPv4 address (and its IPv6 address, if it has one).
- 2 s timeout per query, 3 attempts; truncated answers are retried over TCP.

**HTTP check (tunnel and reverse-proxy types)** — DNS points at Cloudflare or the proxy, not at this server, so Opencord requests `https://<domain>/.well-known/opencord/verify` and expects a random token it serves itself. If it fails because the router can't reach its own public address (missing "hairpin NAT"), the result says so, and the owner can confirm the domain manually.

**Results shown on the row:**

| Result | Meaning |
|---|---|
| Verified | Points to this server |
| Points to another address | Shows which IP it points to |
| No record | No A/AAAA record found |
| Proxied by Cloudflare | The record resolves to Cloudflare's proxy addresses; turn the proxy off |
| Partly correct | For example IPv4 correct, IPv6 different |

Only verified domains can be activated. The Check button can be used once every 10 s.

### 6.3 Active, inactive and primary
- Each verified domain has an **Active** switch.
- Exactly one active domain is the **primary** (star on the row). The first verified domain becomes primary automatically. The primary is used by default for invites, shown as the server's address, and used for the Lavalink audio bridge (§13.1).
- **Deactivating** a domain: it's no longer offered in invites; connections through it keep working for 7 days, and clients are told to switch (§7.5); after that its certificate is no longer served. The primary can only be deactivated after another domain is made primary.
- **Delete** removes the domain and its certificate, after confirmation.
- Each row shows how many users connected through it in the last 24 hours, to help decide what to deactivate.

### 6.4 Automatic re-checks
- Every **12 hours** (with up to 30 minutes of random delay), at start-up, and right after the public IP changes.
- A failed check marks the domain **DNS problem since <time>** with the reason, and notifies the owner (toast and a banner on the page).
- The domain **stays active**: a short DNS problem shouldn't break connections. Certificate renewal waits until DNS is correct again.
- Each domain keeps its last 10 check results, shown from its menu.

### 6.5 Domain row (summary)
Host name · type tag · status (shape + label: Pending, Verified, DNS problem, Inactive) · certificate status ("Valid until 12 Jan", "No certificate", "Renewal failed", "Provided by Cloudflare") · primary star · Active switch · ✓ Check · "⋯" menu: Get certificate / Renew now, Set as primary, Copy server address, Copy invite link, Check history, Delete.

---

## 7. Certificates and how clients use domains

### 7.1 Getting a Let's Encrypt certificate
- **Get certificate** on a verified own-domain row. Free subdomains get one automatically; tunnel and reverse-proxy domains don't need one.
- **The first time**, a dialog asks for an email address (**optional**) and agreement to Let's Encrypt's subscriber agreement. Let's Encrypt no longer sends expiry reminder emails, and Opencord renews automatically, so the email is only for account recovery and important notices. It can be changed later.
- Progress shows on the row step by step: account → order → challenge → validation → issued.
- ACME client: `instant-acme`; private keys and the certificate request with `rcgen`.

### 7.2 Challenge type (chosen automatically)
1. **HTTP-01:** Opencord answers on port 80, only for `/.well-known/acme-challenge/` paths, only while validating. Needs port 80 free and reachable from the internet.
2. **TLS-ALPN-01:** used instead when Opencord itself runs on port 443.
3. **DNS-01:** used when the owner has connected a DNS provider in Domains & network → DNS providers: a Cloudflare API token with DNS edit permission for the zone, or DuckDNS/deSEC tokens for free subdomains. Needs no open ports at all.

Errors are explained in plain words, for example: "Port 80 isn't reachable from the internet. Open or forward it, or connect your DNS provider to use the DNS challenge", "The domain doesn't point to this server", "Let's Encrypt's limit was reached; try again after <time>". After 5 failed attempts in an hour the button waits, with a countdown. **Advanced:** "Use Let's Encrypt's test environment" for trying the setup without hitting limits; test certificates are labelled "Test".

### 7.3 Storage, serving and renewal
- The certificate chain and private key are **stored in the database** (`domain_certificates`). The private key is encrypted with the server's `secrets.key` (XChaCha20-Poly1305), like Lavalink passwords. The ACME account key is stored the same way.
- The TLS listener picks the certificate by host name (SNI, a rustls certificate resolver). Connections by IP or unknown names get the self-signed certificate. New and renewed certificates take effect immediately, without a restart.
- **Renewal:** checked daily. Renew when a third of the lifetime remains, or earlier if Let's Encrypt's renewal information (ARI) asks for it. Retries with backoff; the owner is notified if a certificate gets within 7 days of expiring. The row shows "Valid until <date> · renews automatically".

### 7.4 Uploading a certificate
For owners with their own CA or a wildcard certificate: upload a PEM chain and private key. The row shows the expiry date and warns 14 days before; there's no automatic renewal.

### 7.5 How clients use domains
- `Ready` gains `server_addresses { primary, active_domains, ip_address }`, and changes arrive as `ServerAddressesUpdate`.
- Clients save every address of a server, connect to the primary first, and try the others if it fails (other domains, then the IP). When the primary changes or a domain is deactivated, the saved entry updates by itself.
- A server reached through different addresses is recognised by its `server_id` (from `Hello`) and stays one entry in the server list.
- **TLS:** domains with a CA-signed certificate (Let's Encrypt, uploaded, Cloudflare, proxy) are checked like a website: certificate chain and host name, using the operating system's trust store (`rustls-platform-verifier`). No fingerprint prompt, and renewals cause no warnings. Connections by IP keep fingerprint pinning (TOFU).

### 7.6 Invites with domains
- The invite dialog shows a **domain dropdown only when more than one domain is active**; it defaults to the primary. With one active domain, that one is used without a dropdown. With none, the IP-based link with fingerprint is used, as before.
- Link formats: `opencord://chat.example.com/invite/CODE` (port added only when it isn't 7710) and a web link `https://chat.example.com:7710/invite/CODE`, served on the main port (the port is left out when the server runs on 443 or behind a tunnel or proxy).
- **Web invite page:** a tiny server-rendered page (no JavaScript framework, under 20 KB, monochrome): server name, member count, an **Open in Opencord** button (the `opencord://` link) and a download link.
- Invites made with a domain that's later deactivated stop working; the page says the invite is no longer valid.

---

## 8. Domains & network page layout (owner only)
Sections, top to bottom, built from the existing settings sections and rows:
1. **Public address:** detected IPv4/IPv6, "Detect again", manual override, automatic detection on/off.
2. **Domains:** the rows from §6.5, "Add domain", "Get a free domain" (§9.1), "Use a tunnel" (§9.2).
3. **Network:** the port list from §5.2 with states, the UPnP switch, the main port.
4. **DNS providers:** Cloudflare API token, DuckDNS token, deSEC token (shown masked, with Replace).
5. **Advanced:** DNS resolvers for checks, Let's Encrypt test environment, local proxy listener port, trusted proxy addresses.

Long operations (checks, certificates, tunnels) run on the server and report progress through events; the page never blocks.

---

## 9. No domain? Free subdomains, tunnels and proxies

### 9.1 Free subdomain (recommended when the server can accept incoming connections)
- Providers: **DuckDNS** (`name.duckdns.org`) and **deSEC** (`name.dedyn.io`). Both are free, give an API token, and support the DNS challenge for Let's Encrypt.
- **Get a free domain:** pick a provider → a link to create an account and subdomain there → paste the token → enter the subdomain. Opencord then points the record at the server's IP, verifies it, gets a Let's Encrypt certificate with the DNS challenge, and **keeps the IP updated** whenever the server's public IP changes (dynamic DNS).
- Voice and music work normally because UDP goes straight to the server. The server's ports must be reachable (port forwarding at home, or UPnP from §5.2).

### 9.2 Cloudflare Tunnel (when the server can't accept incoming connections)
- **Quick tunnel** (no Cloudflare account): Opencord starts `cloudflared` and gets a random `*.trycloudflare.com` address. **For testing only**: the address changes every time the tunnel restarts, Cloudflare allows at most 200 requests in flight, and there's no uptime guarantee. The page states these limits next to the address.
- **Named tunnel** (free Cloudflare account and a domain on Cloudflare): the owner creates a tunnel in the Cloudflare dashboard with a public hostname pointing to `http://localhost:7719` (Opencord's local listener) and pastes the tunnel token into Opencord. Opencord runs `cloudflared tunnel run` with that token as a supervised process (restarted on crash, logs viewable), or detects one the owner runs separately, for example as a Docker sidecar.
- `cloudflared` binary: use the one installed on the system, or download Cloudflare's official release on request and verify its checksum.
- Cloudflare provides the trusted certificate and supports WebSockets, so chat, voice signalling, web invites and Lavalink's connection to the audio bridge all work through the tunnel.
- **Limitation, shown clearly on the page:** tunnels don't carry UDP. Voice and video (7711) and music audio from Lavalink (7712) still need UDP to reach the server directly.
  - If the server has no reachable UDP at all, text works fully.
  - Music still works with a Lavalink on the same machine or network, because its UDP stays local.
  - Voice needs a UDP path; a relay (TURN) is planned for later.

### 9.3 Reverse proxy or other tunnel
- For Caddy, Nginx, Traefik, ngrok, Tailscale Funnel and similar. The proxy handles TLS and forwards to Opencord's local listener (or the main port).
- The page shows ready-made snippets for Caddy and Nginx, including WebSocket headers and long timeouts.
- Verified with the HTTP check (§6.2). Opencord trusts `X-Forwarded-For` only from the configured proxy addresses.
- Same UDP note as tunnels: 7711 and 7712 must reach the server directly.

---

## 10. Domains: data and protocol
- **Tables:**
  - `domains (id, hostname, type, public_port, status, active, is_primary, last_checked_at, last_result, created_at)`
  - `domain_checks (domain_id, checked_at, result, details)`, keeping the last 10 per domain
  - `domain_certificates (domain_id, chain_pem, key_enc, issuer, not_before, not_after, is_test, created_at)`
  - `acme_account (key_enc, email, created_at)`
  - `dns_providers (id, kind, token_enc, created_at)`
  - `server_meta` keys: detected and overridden public IPs, ipify on/off, DNS resolvers, UPnP, main port, local listener, trusted proxies, tunnel settings (token encrypted)
- **Requests (owner only):** `AddDomain`, `UpdateDomain` (active, primary, port), `DeleteDomain`, `CheckDomain`, `RequestCertificate { domain_id, email? }`, `UploadCertificate`, `SetPublicAddress`, `DetectPublicAddress`, `SetDnsProvider`, `StartTunnel`, `StopTunnel`, `UpdateNetworkSettings`.
- **Events:** `DomainStatus` and `CertificateProgress` (to the owner while the page is open), `ServerAddressesUpdate` (to everyone).
- **Errors:** `DOMAIN_INVALID`, `DOMAIN_EXISTS`, `DOMAIN_LIMIT`, `DOMAIN_NOT_VERIFIED`, `DNS_MISMATCH`, `ACME_FAILED`, `ACME_RATE_LIMITED`, `PORT_UNREACHABLE`.

---

# Part B — Music with Lavalink

## 11. How Lavalink fits

Lavalink was built for Discord bots. It streams audio to **Discord's voice servers** using Discord's voice protocol and has no API to send audio anywhere else. To use an unmodified Lavalink, the Opencord voice node **acts as a Discord-compatible voice server** for it: the *audio bridge*.

```
Client ──(main gateway: search, queue, controls, commands)──► Opencord server ──(REST + WebSocket, password)──► Lavalink
                                                                  │
                                                                  │ player voice state:
                                                                  │ { token, endpoint = <bridge domain>:<port>, sessionId, channelId }
                                                                  ▼
Lavalink ──(WSS on Opencord's main TLS port, Discord voice protocol v8)──► Voice node: audio bridge
Lavalink ──(UDP 7712, RTP + Opus, transport-encrypted)──────────────────► audio bridge ──► SFU ──► listeners
                                                                                          (the channel's music stream)
```

1. Opencord creates a Lavalink player and gives it a "voice server" address that points at the bridge.
2. Lavalink connects to the bridge as it would to Discord, then sends Opus audio over UDP.
3. The bridge decrypts the transport encryption and forwards the Opus frames, unchanged, to everyone in the voice channel as the channel's **music stream**. Each client plays it in the channel's music player.

Lavalink believes it's a bot in a voice channel; in Opencord there's no bot user, no participant and no "join".

**What this needs, now that Part A exists**
- **A trusted certificate:** Lavalink always uses `wss://` and Java checks the certificate. Any active domain with a trusted certificate works (Let's Encrypt, uploaded, Cloudflare Tunnel, reverse proxy), so **public Lavalink nodes work**. Only IP-only servers still need Opencord's self-signed certificate imported into the Lavalink's trust store, which is only possible for a Lavalink the admin runs.
- **Ports:** the bridge's WebSocket runs on Opencord's **main TLS port** (no extra TCP port). The audio uses **UDP 7712**, which must be reachable from the Lavalink nodes; for Lavalink on the same machine or Docker network it stays internal.
- **End-to-end encryption (DAVE):** Lavalink 4.2+ supports Discord's DAVE encryption. The bridge announces DAVE version 0 (none), so Lavalink sends plain Opus inside the transport encryption. **L0 verifies this.**
- **Alternatives considered:** a custom Lavalink plugin that streams audio directly (admins would have to install it; fallback if L0 fails), or Opencord's own audio extraction with yt-dlp (not chosen: public Lavalink nodes are the reason for using Lavalink).

---

## 12. Lavalink nodes (server)

### 12.1 Adding nodes
Admins add **any number of Lavalink nodes** (up to 20), their own or public ones, in **Server settings → Music → Lavalink nodes** (needs `MANAGE_SERVER`). Opencord always uses the best available node and switches automatically when one goes down (§12.5–12.7).

| Field | Meaning |
|---|---|
| Name | Shown in settings and logs, e.g. "Home server", "Public EU" |
| Host, port, TLS | Where Opencord reaches the node |
| Password | Lavalink's `Authorization` password |
| Priority | **Preferred**, **Normal** (default) or **Backup**. Backup nodes are used only when no Preferred or Normal node is available |
| Max players | Optional cap on how many Opencord channels this node plays for at once |
| Strict address check | On by default: the bridge only accepts this node's IP addresses. Turn off for public nodes whose addresses change |
| Enabled | Off = never used; existing players are moved away |

- Passwords are encrypted at rest with `secrets.key` and never sent to clients; the UI shows "••••••" with a Replace button.
- Database: `lavalink_nodes (id, name, host, port, secure, password_enc, priority, max_players, strict_address, enabled, draining, created_at)`.
- Every enabled node keeps its own WebSocket connection and session, so a spare node is ready the moment it's needed.

### 12.2 Connection
- WebSocket `/v4/websocket` with headers `Authorization` (password), `User-Id` (the internal Lavalink user ID, §12.3) and `Client-Name: Opencord/<version>`. The `ready` op returns the Lavalink `sessionId`.
- Enable session resuming right after connecting (`PATCH /v4/sessions/{sessionId}` with `resuming: true`, `timeout: 60`). When reconnecting, send the old ID in the `Session-Id` header, so music keeps playing through short Opencord restarts or network drops.
- REST calls use the same `Authorization` header, a 10 s timeout and retries with backoff. WebSocket reconnects back off from 1 s to 30 s.
- On connect, call `GET /v4/info`: Lavalink version, enabled `sourceManagers`, plugins and filters. Shown in settings and used for the list of search sources.
- Handle `stats` (load), `playerUpdate` (positions) and `event` (track lifecycle) ops.

### 12.3 Internal Lavalink user ID
- Lavalink requires a `User-Id` (a Snowflake). Generate one per Opencord server on first start and store it in `server_meta`.
- It's only an internal number for Lavalink. **Don't create an Opencord user, member or voice participant for music.**

### 12.4 Players
- One Lavalink player per voice channel that has music. Lavalink's `guildId` and the voice state's `channelId` are both the voice channel ID, so different channels can play different music at the same time, possibly on different nodes.

### 12.5 Node health

| State | Meaning |
|---|---|
| **Healthy** | WebSocket connected, REST answering, load normal |
| **Degraded** | Connected but struggling: CPU load above 85 %, more than 2 % of audio frames missing or late (`frameStats.deficit` / `nulled`), REST latency above 1 s, or repeated track-loading failures in the last 5 minutes |
| **Unavailable** | WebSocket closed and not back within 3 s, REST failing, or password rejected |
| **Draining** | Set by an admin before maintenance: gets no new channels, keeps its current ones until they move |
| **Disabled** | Turned off by an admin |

How problems are noticed quickly:
- **WebSocket:** a closed connection is noticed at once; a ping every 5 s catches connections that hang silently.
- **Load:** `GET /v4/stats` every 15 s (the `stats` op only arrives once a minute), plus frame statistics from the `stats` op. The REST round trip is the node's ping.
- **Audio watchdog:** if a channel is playing but the bridge receives nothing from its node for 2 s, Opencord checks the node (`GET /v4/info`, 2 s timeout). No answer → Unavailable. An answer → only that player is restarted on the same node.
- **No flapping:** a node that was Unavailable must stay healthy for 30 s before it gets new channels again.

### 12.6 Choosing the best node
The same rule is used for a channel's first track, for failover and for searches:
1. **Candidates:** Healthy nodes that aren't draining, are under their max players, and **can play the track's source** (from `/v4/info`: a Spotify track needs LavaSrc, YouTube needs the YouTube source). Degraded nodes only when no Healthy node fits.
2. **Priority:** the best tier with a candidate: Preferred, then Normal, then Backup.
3. **Lowest penalty** inside that tier, using the formula common in Lavalink clients:
   - playing players
   - \+ CPU: `1.05^(100 × systemLoad) × 10 − 10`
   - \+ missing frames: `1.03^(500 × deficit / 3000) × 600 − 600`
   - \+ empty frames: `(1.03^(500 × nulled / 3000) × 300 − 300) × 2`
   - \+ Opencord's own terms: 50 per track-loading failure in the last 5 minutes, and REST ping in ms ÷ 10
4. **Ties:** fewer Opencord channels on the node, then lower ping.

- **Sticky:** a channel stays on its node for the whole listening session.
- **Between tracks:** if a channel's node becomes Degraded or Draining, the channel moves at the next track change, so nobody hears a cut.
- **Searches** go to the best node with the requested source, with one retry on the next best node. Results can play on any node with the same source (Lavalink's encoded tracks aren't tied to one node).
- The source list in the player only offers sources available on at least one healthy node.

### 12.7 Failover
When a node becomes Unavailable:
1. For each channel playing on it, pick the best node (§12.6), excluding the failed one.
2. Create the player there with the same track (`track.encoded`) and the estimated position (last `playerUpdate` + time since), with a new bridge token. The old token is revoked, so late audio from the old node is rejected.
3. Move channels 5 at a time, recalculating penalties after each move.
4. The player shows "Switched to another music server".

- **Target:** music continues within 5 s of a node crash, within 2 s of the old position.
- **Track can't move:** if no healthy node has the track's source, skip to the next playable track with a notice.
- **No node left:** pause, keep the queue, show "Music server unavailable — retrying", and continue at the saved position once a suitable node is healthy.
- **The failed node comes back:** destroy any old players still on it, so nothing plays twice.
- **Maintenance:** Drain (channels move at their next track change), Move players now (immediate), Disable and Delete (move players first).
- **Opencord restarts** use Lavalink session resuming (§12.2) and keep music on the same nodes.

### 12.8 Node list in settings
- Header summary: "3 nodes · 2 healthy · 7 channels playing".
- **Bridge checklist** at the top, so admins see what Lavalink needs: "Lavalink connects to `wss://chat.example.com:7710` ✓ trusted certificate · sends audio to UDP `203.0.113.10:7712` ✓ listening · router mapping ✓ / not used".
- One row per node: status shape and label, name, `host:port`, Lavalink version, Opencord channels / total players, CPU, ping, sources, last error.
- Row actions: Test, Edit, Drain / Undrain, Move players now, Enable / Disable, Delete.
- **Node events** (last 50): connected, disconnected, degraded, recovered, "moved 3 channels from Home server to Public EU".
- If every node is unavailable: a banner on the Music settings page and one toast for members with `MANAGE_SERVER`.

### 12.9 Test connection
The **Test** button runs three steps, each shown with ✓ or ✕:
1. REST reachable and password correct (`GET /v4/info`).
2. WebSocket `ready` received.
3. **Audio bridge:** a temporary player whose voice state points at the bridge; the bridge must see Lavalink's Identify and UDP IP discovery within 10 s; then the player is destroyed.

Failures explain the fix, for example: "Lavalink couldn't verify Opencord's certificate. Activate a domain with a certificate in Domains & network" or "Lavalink didn't send audio to UDP 7712. Open the port for this node."

---

## 13. Audio bridge (voice node)

### 13.1 Address
- **Bridge domain:** the primary domain by default; another active domain with a trusted certificate can be chosen in Music settings. The voice state's `endpoint` is `<bridge domain>:<public port>` (for tunnel and proxy domains, port 443).
- **Lavalink in the same Docker Compose project:** give the Opencord service a network alias equal to the bridge domain, so Lavalink resolves the domain to Opencord inside Docker and the certificate still matches.
- **External voice nodes (Phase 2 §3.4):** the bridge runs on the node hosting the channel, so that node needs its own public address and certificate, configured on the node.

### 13.2 Protocol
The main TLS listener (and the local proxy listener used by tunnels and proxies) routes WebSocket upgrades on `/` that carry a `v` query parameter (`wss://<domain>:<port>/?v=8`) to the bridge. Everything else on those listeners is unchanged. The bridge implements the **server side of Discord's voice protocol, version 8**, limited to what Lavalink uses:

| Step | Message | Bridge behavior |
|---|---|---|
| 1 | WebSocket upgrade on `/?v=8` | Also accept `v=4` |
| 2 | Hello (op 8) | Send `heartbeat_interval` (e.g. 13 750 ms) |
| 3 | Identify (op 0): `server_id`, `user_id`, `session_id`, `token`, `max_dave_protocol_version` | `server_id` must be the voice channel ID, `user_id` the internal Lavalink user ID; `token` and `session_id` must match the player the main server announced (single-use, expires after 60 s) |
| 4 | Ready (op 2): `ssrc`, `ip`, `port`, `modes` | `ip`/`port`: the server's public IP and UDP 7712. `modes`: `aead_aes256_gcm_rtpsize`, `aead_xchacha20_poly1305_rtpsize` (the second is mandatory) |
| 5 | UDP IP discovery (74-byte request, type `0x1`) | Reply with type `0x2` and Lavalink's address and port as seen by the bridge |
| 6 | Select Protocol (op 1): `protocol: "udp"`, address, port, mode | Remember Lavalink's UDP address and the chosen mode |
| 7 | Session Description (op 4): `mode`, 32-byte `secret_key`, `dave_protocol_version: 0` | Random key per connection |
| 8 | Speaking (op 5) | Lavalink announces its SSRC before sending audio |
| — | Heartbeat (op 3, `{t, seq_ack}`) → Heartbeat ACK (op 6) | v8 sequence numbers on server messages for buffered resume |
| — | Resume (op 7) → Resumed (op 9) | Re-send buffered messages; if the buffer can't cover the gap, close so Lavalink reconnects |

- Ignore opcodes Lavalink doesn't need; reject DAVE opcodes (21–31) because the negotiated version is 0.
- Use Discord-compatible close codes (e.g. 4004 authentication failed, 4006 session no longer valid, 4014 disconnected), so Lavalink reports clear `WebSocketClosedEvent`s.
- Rust: `tokio-tungstenite` on the existing rustls listener; `aes-gcm` and `chacha20poly1305` for decryption.

### 13.3 Audio
- RTP with the `rtpsize` modes: the RTP header (including any extension preamble) is unencrypted, and a 32-bit nonce is appended to the packet. Strip the nonce, decrypt, then drop the header extension body if present. The result is Opus: 48 kHz, stereo, 20 ms frames.
- Drop packets from unknown addresses or SSRCs, packets that fail decryption and anything over 1,500 bytes. Handle sequence-number wrap.
- Forward the Opus frames **unchanged** into the SFU as the channel's music stream, announced with `MusicStream` (§18.4).
- Send it to everyone connected to the channel, except deafened listeners and listeners who muted music. Priority: below voice, above video.
- When Lavalink pauses or stops it stops sending; listeners treat it as a silence gap.
- Metrics: packets received, decrypt failures, jitter, bridge connections.

### 13.4 Access control
- Every player gets its own random `token` and `session_id`; for external voice nodes the main server sends `ExpectMusicBridge { channel_id, token, session_id, lavalink_addresses }` over the control channel.
- The bridge accepts audio for a channel only from the node that currently plays it; on failover the old token is revoked.
- With **Strict address check** (§12.1), WebSocket and UDP are accepted only from the node's resolved IP addresses (re-resolved every 5 minutes). Behind a tunnel or proxy, the WebSocket's real source comes from trusted `X-Forwarded-For` (§9.3).
- UDP audio is accepted only from the address that completed IP discovery for a valid session.
- Rate-limit connection attempts per source address.

### 13.5 Configuration (`opencord.toml`)
```toml
[music]
enabled = true
audio_udp_port = 7712
bridge_domain = "primary"
```

| Key | Meaning |
|---|---|
| `audio_udp_port` | UDP port for music audio from Lavalink |
| `bridge_domain` | `primary`, or a specific active domain with a trusted certificate. Also changeable in Music settings |

---

## 14. Music sessions (server)

### 14.1 Model
One `MusicSession` per voice channel, run as a single actor so all actions for a channel (UI, commands, Lavalink events) are applied in order.

| Field | Meaning |
|---|---|
| `now_playing` | Current `QueueItem` or none |
| `queue` | Upcoming items (max 500) |
| `history` | Last 50 played items |
| `state` | `idle`, `loading`, `playing`, `paused` |
| `anchor` | `position_ms` at `server_time_ms`, for progress sync |
| `repeat` | `off`, `queue`, `track` |
| `shuffle` | on/off, plus the original order so it can be restored |
| `skip_votes` | Users who voted to skip the current item |
| `version` | Increases with every change |

`QueueItem { item_id, track metadata, encoded track (server only), requester_id, added_at }`.

Saved (debounced, at most every 2 s) to `music_sessions (channel_id, state_json, updated_at)`, so the queue survives an Opencord restart.

### 14.2 Playback rules
- **Adding** when idle plays immediately; otherwise the track goes to the end. "Play next" puts it first in the queue; "Play now" replaces the current track (which goes to history).
- **Track end** (`finished`): next item according to repeat and shuffle. **Failures** (`loadFailed`, `TrackExceptionEvent`, `TrackStuckEvent`): notice, then the next item; stop after 3 failures in a row.
- **Previous:** more than 5 s into the track → restart it; otherwise play the most recent history item and put the current one back at the front of the queue.
- **Repeat queue:** finished items go to the end. **Repeat track:** the current item replays.
- **Shuffle on:** randomize the upcoming order and remember the original. **Off:** restore the original order of the remaining items.
- **Seek** only for seekable tracks. Live streams show "LIVE".
- **Everyone leaves:** pause at once; destroy the Lavalink player after 2 minutes (setting); keep the queue for 30 minutes so music resumes if someone comes back.
- **Queue ends:** idle; destroy the player after 5 minutes (setting).
- **Volume:** no shared volume. The Lavalink player stays at 100 %, so Lavalink never re-encodes audio for volume. Each user has a personal music volume (§19.8).
- **Moves are relative:** `MusicQueueMove { item_id, before_item_id? }` places an item before another one (or at the end), so two people dragging at once never corrupt the order.
- **No double actions:** next, previous, seek, remove and vote carry an `expected_item_id`; if the current item already changed, the server ignores the action and returns `MUSIC_STALE`.
- Lavalink's `track.userData` carries `{ item_id, channel_id }` to map events back.

### 14.3 Sync to clients
- **Subscribing:** users connected to the voice channel are subscribed automatically; others who can see the channel subscribe when they open its music panel (read-only).
- **Snapshot then updates:** `MusicSnapshot { version, full state }`, then `MusicUpdate { version, changes }` (now playing, playback anchor, queue operations by item ID, modes, votes, notices). A version gap → request a new snapshot.
- **Progress:** anchors from Lavalink's `playerUpdate`, stamped with the Opencord server's receive time and refreshed after every play, pause and seek. Clients interpolate with the server clock offset (`HeartbeatAck` gains `server_time_ms`), subtract their playout delay for the music stream so the bar matches what they hear, and correct drift above 1 s.
- **Now-playing summary:** `MusicNowPlaying { channel_id, title, artist, artwork_id, playing }` to everyone who can see the channel, for the now-playing card (§19.3).

### 14.4 Notices
Short activity messages in the music panel: "Kai added 3 tracks", "Now playing: …", "Couldn't play '…', skipped". Errors and your own actions also show as toasts.

---

## 15. Search, links and artwork

### 15.1 Search
- `MusicSearch { channel_id, query, source }`. The server maps the source to a Lavalink prefix and calls `GET /v4/loadtracks`: `ytmsearch:` (YouTube Music), `ytsearch:` (YouTube), `scsearch:` (SoundCloud), and plugin prefixes when installed and enabled, e.g. `spsearch:` (Spotify through LavaSrc).
- Up to 15 results, from sources the admin enabled and at least one healthy node offers.
- Results are cached on the server per user for 15 minutes and returned with opaque `result_id`s. **Clients never send encoded tracks or raw Lavalink identifiers.**
- Rate limit 10 searches per 10 s per user; queries up to 200 characters.

### 15.2 Links
- Pasting a link works when **Allow links** is on: `https` only, host from an enabled source (`youtube.com`, `youtu.be`, `music.youtube.com`, `soundcloud.com`, `open.spotify.com`, …). IP addresses, `localhost` and private hosts are rejected.
- Tracks with `sourceName` `http` or `local` are refused unless the admin explicitly allows them (they let users make Lavalink fetch any URL or file).
- Playlists show as one card with "Add all", up to the playlist limit (default 100).

### 15.3 Track information
- Title, artist (Lavalink's `author`), duration or "LIVE", artwork, who added it, and the link (`uri`, opened through the link confirmation).
- **Source** as a text pill: "YouTube", "SoundCloud", "Bandcamp", "Twitch", "Vimeo", "Spotify", "Apple Music", "Deezer", otherwise the capitalised `sourceName`. No brand logos.
- **Spotify, Apple Music and Deezer tracks** (through LavaSrc) show their source for the track information, but the sound comes from a matched copy, usually on YouTube.

### 15.4 Artwork proxy
- Clients load artwork from Opencord, never directly from YouTube or other sites.
- `GET /media/music/artwork/{artwork_id}?size=128|512` (media token from Phase 2 §4.4). The server maps the ID to the `artworkUrl` it got from Lavalink, fetches it (https only, public hosts only, at most 5 MB, 5 s timeout), re-encodes it without metadata, resizes it and caches it on disk (7 days, 200 MB, least recently used evicted).
- No artwork → a monochrome music-note placeholder.

---

## 16. Permissions and limits

### 16.1 Permission bits

| Bit | Name | Allows | Default for `@everyone` |
|---|---|---|---|
| 28 | `MUSIC_ADD` | Search and add tracks (starts playback when idle); remove your own queued tracks; skip your own track while it plays | on |
| 29 | `MUSIC_CONTROL` | Pause/resume, seek, next, previous, repeat, shuffle | on |
| 30 | `MUSIC_MANAGE_QUEUE` | Drag and reorder the queue, remove anyone's tracks, clear the queue, "Play next" and "Play now" | off |
| 31 | `MUSIC_MANAGE` | Stop the player, block members from music, bypass music limits, control music without being in the channel, music settings | off |

- Domains & network: **owner only**. Lavalink nodes and the Commands page: `MANAGE_SERVER`.
- Channel permission overwrites work as usual (e.g. music only in a "Music" voice channel).
- Adding and controlling music requires being connected to that voice channel, except with `MUSIC_MANAGE`. Anyone who can see the channel can view its player.
- Members without `MUSIC_CONTROL` can **vote to skip** when vote skip is on (threshold default 50 % of listening members).
- **Managing users:** Phase 2's voice moderation stays (server mute, deafen, move, disconnect). `MUSIC_MANAGE` adds music moderation: block a member from music (list in settings, with reason), remove anyone's tracks, stop the player.
- The same permissions apply to the UI and to chat commands (§17). The server enforces everything; the client hides or disables controls with a tooltip.

### 16.2 Music settings (Server settings → Music; `MUSIC_MANAGE`, nodes need `MANAGE_SERVER`)

| Setting | Default |
|---|---|
| Music enabled | on |
| Enabled search sources / default source | Every available source / YouTube Music if available |
| Allow links | on |
| Allow live streams | on |
| Max track length | no limit (10 min · 30 min · 1 h · 3 h) |
| Max tracks per member in the queue | 20 |
| Max tracks added from one playlist | 100 |
| Queue limit | 500 |
| Vote skip | off (threshold 50 %) |
| Stop after everyone leaves | 2 min |
| Disconnect after the queue ends | 5 min |
| Bridge domain | Primary domain |
| Blocked members | list with reason, unblock button |

---

## 17. Chat commands

### 17.1 Command framework
- A small server-side registry of **built-in commands**. Each command has a name, description, options (text, integer, boolean, choice, user, channel; required or optional; optional autocomplete), required permissions and a handler. Music is the first feature to register commands; the framework is ready for more later.
- Commands are a server feature, not a bot: results are posted as messages attributed to the user who ran the command.
- Clients receive the available commands in `Ready` and `CommandsUpdate`, filtered to enabled commands of features that are set up (music commands only when music is configured).
- **The server parses and validates everything.** The client only uses the command specs to build the input UI.
- Using commands requires `SEND_MESSAGES` in the channel where they're typed, plus the permission of the action.

### 17.2 Music commands

| Command | Options | Permission | Reply |
|---|---|---|---|
| `/play` | `query` (text or link, with autocomplete), `next` (yes/no) | `MUSIC_ADD`; `next` needs `MUSIC_MANAGE_QUEUE` | Public |
| `/pause` | — | `MUSIC_CONTROL` | Public |
| `/resume` | — | `MUSIC_CONTROL` | Public |
| `/skip` | — | `MUSIC_CONTROL`; your own track with `MUSIC_ADD`; otherwise a skip vote when vote skip is on | Public |
| `/previous` | — | `MUSIC_CONTROL` | Public |
| `/seek` | `time` (e.g. `1:30`) | `MUSIC_CONTROL` | Public |
| `/stop` | — | `MUSIC_MANAGE`; stops the music and clears the queue | Public |
| `/queue` | `page` | Can see the channel | Only you |
| `/nowplaying` (alias `/np`) | — | Can see the channel | Only you |
| `/remove` | `position` | Your own track with `MUSIC_ADD`; others need `MUSIC_MANAGE_QUEUE` | Public |
| `/move` | `from`, `to` | `MUSIC_MANAGE_QUEUE` | Public |
| `/clear` | — | `MUSIC_MANAGE_QUEUE` | Public |
| `/shuffle` | — | `MUSIC_CONTROL` | Public |
| `/repeat` | `mode`: off, queue, track | `MUSIC_CONTROL` | Public |
| `/volume` | `percent` (0–200) | None: sets **your own** music volume, handled by the client, never sent | Only you |
| `/music` | — | None: opens the music panel (client-side) | — |

- **Target channel:** the voice channel the user is connected to on this server. If they aren't connected: "Join a voice channel to use music commands" (only you). Members with `MUSIC_MANAGE` can add a `channel` option to control another voice channel.
- Commands work in any text channel and in voice channels' chats.
- Commands share the music rate limits with the UI.

### 17.3 Replies
- **Public results** appear as a compact message in the channel where the command was used: "**Kai** used `/play` · Added *Song* — Artist to the queue in General". Stored like other messages, with `kind = command_result` and `interaction { user_id, command }`.
- **"Only you" replies** and errors are **ephemeral**: sent only to the user's session, shown with "Only you can see this · Dismiss", never stored.
- Server setting **Command results: Public / Only to the user** (default Public), to keep busy chats clean.

### 17.4 Commands page (Server settings → Commands, `MANAGE_SERVER`)
- Commands grouped by feature (Music). Each row: command, description, permission summary, an on/off switch, and "Allowed in: All channels / chosen channels".
- The Command results setting from §17.3.

### 17.5 Protocol
- `ExecuteCommand { channel_id, name, options, nonce }` → `CommandResult { message_id?, ephemeral?, error? }`.
- `AutocompleteCommand { channel_id, name, option, value }` → up to 10 choices. Rate limit 5 per second per user; the client cancels outdated requests.
- Events: `CommandsUpdate`, `EphemeralMessage { channel_id, nonce, content }` (only to the sending session).
- Errors: `COMMAND_UNKNOWN`, `COMMAND_DISABLED`, `COMMAND_NOT_ALLOWED_HERE`, `COMMAND_INVALID_OPTIONS`, plus the music errors in §18.5.

---

## 18. Music protocol additions

### 18.1 Main gateway requests (`proto/opencord/v1/music.proto`)

| Request | Permission / notes |
|---|---|
| `MusicSubscribe` / `MusicUnsubscribe { channel_id }` | Can see the channel |
| `MusicSearch { channel_id, query, source }` → `MusicSearchResults { results, playlist? }` | `MUSIC_ADD` |
| `MusicEnqueue { channel_id, result_ids, before_item_id?, placement: END \| NEXT \| NOW }` | `MUSIC_ADD`; a position, `NEXT` or `NOW` need `MUSIC_MANAGE_QUEUE` |
| `MusicPlayback { channel_id, action: PAUSE \| RESUME \| NEXT \| PREVIOUS \| SEEK \| STOP, position_ms?, expected_item_id? }` | `MUSIC_CONTROL`; `NEXT` on your own track needs only `MUSIC_ADD`; `STOP` needs `MUSIC_MANAGE` |
| `MusicSetMode { channel_id, repeat?, shuffle? }` | `MUSIC_CONTROL` |
| `MusicQueueMove { channel_id, item_id, before_item_id? }` | `MUSIC_MANAGE_QUEUE` |
| `MusicQueueRemove { channel_id, item_ids }` | Own items with `MUSIC_ADD`; others need `MUSIC_MANAGE_QUEUE` |
| `MusicQueueClear { channel_id }` | `MUSIC_MANAGE_QUEUE` |
| `MusicVoteSkip { channel_id, expected_item_id }` | Connected, vote skip enabled |
| `MusicBlockMember { user_id, blocked, reason }` | `MUSIC_MANAGE` |
| `UpdateMusicSettings { ... }` | `MUSIC_MANAGE` |
| `CreateLavalinkNode`, `UpdateLavalinkNode`, `DeleteLavalinkNode`, `TestLavalinkNode`, `FetchLavalinkNodes`, `MoveLavalinkPlayers { from_node_id, to_node_id? }` | `MANAGE_SERVER` |

Rate limits: search 10 / 10 s, enqueue 10 / 10 s, playback and queue changes 20 / 10 s per user.

### 18.2 Events
- `MusicSnapshot` and `MusicUpdate` to subscribers; `MusicNowPlaying` to everyone who can see the channel; `MusicSettingsUpdate` to everyone.
- To members with `MANAGE_SERVER` while the Music settings page is open: `LavalinkNodeStatus` and `LavalinkNodeEvent`.

### 18.3 Track object sent to clients
`MusicTrack { title, artist, duration_ms, is_stream, is_seekable, source, source_display_name, uri, artwork_id }`. Queue items add `item_id`, `requester_id`, `added_at`; search results add `result_id`. Encoded tracks are never included.

### 18.4 Voice gateway and internal control
- `MusicStream { channel_id, ssrc, active }` announces the channel's music stream. It's not a participant: no `ClientConnect`, no tile, no speaking indicator.
- `MediaSinkWants` gains a music entry (`{ music: true, paused }`), so a listener who mutes music stops receiving it.
- Internal control channel: `ExpectMusicBridge`, `CancelMusicBridge`, `MusicBridgeStatus`.

### 18.5 Music error codes
`MUSIC_NOT_CONFIGURED`, `MUSIC_NODE_UNAVAILABLE`, `MUSIC_NOT_IN_CHANNEL`, `MUSIC_BLOCKED`, `MUSIC_QUEUE_FULL`, `MUSIC_MEMBER_LIMIT`, `MUSIC_TRACK_TOO_LONG`, `MUSIC_STREAMS_NOT_ALLOWED`, `MUSIC_SOURCE_NOT_ALLOWED`, `MUSIC_LINK_NOT_ALLOWED`, `MUSIC_STALE`.

---

## 19. Music UI (client)
Built with the rules in §0.1.

### 19.1 Music button
- In the **voice panel** (sidebar, bottom-left, "Voice connected") the buttons are: Camera · Screen · **Music** · Soundboard.
- In the **voice view's control bar**: Mute · Deafen · Camera · Screen share · **Music** · Soundboard · (gap) · Disconnect.
- Pressing Music switches the right-side panel to the music panel; pressing it again switches back to the users list. The button is shown active (inverted) while the music panel is open.
- If music isn't set up, the button is hidden, except for members with `MANAGE_SERVER`, who get "Set up music" in the panel.

### 19.2 Right-side panel modes
- The right-side panel (the users list, UI plan §4.7) gets a small segmented header when a voice channel is involved: **People · Music** (and **Chat** for a voice channel's text chat).
- **People** shows "In this voice channel" (the participants) first, then the server members as before.
- The music panel replaces the users list in the same place, at a width of 340 px instead of 264 px (one 120 ms resize, no other animation). On narrow windows, where the right-side panel is an overlay, Music opens the overlay.

### 19.3 Now-playing card (only in the right-side users list)
- At the top of the users list: 48 px cover, title (one line), artist · source (one line), and a static "playing" glyph or "Paused".
- Clicking it opens the music panel. Right-click: Your music volume, Mute music.
- Shown for the voice channel you're looking at, or the one you're connected to.
- **It appears only here:** not in the left sidebar, not in the voice grid, not as a participant.

### 19.4 Music panel
Built like an activity, with the current track in the middle and what's next underneath. From top to bottom:
1. **Header:** "Music · General" and a "⋯" menu: Stop player (`MUSIC_MANAGE`), Music settings, Expand (§19.7).
2. **Cover:** centered, square, up to 240 px, radius 12. Artwork that isn't square is cropped to fill. No blur behind it.
3. **Track:** centered title (`title` style, 2 lines max), artist (`small`), and a centered row with the source pill and "Added by" with a small avatar.
4. **Progress:** 4 px bar with elapsed and total time; drag to seek, sent on release. Disabled with a tooltip for streams, unseekable tracks or missing permission.
5. **Controls:** Shuffle · Previous · **Play/Pause** (44 px inverted circle) · Next · Repeat. Without `MUSIC_CONTROL`, Next becomes **Vote to skip (2/5)** when vote skip is on.
   - **Shuffle** (`shuffle` icon) toggles on and off. On: the upcoming tracks play in random order (§14.2).
   - **Repeat** cycles with each click: **Off** (`repeat` icon, `textMuted`) → **Repeat queue** (`repeat` icon, active) → **Repeat track** (`repeat_one` icon, active) → Off.
   - Active states use `text` colour with a 4 px dot underneath; the tooltip names the current mode. Both are shared: everyone in the channel sees the same shuffle and repeat state.
6. **Your volume:** a speaker button opens one slider (0–200 %) with mute.
7. **Add a song:** a search field with a source pill ("YouTube Music ▾"). While searching, results replace the list below, with "Back to queue".
8. **Up next** (virtualized), with "12 tracks · 47 min" and Clear (`MUSIC_MANAGE_QUEUE`). Rows: 40 px artwork, title, "artist · duration", the requester's avatar, a drag handle (with `MUSIC_MANAGE_QUEUE`) and a "⋯" menu: Play next, Move to top, Move up/down, Remove, Copy link, Open link.
9. **Recently played** link at the bottom shows the last 50 tracks, each with "Add again".

- **Empty state:** placeholder cover, "Nothing playing", and the search field focused.
- **Read-only** (not connected to the channel): controls disabled, "Join the voice channel to control music" with a Join button.

### 19.5 Drag and drop
- **Reorder** Up next by dragging rows; a line shows where the track will land. Dropping at the top makes it play next.
- **Drag a search result or a recently played track** into Up next at any position.
- **Drop on the cover** to play it now.
- All of these need `MUSIC_MANAGE_QUEUE`; without it, drag handles aren't shown and the Add button appends to the end.
- Every drag action has a menu and keyboard alternative (Play next, Move to top, Move up/down).
- When someone else reorders, rows slide to their new place in 120 ms (instantly with reduce motion). Moves are sent as "before this item" (§14.2), so simultaneous drags stay consistent.

### 19.6 Commands in the composer
- Typing `/` at the start of a message opens the command list in the composer's existing autocomplete popup: name, description, and a dimmed entry with the reason when it can't be used ("Join a voice channel first", "Needs Control playback").
- Choosing a command turns it into a chip with its options as inline fields (`/play` `query:`). `Tab` moves between options, `Enter` runs, `Esc` cancels.
- `/play`'s `query` shows live suggestions from music search (after 400 ms of no typing, at most 5, with small artwork); picking one plays exactly that track.
- Ephemeral replies show as a compact muted message with "Only you can see this · Dismiss"; public results as compact command-result messages (§17.3).

### 19.7 Expand (optional, large screens)
"Expand" shows the same music panel content in the voice view's main area, activity-style: large cover and controls in the center, Up next beside it, participant tiles in a strip at the bottom. Same widgets, no extra effects.

### 19.8 Personal music volume and listening
- **One client setting for all servers**, stored locally and never sent. It appears in the music panel, the now-playing card's right-click menu, **User settings → Voice & audio → Music volume**, and `/volume`. Mute is part of the same setting and tells the voice node to stop sending the music stream.
- Optional local setting **"Lower music while people talk"** (off by default): −10 dB while someone speaks, 300 ms release.
- Music goes through the Phase 2 mixer, so echo cancellation removes it from listeners' microphones. Gain: your music volume × ducking, then the existing limiter.

### 19.9 States and errors

| State | What the user sees |
|---|---|
| Music not set up | `MANAGE_SERVER`: "Set up music"; others: Music button hidden |
| Switched to another node | Notice: "Switched to another music server" |
| Every node unavailable | Banner "Music server unavailable, retrying…"; controls disabled; queue kept |
| Not in the channel | Read-only panel with a Join button |
| Missing permission | Disabled controls with a tooltip naming the permission |
| Blocked from music | "You can't use music on this server" |
| Track failed | Notice "Couldn't play '…', skipped" |
| Loading | Skeleton rows in search; spinner only on the Play button |

### 19.10 Rust core API (sketch)
```rust
pub async fn music_subscribe(server_key: String, channel_id: i64) -> Result<MusicSnapshot>;
pub async fn music_unsubscribe(server_key: String, channel_id: i64) -> Result<()>;
pub async fn music_search(server_key: String, channel_id: i64, query: String, source: Option<String>) -> Result<MusicSearchResults>;
pub async fn music_enqueue(server_key: String, channel_id: i64, result_ids: Vec<String>, before_item_id: Option<String>, placement: Placement) -> Result<()>;
pub async fn music_playback(server_key: String, channel_id: i64, action: PlaybackAction, expected_item_id: Option<String>) -> Result<()>;
pub async fn music_set_mode(server_key: String, channel_id: i64, repeat: Option<RepeatMode>, shuffle: Option<bool>) -> Result<()>;
pub async fn music_queue_move(server_key: String, channel_id: i64, item_id: String, before_item_id: Option<String>) -> Result<()>;
pub async fn music_queue_remove(server_key: String, channel_id: i64, item_ids: Vec<String>) -> Result<()>;
pub async fn music_queue_clear(server_key: String, channel_id: i64) -> Result<()>;
pub async fn music_vote_skip(server_key: String, channel_id: i64, expected_item_id: String) -> Result<()>;
pub fn music_set_local_volume(volume: f32, muted: bool, duck_while_talking: bool);
pub async fn music_artwork(server_key: String, artwork_id: String, size: u32) -> Result<String>;
pub async fn command_autocomplete(server_key: String, channel_id: i64, name: String, option: String, value: String) -> Result<Vec<CommandChoice>>;
pub async fn command_execute(server_key: String, channel_id: i64, name: String, options: Vec<CommandOption>) -> Result<CommandOutcome>;
```
- `music_artwork` returns the path of a locally cached file.
- Music, command and address events arrive on the existing core event stream.
- One Riverpod provider per subscribed channel holds the music state.

---

## 20. Security
- Secrets (Lavalink passwords, DNS provider tokens, tunnel tokens, private keys, ACME account key) are encrypted at rest with `secrets.key`, never sent to clients and never logged.
- Domain, certificate and network changes are owner-only and written to the server log.
- DNS checks only send queries; certificate challenges answer only the exact ACME paths; port 80 is open only while validating.
- Clients can only reference server-issued `result_id`s or allow-listed links; the artwork proxy only fetches URLs that came from Lavalink results, from public hosts, with limits.
- The audio bridge checks per-player tokens, optional node addresses and the IP-discovery address (§13.4).
- Commands are parsed and permission-checked on the server; ephemeral replies only go to the sending session.
- Every request is rate-limited, and music limits are enforced on the server.
- **End-to-end encryption (later):** music comes from the server, so it can't be end-to-end encrypted; the music panel must say so in encrypted voice channels, and admins can turn music off there.
- `docs/music.md` notes that admins choose and are responsible for the sources they enable.

---

## 21. Testing and milestones

### 21.1 Testing
- **DNS checks:** a fake DNS server in tests answering with matching, mismatching, CNAME-chained, truncated (TCP fallback), Cloudflare-proxy and missing records, and timeouts.
- **Certificates:** ACME against **Pebble** (Let's Encrypt's test ACME server) in CI for HTTP-01, TLS-ALPN-01 and DNS-01; renewal with short-lived test certificates; SNI picks the right certificate; renewed certificates are served without restart.
- **Clients and domains:** connecting by domain needs no fingerprint prompt; fallback to the next address when the primary fails; one server entry across addresses; deactivated domains switch clients to the primary; invite dropdown appears only with more than one active domain.
- **Manual:** a real domain with Let's Encrypt's test environment, DuckDNS and deSEC, a Cloudflare quick tunnel and a named tunnel, Caddy and Nginx snippets, UPnP on a home router.
- **Music:** the existing tests: queue rules, permissions, link validation, sync and version gaps, bridge conformance (a Rust client speaking Discord's voice protocol), real Lavalink in CI (with the `http` source enabled only in the test setup), three nodes with failover, restarts, security (encoded-track injection, private links, bridge connections from unknown addresses).
- **Commands:** parsing and validation on the server, permissions, ephemeral replies reach only the sender, public results stored correctly, autocomplete rate limit, `/volume` and `/music` never reach the server.
- **UI performance:** the §0.1 targets measured in profile mode.

### 21.2 Milestones

**L0 — Spike: Lavalink audio bridge**
- Run Lavalink 4.2.x locally. Minimal bridge on the main TLS port (§13.2–13.3): a player whose voice state points at the bridge plays a track, and the decrypted Opus reaches a Phase 2 voicebot.
- Verify and record: Lavalink accepts `dave_protocol_version: 0`; which encryption mode it picks; that the port in the endpoint is used; v8 heartbeats and resume; a certificate from Let's Encrypt's test environment or an imported self-signed one. If possible, also test one public node.
- **If Lavalink refuses to work without DAVE:** stop and report (options: DAVE external-sender role in the bridge, or the Lavalink plugin fallback).
- **Accept:** 60 s of music plays through a voicebot without gaps.

**D1 — Public address and ports**
- ipify detection and overrides, port list and states, main port setting, local proxy listener, trusted proxies, optional UPnP (§5).
- **Accept:** IP detection, override and change handling work; ports show correct states; UPnP maps and removes ports on a test router.

**D2 — Domains**
- Add-domain dialog, DNS and HTTP checks, statuses, active/inactive, primary, 12-hour re-checks, check history, `server_addresses` and client address handling, domain TLS verification in clients, invite dropdown and web invite page (§6, §7.5–7.6, §8).
- **Accept:** the DNS-check and client-domain tests in §21.1 pass.

**D3 — Certificates**
- Let's Encrypt with HTTP-01, TLS-ALPN-01 and DNS-01 (Cloudflare, DuckDNS, deSEC), optional email, storage in the database, SNI serving with hot reload, renewal, uploaded certificates (§7.1–7.4).
- **Accept:** the certificate tests in §21.1 pass; a real test-environment certificate is issued and served.

**D4 — Free subdomains, tunnels and proxies**
- DuckDNS and deSEC flows with dynamic DNS, Cloudflare quick and named tunnels with a supervised `cloudflared`, reverse-proxy domains with snippets (§9).
- **Accept:** each option gives a working domain that clients connect to; the UDP limits are shown where they apply.

**L1 — Lavalink nodes**
- Client, resuming, encrypted passwords, internal user ID, multiple nodes with health, best-node selection, node list with the bridge checklist and events, connection test (§12).
- **Accept:** node management works from the UI; wrong password, unreachable host and untrusted certificate each give the right explanation; with three nodes, new channels go to the best node.

**L2 — Music sessions and permissions**
- Session actor, queue and playback rules, relative moves, Lavalink events, persistence, timeouts (§14); failover (§12.7); permissions, limits, blocked members, settings (§16).
- **Accept:** rule and permission tests pass; real-Lavalink CI tests pass; every multi-node and restart test passes.

**L3 — Bridge in production**
- Bridge address from domains (§13.1), music stream in the SFU, `MusicStream`, forwarding rules, mute via `MediaSinkWants`, external voice nodes, access control, metrics (§13).
- **Accept:** a 10-person channel hears the music within 150 ms of each other on a LAN; deafened listeners receive no music packets; a public node works through a domain with a Let's Encrypt certificate.

**L4 — Search, links and artwork** (§15)
- **Accept:** search works for every enabled source; disallowed links and private addresses are rejected; encoded-track injection is impossible; artwork loads only through the proxy.

**L5 — Music UI**
- Music button, right-side panel modes, now-playing card, music panel, drag and drop, personal volume, expand, states (§19.1–19.5, §19.7–19.9).
- **Accept:** everything works on Windows, macOS and Linux; music never appears as a participant; the now-playing card appears only in the right-side users list; the §0.1 targets are met.

**L6 — Chat commands**
- Command framework, music commands, replies (public and ephemeral), Commands page, composer autocomplete (§17, §19.6).
- **Accept:** the command tests in §21.1 pass; every music action can be done both from the UI and with a command, with the same permissions.

**L7 — Sync and polish**
- Snapshot and update versioning, stale-action handling, vote skip, notices, accessibility (focus order, screen-reader labels for controls, queue rows and command chips).
- **Accept:** sync tests pass; two people dragging in the queue at the same time always end with the same order.

**L8 — Hardening and docs**
- `docs/domains.md` and `docs/music.md` (§23), deploy updates (ports 80 optional, 7710, 7711/udp, 7712/udp), security tests, load test with 20 playing channels.
- **Accept:** a fresh setup with a free subdomain, a certificate and a public Lavalink node plays music by following the docs.

**After Phase 3 (optional):** lyrics, audio effects, autoplay/radio, saved playlists, listening statistics, a TURN relay for servers without reachable UDP.

---

## 22. Documentation to read first
- **Let's Encrypt:** challenge types (HTTP-01, TLS-ALPN-01, DNS-01), rate limits, the staging environment, renewal information (ARI), and the 2025 end of expiry emails.
- **Crates:** `instant-acme`, `rcgen`, `hickory-proto`, `rustls` certificate resolvers, `rustls-platform-verifier`, `igd-next`.
- **DNS basics** for the checks: NS, A, AAAA and CNAME records, authoritative versus recursive queries, truncation and TCP fallback.
- **DuckDNS and deSEC** API documentation (updating records and TXT records).
- **Cloudflare Tunnel:** quick tunnels (limits), named tunnels with tokens, running `cloudflared` as a service.
- **Pebble** (Let's Encrypt's ACME test server) for CI.
- **Lavalink v4:** REST API, WebSocket API, configuration, plugins (youtube-source, LavaSrc, LavaSearch, LavaLyrics), and the v4 changelog (DAVE from 4.2.0, voice gateway v8 from 4.0.8).
- **Discord developer docs, Voice Connections:** opcodes, v8 buffered resume, IP discovery, `rtpsize` encryption modes, DAVE version negotiation, close codes. Only the voice-server side Lavalink uses is needed.
- RFC 3550 (RTP), RFC 7587 (Opus over RTP).

---

## 23. Admin guides

**`docs/domains.md`**
1. Choosing a domain type (§4) and what each means for voice and music.
2. Creating DNS records, including Cloudflare's "DNS only" setting, and reading check results.
3. Certificates: ports 80/443 and the DNS challenge, DNS provider tokens, uploaded certificates, binding low ports on Linux (`CAP_NET_BIND_SERVICE`) or with Docker.
4. Free subdomains with DuckDNS or deSEC.
5. Cloudflare Tunnel: quick versus named tunnels, running `cloudflared` from Opencord or as a Docker sidecar, and the UDP limitation.
6. Reverse proxies: Caddy and Nginx examples, the local listener and trusted proxy addresses.
7. Port forwarding and UPnP.
8. Troubleshooting each check result and certificate error.

**`docs/music.md`**
1. Running Lavalink next to Opencord with Docker Compose: the official image, `application.yml` (password, sources, the youtube-source plugin, `http` and `local` sources disabled), and the network alias trick so the bridge domain resolves inside Docker (§13.1).
2. Using public Lavalink nodes: needs an active domain with a trusted certificate and UDP 7712 reachable; turn off Strict address check if their addresses change.
3. IP-only servers: importing Opencord's self-signed certificate into your own Lavalink's Java trust store (`keytool -importcert -cacerts`).
4. Adding nodes, priorities, the bridge checklist, the connection test, draining for maintenance. Every node needs the same sources if tracks should survive a failover.
5. Sources: YouTube needs the youtube-source plugin (often with extra setup from its docs); Spotify, Apple Music and Deezer need LavaSrc and play matched copies. Admins are responsible for the sources they enable.
6. Commands: the Commands page, channel restrictions and the result setting.
7. Troubleshooting each node state, test failure and close code.
