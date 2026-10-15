# Domains and network

Only the server's owner sees this, in the app under **Server settings → Domains & network**. It shows where the server is reached from the internet, which ports it uses and how they are reached. The server does the work in the background; the page shows what it reports, live while it is open.

This guide grows with Phase 3. What is here so far:

- [Public address](#public-address)
- [Ports](#ports)
- [Opening ports on a home router](#opening-ports-on-a-home-router)
- [Ports below 1024](#ports-below-1024)
- [The main port](#the-main-port)
- [Tunnels and reverse proxies: the local listener](#tunnels-and-reverse-proxies-the-local-listener)
- [Advanced settings](#advanced-settings)

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
| 7719 | TCP, this machine only | The local listener for tunnels and reverse proxies | Programs on the same machine |

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

Tunnels (Cloudflare Tunnel, ngrok) and reverse proxies (Caddy, Nginx, Traefik) take care of TLS and forward plain HTTP to Opencord's local listener: `http://127.0.0.1:7719`. It serves the same things as the main port without TLS, and only programs on the same machine can reach it. It runs while a tunnel or reverse-proxy domain exists.

Behind a proxy, every connection seems to come from the proxy. Opencord believes the client address in `X-Forwarded-For` only from **trusted proxies**: `127.0.0.1` and `::1` by default, which covers a proxy on the same machine. Add a proxy on another machine under Advanced.

## Advanced settings

| Setting | Default | Meaning |
|---|---|---|
| DNS resolvers for checks | 1.1.1.1, 8.8.8.8, 9.9.9.9 | Domain checks find each domain's name servers through these. Change them when your network blocks the defaults. |
| Let's Encrypt test environment | Off | Certificates from Let's Encrypt's staging environment, for trying the setup without reaching its limits. They are not trusted by browsers or apps. |
| Local listener port | 7719 | Where tunnels and proxies forward to. |
| Trusted proxies | 127.0.0.1, ::1 | Addresses or ranges (`10.0.0.0/8`) whose `X-Forwarded-For` is believed. |
