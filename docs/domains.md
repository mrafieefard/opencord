# Domains and network

Only the server's owner sees this, in the app under **Server settings → Domains & network**. It shows where the server is reached from the internet, which ports it uses and how they are reached. The server does the work in the background; the page shows what it reports, live while it is open.

This guide grows with Phase 3. What is here so far:

- [Kinds of domain](#kinds-of-domain)
- [Adding a domain](#adding-a-domain)
- [What a check says](#what-a-check-says)
- [Active, inactive and primary](#active-inactive-and-primary)
- [Invites through a domain](#invites-through-a-domain)
- [How apps use domains](#how-apps-use-domains)
- [Certificates](#certificates)
- [DNS providers](#dns-providers)
- [Free subdomains](#free-subdomains)
- [Cloudflare Tunnel](#cloudflare-tunnel)
- [Reverse proxies](#reverse-proxies)
- [Public address](#public-address)
- [Ports](#ports)
- [Opening ports on a home router](#opening-ports-on-a-home-router)
- [Ports below 1024](#ports-below-1024)
- [The main port](#the-main-port)
- [Tunnels and reverse proxies: the local listener](#tunnels-and-reverse-proxies-the-local-listener)
- [Advanced settings](#advanced-settings)
- [Troubleshooting](#troubleshooting)

## Kinds of domain

| Kind | Use it when | How it is checked | Voice and music (UDP) |
|---|---|---|---|
| **Own domain** | You have a domain and can point it at this server | DNS: its A and AAAA records must point here | Straight to the server |
| **Reverse proxy** | Caddy, Nginx, Traefik, or a tunnel such as ngrok or Tailscale Funnel, handles TLS | HTTPS: the proxy must lead to this server | Straight to the server: proxies carry no UDP |
| **Free subdomain** | You have no domain: DuckDNS or deSEC | DNS | Straight to the server |
| **Cloudflare Tunnel** | The server cannot accept incoming connections | HTTPS through the tunnel | Not through the tunnel |

Free subdomains and Cloudflare Tunnel have their own buttons on the page, with what they need to set up: see [Free subdomains](#free-subdomains) and [Cloudflare Tunnel](#cloudflare-tunnel).

## Adding a domain

**Add domain** asks for the name (for example `chat.example.com`; names in other scripts are turned into punycode), how it reaches the server, and the port people connect to (the main port for own domains, 443 behind a proxy). A server has at most 20 domains.

For an own domain, the next step shows the DNS records to create at your DNS provider, with copy buttons:

```text
A     chat.example.com  →  203.0.113.10
AAAA  chat.example.com  →  2001:db8::10
```

The AAAA record appears only when the server has IPv6. If the domain's DNS is at Cloudflare, set the records to **DNS only** (grey cloud): Cloudflare's proxy forwards only web traffic on a few ports, and no UDP for voice. To keep a domain behind Cloudflare, use a [Cloudflare Tunnel](#cloudflare-tunnel).

Behind a reverse proxy, the next step shows how to point the proxy at Opencord's local listener. For Caddy:

```text
chat.example.com {
    reverse_proxy 127.0.0.1:7719
}
```

Then press **Check now**, or **Check** on the domain's row later; records can take a few minutes to appear.

## What a check says

Own domains are checked by asking the domain's own name servers directly, so caches cannot hide a mistake. A public resolver is asked too, and when it still has an old answer the row says so: it catches up within the record's time to live.

| Result | Meaning | What to do |
|---|---|---|
| Verified | It points to this server | Nothing |
| Points to another address | Its records give another address, shown on the row | Change the A/AAAA records to the server's address |
| No record | The name has no A or AAAA record, or does not exist | Create the records |
| Proxied by Cloudflare | It points to Cloudflare's proxy | Set the record to DNS only (grey cloud) |
| Partly correct | Some records are right, others not: an old AAAA record, or a second address | Remove or fix the wrong records |
| Check failed | No name server answered, or the server's public address is unknown | Try again; set the public address if needed |

Behind a proxy, the check asks `https://chat.example.com/.well-known/opencord/verify` through the proxy for a token only this server knows. If the router cannot reach its own public address (no "hairpin NAT"), the check cannot get out and back in; open the address from another network, and if the page loads, use **Confirm it works** from the domain's menu.

A domain may be checked by hand once every 10 seconds. Every domain is checked again at start-up, every 12 hours, and right after the public address changes; each keeps its last 10 checks under **Check history**.

## Active, inactive and primary

- Only a domain that passed a check can be active. The first one to pass becomes active and the **primary** by itself.
- The primary is what people connect to first, what invites use, and where Lavalink connects for music.
- A domain whose check fails after it passed is marked **DNS problem** with the time it started, and the owner is told. It stays active, so a short DNS hiccup breaks nothing.
- Turning a domain off takes it out of invites and tells apps to stop using it; connections through it keep working for 7 days. The primary can only be turned off, or deleted, once another domain is the primary.
- Each row shows how many people connected through the domain in the last 24 hours, to help decide what can go.

## Invites through a domain

With one active domain, every new invite goes through it. With more than one, the invite dialog offers the choice, the primary first. An invite through a domain gives two links:

- `opencord://chat.example.com/invite/CODE` for the app (the port is added when it is not 7710). It carries the server's fingerprint unless the domain has a certificate from an authority (behind a proxy or tunnel; with Let's Encrypt from the certificates milestone on).
- `https://chat.example.com/invite/CODE`, a small page with the server's name, its member count, an **Open in Opencord** button and a download link.

An invite through a domain stops working when the domain is turned off or deleted.

## How apps use domains

Apps learn the active domains and the public address when they connect, and when they change. They connect to the primary first, then the other domains, then the address they added the server with, then the IP address, so a domain that stops answering costs nothing but a few seconds. A server added again at another address is recognised and stays one entry. Domains with a certificate from an authority (Let's Encrypt, an uploaded one, Cloudflare's or a proxy's) are checked like a website, with no fingerprint to confirm.

## Certificates

A domain with a certificate from Let's Encrypt is checked by apps like a website: no fingerprint to confirm, and renewals cause no warnings. Tunnel and reverse-proxy domains already have one, from Cloudflare or the proxy.

**Getting one.** Once an own domain is verified, open its **⋯** menu and choose **Get certificate**. The first time, Opencord asks for an email address (optional: Let's Encrypt only uses it for account recovery and important notices; it sends no expiry reminders) and for your agreement to Let's Encrypt's subscriber agreement. The row then shows each step: creating the account, ordering, setting up the challenge, validating, issuing. The certificate is used at once, without a restart.

**How Let's Encrypt checks the domain.** Opencord picks the way that can work:

| Way | Used when | Needs |
|---|---|---|
| DNS challenge | A [DNS provider](#dns-providers) is connected that holds the domain | No open ports |
| TLS-ALPN challenge | Opencord's main port is 443 | Port 443 reachable from the internet |
| HTTP challenge | Otherwise | Port 80 free on the machine and reachable from the internet; Opencord answers on it only while Let's Encrypt checks |

When it fails, the row says why in plain words, for example that port 80 is not reachable, or that Let's Encrypt's limit was reached and when to try again. After 5 failures within an hour, the next try waits until an hour has passed since the first of them; the row says when. To try the setup without reaching Let's Encrypt's limits, turn on **Let's Encrypt test environment** under [Advanced settings](#advanced-settings): its certificates are labelled "Test", kept, and not used.

**Renewal.** Opencord looks every hour and renews when a third of the certificate's life is left (about 30 days for Let's Encrypt's 90), or earlier when Let's Encrypt asks for it. The old certificate stays in use until the new one is in. If renewing keeps failing, the row says "Renewal failed", and you are told 7 days before it expires.

**Your own certificate.** For a certificate from your own authority, or a wildcard one, choose **Upload certificate** in the domain's menu and paste or choose the certificate chain and the private key, both in PEM. Opencord checks that they belong together, cover the domain and have not expired. It does not renew uploaded certificates; you are told 14 days before one expires.

**Where they are kept.** Certificates are in the server's database. Private keys and the Let's Encrypt account are encrypted with `secrets.key` in the data directory; back it up together with the database. Without it the keys cannot be read: certificates must be requested again and providers connected again.

For Opencord on a machine Let's Encrypt cannot reach, such as a test network, `[acme]` in `opencord.toml` points it elsewhere:

| Key | Default | Meaning |
|---|---|---|
| `directory` | Let's Encrypt | The ACME directory used for certificates |
| `test_directory` | Let's Encrypt's staging | Used when the test environment is on |
| `root` | none | A PEM file with the ACME server's own root, for a private CA such as step-ca |
| `http_port` | 80 | Where the HTTP challenge is answered |
| `tls_alpn_port` | 443 | The main port that makes the TLS-ALPN challenge possible |
| `dns_propagation_s` | 120 | How long, at most, to wait for a DNS challenge record to appear |

## DNS providers

Connecting a DNS provider lets certificates use the DNS challenge, which needs no open ports at all: the server sets a temporary record and removes it afterwards. Under **DNS providers**, choose **Connect** and paste a token:

| Provider | Token |
|---|---|
| Cloudflare | An API token with "Edit zone DNS" permission for the zone (My Profile → API Tokens → Create Token → Edit zone DNS) |
| DuckDNS | The account token shown on duckdns.org |
| deSEC | A token from desec.io (Token Management) |

The token is checked when it is connected, kept encrypted on the server, and never shown again: the page shows its last four characters. **Replace** puts in a new one; **Disconnect** deletes it.

## Free subdomains

No domain? A free one from DuckDNS (`name.duckdns.org`) or deSEC (`name.dedyn.io`) works like your own, and Opencord does the DNS for it.

1. Under **Domains**, choose **Get a free domain** and pick DuckDNS or deSEC.
2. At the provider: on duckdns.org, sign in and add a subdomain; the token is at the top of the page. On desec.io, register, create a domain under dedyn.io, and make a token under Token Management.
3. Paste the token and the name. The first time, agree to Let's Encrypt's subscriber agreement (and give an email if you like): the certificate comes by itself.

Opencord then points the name at the server's public address, checks it until the records show (seconds for DuckDNS, up to a minute or so for deSEC), and gets a certificate through the DNS challenge, so no port needs to be open for it. When the public address changes, Opencord updates the record before it checks the domains again: the name keeps working on a home connection whose address changes (dynamic DNS).

The token connects the provider under [DNS providers](#dns-providers) too, where it can be replaced. People still reach the server's ports directly, so forward them (or turn on UPnP) as for any domain.

## Cloudflare Tunnel

A tunnel reaches the server when its ports cannot be opened (no port forwarding, carrier-grade NAT). cloudflared, Cloudflare's connector, opens an outgoing connection to Cloudflare and forwards the tunnel's name to Opencord's [local listener](#reverse-proxies). Cloudflare provides the certificate, so apps trust the name without a fingerprint, and WebSockets pass through: chat, voice signalling, web invites and a Lavalink node's connection to the audio bridge all work.

**Tunnels carry no UDP.** Voice and video (UDP 7711) and music from Lavalink (UDP 7712) still need to reach the server directly. Apps that connect through a tunnel or a reverse proxy are sent to the server's public address for voice and video (the one under **Public address**, detected or set by hand), so it must be known and UDP 7711 open there. Without any reachable UDP, text works fully; music works when Lavalink runs on the same machine or network, since its UDP stays local; voice needs a UDP path (a relay is planned for later).

Under **Domains**, choose **Use a tunnel**:

| Choice | Needs | What you get |
|---|---|---|
| Quick tunnel | Nothing | A random `*.trycloudflare.com` address. For testing only: the address changes every time the tunnel starts, Cloudflare allows at most 200 requests at once, and there is no uptime guarantee. |
| My Cloudflare tunnel | A free Cloudflare account and a domain on Cloudflare | Your own name. In Cloudflare's dashboard (Zero Trust, then Networks, then Tunnels) create a tunnel, add a public hostname whose service is `http://localhost:7719`, and paste the tunnel's token (the long value in its install command) and that hostname into Opencord. |
| I run cloudflared | cloudflared running elsewhere, such as a Docker sidecar | Add the hostname as a tunnel domain; Opencord checks it through the tunnel. |

Opencord runs cloudflared itself for the first two: it starts it again when it stops (after a pause that grows to a minute), shows its state and restarts on the page, and keeps its last 200 lines of output (**Output**), with the token hidden. The tunnel starts again with the server. **Stop** ends it; a quick tunnel's address goes with it, a named tunnel's domain stays.

**cloudflared.** Opencord uses `cloudflared` under `[tunnel]` in `opencord.toml` when it is set, else one it downloaded, else the one on the server's `PATH`. Without one, the page offers **Download**: Opencord fetches Cloudflare's latest release for the machine from GitHub and keeps it only when its SHA-256 matches the checksum Cloudflare lists in the release notes, and GitHub's own digest of the file. It is saved as `bin/cloudflared` in the data directory.

**As a Docker sidecar.** The sidecar shares the Opencord container's network, so the tunnel's `http://localhost:7719` reaches the local listener as it would on one machine. Add this service to `deploy/docker-compose.yml`, with the tunnel token in `.env` as `TUNNEL_TOKEN`, then add the hostname with "I run cloudflared":

```yaml
  cloudflared:
    image: cloudflare/cloudflared:latest
    restart: unless-stopped
    network_mode: "service:opencord"
    environment:
      TUNNEL_TOKEN: ${TUNNEL_TOKEN}
    command: tunnel --no-autoupdate run
```

## Cloudflare proxy

**Add domain** no longer offers Cloudflare's proxy (the orange cloud). A domain added that way with an earlier version keeps working: Cloudflare connects to the server on 443 (or the port an Origin Rule names), and **Upload origin certificate** in the domain's menu takes a Cloudflare origin certificate for SSL/TLS **Full (strict)**. While it is active, Opencord believes Cloudflare's addresses about who they forward (`X-Forwarded-For`). Voice and video still go straight to the server on UDP 7711.

## Reverse proxies

Caddy, Nginx, Traefik, ngrok, Tailscale Funnel and the like handle TLS and forward to Opencord. **Add domain** with "Through a reverse proxy" shows the configuration for the name. For Caddy:

```
chat.example.com {
    reverse_proxy 127.0.0.1:7719
}
```

For Nginx, with its own certificate; WebSockets need the upgrade headers, and calls and chats keep connections open for long:

```nginx
server {
    listen 443 ssl;
    http2 on;
    server_name chat.example.com;
    ssl_certificate     /etc/letsencrypt/live/chat.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/chat.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:7719;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_read_timeout 1h;
        proxy_send_timeout 1h;
        client_max_body_size 50m;
    }
}
```

Files go up in pieces of 8 MiB, so a proxy must let request bodies of at least 9 MB through: the `client_max_body_size 50m` above does, and Caddy and Traefik have no limit by default. Pieces are that size because Cloudflare refuses requests over 100 MB, on tunnels and proxied records alike, and an 8 MiB piece is quick to send again after a dropped connection.

A proxy on another machine cannot reach the local listener: point it at the main port (`https://<server>:7710`) instead, and add its address to **Trusted proxies** under Advanced. Like tunnels, proxies carry no UDP: 7711 and 7712 must reach the server directly.

### In Docker: Traefik, Dokploy, Coolify

A proxy in another container reaches Opencord over their shared Docker network, not over loopback. The Opencord image sets `OPENCORD_LOCAL_LISTENER_BIND=0.0.0.0`, so the local listener answers there:

- Point the proxy at the Opencord service on port 7719, in plain HTTP. In Dokploy, that is the domain's container port.
- Don't publish 7719 with `ports:`: the proxy reaches it without that, and it serves without TLS.
- Add the Docker network's range to **Trusted proxies** under Advanced, for example `10.0.0.0/8` or `172.16.0.0/12` (`docker network inspect <network>` shows it). Without it, every person seems to come from the proxy, and rate limits and bans treat them as one.
- Publish 7711/udp, and 7712/udp for Lavalink on other machines, straight from the Opencord container.

With the host's network (`network_mode: host`), the listener would answer on the machine's own addresses: set `OPENCORD_LOCAL_LISTENER_BIND=127.0.0.1` there. Images from before this setting listen on the container's loopback only, which a proxy in another container cannot reach: it answers with Bad Gateway.

## Public address

The server finds its public address by asking a lookup service what address its request came from: `https://api.ipify.org`, and `https://api6.ipify.org` when the machine has IPv6. It asks at start-up and every 6 hours. The address is what domains must point to, and where Lavalink nodes on other machines send music.

- **Set by hand**: for a fixed address, or one the lookup gets wrong (for example when the server's outgoing traffic leaves through a VPN). An address set by hand is used instead of the detected one; clear it to go back.
- **Detect automatically** off: the server never contacts the lookup service. To never contact it at all, not even before the first start, set the lookups to empty in `opencord.toml`:

```toml
[network]
ip_lookup_v4 = ""
ip_lookup_v6 = ""
```

`OPENCORD_IP_LOOKUP_V4` and `OPENCORD_IP_LOOKUP_V6` do the same. Any service that answers with the caller's address as plain text works in their place.

When the address changes, the owner gets a notice, and (from the domains milestones on) domains are checked again and free subdomains follow it.

## Ports

| Port | Protocol | Used for | Must be reachable from |
|---|---|---|---|
| 7710 (the main port) | TCP | Chat, voice signalling, files, invite pages, and Lavalink's connection to the music bridge | Everyone, Lavalink nodes |
| 7711 | UDP | Voice and video | Everyone |
| 7712 | UDP | Music audio from Lavalink | Lavalink nodes on other machines |
| 80 | TCP | Let's Encrypt's check, only while a certificate is requested or renewed | The internet |
| 7719 | TCP, this machine only (or its Docker network) | The local listener for tunnels and reverse proxies | Programs on the same machine, or proxies in other containers |

The page shows each port's state:

| State | Meaning |
|---|---|
| Listening | Opencord uses it |
| Off | Its feature is off (no voice, no music, no tunnel or proxy) |
| Free | Not in use; Opencord takes it when it needs it |
| In use by another program | Something else listens on it; stop it or pick another port |
| Not allowed | The system does not let Opencord use it: see [Ports below 1024](#ports-below-1024) |

## Opening ports on a home router

Forward the ports above to the server's machine in the router's settings, or turn on **Open ports on my router (UPnP)** in the app. With UPnP, the server asks the router to forward the main port and the voice and music UDP ports to it. The mappings last an hour and are renewed every half hour, and they are removed when UPnP is turned off or the server stops. The page shows the router that answered, the public address the router reports, and for each port whether it is forwarded or why the router refused.

- Many routers have UPnP off; turn it on in the router first.
- A router whose mapping table is full refuses new mappings ("Port mapping table full" or similar); remove old ones in the router's settings.
- In Docker, the server cannot reach the router with UPnP from the container's own network. Forward the ports by hand, or run the container with the host's network.
- UPnP opens ports on the router only. A firewall on the server's machine needs its own rules.

## Ports below 1024

Linux lets only privileged programs listen on ports below 1024, such as 443 (to use as the main port) and 80 (for Let's Encrypt's check). Either:

- with systemd, give the service the capability, as the comments in `deploy/opencord-server.service` show:

```ini
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
```

- with the binary itself:

```sh
sudo setcap cap_net_bind_service=+ep /usr/local/bin/opencord-server
```

- with Docker, map the port in the compose file (`"443:7710/tcp"`); inside the container, Docker already lets the server listen on low ports.

## The main port

The main port can be changed in the app; it takes effect when the server restarts. Use 443 so addresses and invite links need no port number. If the new port cannot be had at the next start (another program has it, or it is below 1024 without the capability), the server falls back to the port in `opencord.toml`, `bind` under `[server]`, and logs why, so the owner can always get back in.

## Tunnels and reverse proxies: the local listener

Tunnels (Cloudflare Tunnel, ngrok) and reverse proxies (Caddy, Nginx, Traefik) take care of TLS and forward plain HTTP to Opencord's local listener: `http://127.0.0.1:7719`. It serves the same things as the main port without TLS, and only programs on the same machine can reach it. It runs while a tunnel or reverse-proxy domain exists, or a tunnel runs.

It listens on `127.0.0.1`. `local_listener_bind` under `[network]` in `opencord.toml` (`OPENCORD_LOCAL_LISTENER_BIND`) changes that address, and takes effect at the next start. The Docker image sets it to `0.0.0.0` so a proxy in another container can reach it ([In Docker](#in-docker-traefik-dokploy-coolify)). Its port is set in the app.

Behind a proxy, every connection seems to come from the proxy. Opencord believes the client address in `X-Forwarded-For` only from **trusted proxies**: `127.0.0.1` and `::1` by default, which covers a proxy on the same machine. Add a proxy on another machine under Advanced.

## Advanced settings

| Setting | Default | Meaning |
|---|---|---|
| DNS resolvers for checks | 1.1.1.1, 8.8.8.8, 9.9.9.9 | Domain checks find each domain's name servers through these. Change them when your network blocks the defaults. |
| Let's Encrypt test environment | Off | Certificates from Let's Encrypt's staging environment, for trying the setup without reaching its limits. They are not trusted by browsers or apps. |
| Local listener port | 7719 | Where tunnels and proxies forward to. |
| Trusted proxies | 127.0.0.1, ::1 | Addresses or ranges (`10.0.0.0/8`) whose `X-Forwarded-For` is believed. |

## Troubleshooting

### Check results

| Result | Meaning | What to do |
|---|---|---|
| **Points to another address** | The domain's A or AAAA records point to a different IP than Opencord's public address | Update your DNS records at your domain registrar or DNS provider to match the server's public IP |
| **No record** | The hostname has no DNS records, or newly created records haven't propagated | Create the A/AAAA records at your provider and wait a few minutes |
| **Proxied by Cloudflare** | Cloudflare's proxy (orange cloud) is turned on, blocking UDP voice and music traffic | Set the DNS record to **DNS only** (grey cloud) in Cloudflare's DNS dashboard |
| **Partly correct** | Some records match the server, but extra or stale records exist (e.g. an old AAAA record) | Remove the outdated or conflicting DNS records |
| **Check failed** | Name servers did not answer, or the server's public IP could not be detected | Verify your network connection and DNS resolvers; set your public address manually under Public address if auto-detection fails |
| **Wrong verify token** | A reverse proxy or tunnel answered, but didn't return the server's verification token | Ensure the proxy forwards traffic to the local listener port (`http://127.0.0.1:7719`) and preserves `/.well-known/opencord/verify` |
| **Hairpin NAT failure** | Behind a home router without NAT loopback, proxy checks from inside the network fail | Test reaching the address from a mobile or external network; if it works, click **Confirm it works** in the domain's menu |

### Certificates

| Error | Meaning | What to do |
|---|---|---|
| **Port 80 not reachable** | Let's Encrypt could not connect on port 80 for the HTTP challenge | Forward port 80/tcp on your router, check your firewall, or connect a [DNS provider](#dns-providers) to use the DNS challenge instead |
| **Rate limit reached** | Too many failed attempts (5 per hour) or too many certificates for this domain | Wait until the retry time shown on the row; enable **Let's Encrypt test environment** under Advanced settings while testing |
| **DNS propagation timeout** | The DNS-01 challenge record was not seen by Let's Encrypt within 120 seconds | Verify the DNS provider token has "Edit zone DNS" permissions and that the zone name matches |
| **CAA record prohibits issuance** | The domain's CAA records do not allow Let's Encrypt to issue certificates | Add `0 issue "letsencrypt.org"` to your domain's DNS records, or remove the restricting CAA record |
| **Permission denied / Low port** | Opencord cannot bind port 80 or 443 on Linux without privileges | Grant `CAP_NET_BIND_SERVICE` (see [Ports below 1024](#ports-below-1024)) or use Docker port forwarding |
| **Renewal failed** | Automatic renewal failed before expiration | The row shows the exact cause (e.g. port 80 unreachable or expired provider token); resolve it before the 7-day grace period ends |

