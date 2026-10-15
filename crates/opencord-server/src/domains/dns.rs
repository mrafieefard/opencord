//! Asking DNS what a domain points to (Phase 3 plan §6.2), with plain DNS
//! packets over UDP port 53 (TCP when an answer is truncated), not the
//! system's resolver: a public resolver finds the domain's own name
//! servers, and those are asked directly, recursion off, so caches cannot
//! hide a change or a mistake. CNAMEs are followed, across zones too.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use hickory_proto::op::{Message, MessageType, Query, ResponseCode};
use hickory_proto::rr::rdata::{CNAME, NS, SOA};
use hickory_proto::rr::{Name, RData, RecordType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

/// Each query's wait, and how often it is sent.
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);
const ATTEMPTS: usize = 3;
/// CNAMEs followed before giving up (plan §6.2).
const MAX_CNAMES: usize = 8;
/// Name servers of a zone asked, at most.
const MAX_NAME_SERVERS: usize = 4;
const UDP_BUFFER: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DnsError {
    #[error("{0} is not a valid domain name")]
    InvalidName(String),
    #[error("no DNS server answered: {0}")]
    NoAnswer(String),
    #[error("{0} does not exist")]
    NoSuchDomain(String),
    #[error("no name servers found for {0}")]
    NoNameServers(String),
    #[error("more than {MAX_CNAMES} CNAMEs in a row")]
    TooManyCnames,
    #[error("a DNS server answered with an error: {0}")]
    Refused(String),
}

/// What a name resolves to, after its CNAMEs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved {
    pub v4: Vec<Ipv4Addr>,
    pub v6: Vec<Ipv6Addr>,
    /// The names followed, the asked one first.
    pub chain: Vec<String>,
}

impl Resolved {
    pub fn is_empty(&self) -> bool {
        self.v4.is_empty() && self.v6.is_empty()
    }

    pub fn addresses(&self) -> impl Iterator<Item = IpAddr> + '_ {
        self.v4
            .iter()
            .map(|ip| IpAddr::V4(*ip))
            .chain(self.v6.iter().map(|ip| IpAddr::V6(*ip)))
    }
}

#[derive(Debug, Clone)]
pub struct DnsChecker {
    /// Public resolvers, asked with recursion to find name servers.
    pub resolvers: Vec<SocketAddr>,
    /// The port name servers are asked on: 53, but for tests.
    pub name_server_port: u16,
    pub timeout: Duration,
}

impl DnsChecker {
    pub fn new(resolvers: Vec<SocketAddr>) -> Self {
        Self {
            resolvers,
            name_server_port: 53,
            timeout: QUERY_TIMEOUT,
        }
    }

    /// What `host`'s own name servers say it points to.
    pub async fn authoritative(&self, host: &str) -> Result<Resolved, DnsError> {
        let mut name = parse_name(host)?;
        let mut resolved = Resolved {
            chain: vec![display(&name)],
            ..Resolved::default()
        };
        for _ in 0..=MAX_CNAMES {
            let servers = self.zone_servers(&name).await?;
            let mut target = None;
            for record_type in [RecordType::A, RecordType::AAAA] {
                let answer = self.ask(&servers, &name, record_type, false).await?;
                if answer.metadata.response_code == ResponseCode::NXDomain {
                    return Err(DnsError::NoSuchDomain(display(&name)));
                }
                let (end, found) = follow(&answer, &name, &mut resolved.chain)?;
                if end != name && found.is_empty() {
                    // A CNAME out of this zone: ask the target's own.
                    target = Some(end);
                    break;
                }
                collect(&mut resolved, found);
            }
            match target {
                Some(next) => name = next,
                None => return Ok(resolved),
            }
        }
        Err(DnsError::TooManyCnames)
    }

    /// What a public resolver answers for `host`, caches and all: compared
    /// with [`Self::authoritative`] to warn of stale caches.
    pub async fn recursive(&self, host: &str) -> Result<Resolved, DnsError> {
        let name = parse_name(host)?;
        let mut resolved = Resolved {
            chain: vec![display(&name)],
            ..Resolved::default()
        };
        let resolvers: Vec<SocketAddr> = self.resolvers.iter().take(1).copied().collect();
        for record_type in [RecordType::A, RecordType::AAAA] {
            let answer = self.ask(&resolvers, &name, record_type, true).await?;
            let (_, found) = follow(&answer, &name, &mut resolved.chain)?;
            collect(&mut resolved, found);
        }
        Ok(resolved)
    }

    /// The addresses of the name servers of the zone `name` is in.
    async fn zone_servers(&self, name: &Name) -> Result<Vec<SocketAddr>, DnsError> {
        let zone = self.zone_of(name).await?;
        let answer = self
            .ask(&self.resolvers, &zone, RecordType::NS, true)
            .await?;
        let hosts: Vec<Name> = answer
            .answers
            .iter()
            .filter_map(|record| match &record.data {
                RData::NS(NS(host)) => Some(host.clone()),
                _ => None,
            })
            .take(MAX_NAME_SERVERS)
            .collect();
        if hosts.is_empty() {
            return Err(DnsError::NoNameServers(display(&zone)));
        }
        // Glue first; the rest through the resolver.
        let glue: HashMap<Name, Vec<IpAddr>> =
            answer
                .additionals
                .iter()
                .fold(HashMap::new(), |mut glue, record| {
                    if let Some(ip) = record.data.ip_addr() {
                        glue.entry(record.name.to_lowercase())
                            .or_insert_with(Vec::new)
                            .push(ip);
                    }
                    glue
                });
        let mut servers = Vec::new();
        for host in hosts {
            let ips = match glue.get(&host.to_lowercase()) {
                Some(ips) => ips.clone(),
                None => self.resolve_host(&host).await,
            };
            servers.extend(
                ips.into_iter()
                    .map(|ip| SocketAddr::new(ip, self.name_server_port)),
            );
        }
        if servers.is_empty() {
            return Err(DnsError::NoNameServers(display(&zone)));
        }
        // IPv4 first: more machines can reach it.
        servers.sort_by_key(|server| server.is_ipv6());
        Ok(servers)
    }

    /// The zone `name` belongs to: the closest name, from `name` up, that
    /// a resolver shows an SOA record for. A CNAME owner is never a zone's
    /// apex, so its parent is asked next: a resolver would otherwise follow
    /// the CNAME and show the target's zone.
    async fn zone_of(&self, name: &Name) -> Result<Name, DnsError> {
        let mut candidate = name.to_lowercase();
        loop {
            let answer = self
                .ask(&self.resolvers, &candidate, RecordType::SOA, true)
                .await?;
            let aliased = answer.answers.iter().any(|record| {
                record.name.to_lowercase() == candidate && matches!(record.data, RData::CNAME(_))
            });
            if !aliased {
                let soa = answer
                    .answers
                    .iter()
                    .chain(&answer.authorities)
                    .filter(|record| matches!(record.data, RData::SOA(SOA { .. })))
                    .map(|record| record.name.to_lowercase())
                    .find(|owner| owner.zone_of(&candidate));
                if let Some(zone) = soa {
                    return Ok(zone);
                }
                if answer.metadata.response_code == ResponseCode::NXDomain {
                    return Err(DnsError::NoSuchDomain(display(name)));
                }
            }
            if candidate.num_labels() <= 1 {
                return Err(DnsError::NoNameServers(display(name)));
            }
            candidate = candidate.base_name();
        }
    }

    /// A name server's addresses, through the resolver.
    async fn resolve_host(&self, host: &Name) -> Vec<IpAddr> {
        let mut ips = Vec::new();
        for record_type in [RecordType::A, RecordType::AAAA] {
            if let Ok(answer) = self.ask(&self.resolvers, host, record_type, true).await {
                ips.extend(
                    answer
                        .answers
                        .iter()
                        .filter_map(|record| record.data.ip_addr()),
                );
            }
        }
        ips
    }

    /// The first answer from `servers`, each tried in turn, every attempt.
    async fn ask(
        &self,
        servers: &[SocketAddr],
        name: &Name,
        record_type: RecordType,
        recursion: bool,
    ) -> Result<Message, DnsError> {
        let mut last = String::from("no server to ask");
        for _ in 0..ATTEMPTS {
            for server in servers {
                match self.query(*server, name, record_type, recursion).await {
                    Ok(answer) => match answer.metadata.response_code {
                        ResponseCode::NoError | ResponseCode::NXDomain => return Ok(answer),
                        code => last = format!("{server} answered {code}"),
                    },
                    Err(error) => last = format!("{server}: {error}"),
                }
            }
        }
        if last.contains("answered") {
            return Err(DnsError::Refused(last));
        }
        Err(DnsError::NoAnswer(last))
    }

    /// One question to one server, over UDP, and again over TCP when the
    /// answer did not fit.
    async fn query(
        &self,
        server: SocketAddr,
        name: &Name,
        record_type: RecordType,
        recursion: bool,
    ) -> Result<Message, String> {
        let mut message = Message::query();
        message.metadata.recursion_desired = recursion;
        message.add_query(Query::query(name.clone(), record_type));
        let id = message.metadata.id;
        let bytes = message.to_vec().map_err(|error| error.to_string())?;
        let answer = tokio::time::timeout(self.timeout, udp(server, &bytes, id))
            .await
            .map_err(|_| "no answer in time".to_owned())??;
        if !answer.metadata.truncation {
            return Ok(answer);
        }
        tokio::time::timeout(self.timeout, tcp(server, &bytes, id))
            .await
            .map_err(|_| "no answer over TCP in time".to_owned())?
    }
}

async fn udp(server: SocketAddr, query: &[u8], id: u16) -> Result<Message, String> {
    let bind: SocketAddr = match server {
        SocketAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        SocketAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    let socket = UdpSocket::bind(bind).await.map_err(|e| e.to_string())?;
    socket.connect(server).await.map_err(|e| e.to_string())?;
    socket.send(query).await.map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; UDP_BUFFER];
    loop {
        let size = socket.recv(&mut buffer).await.map_err(|e| e.to_string())?;
        // Anything else on the socket is not our answer.
        if let Ok(answer) = Message::from_vec(&buffer[..size])
            && answer.metadata.id == id
            && answer.metadata.message_type == MessageType::Response
        {
            return Ok(answer);
        }
    }
}

async fn tcp(server: SocketAddr, query: &[u8], id: u16) -> Result<Message, String> {
    let mut stream = TcpStream::connect(server)
        .await
        .map_err(|e| e.to_string())?;
    let length = u16::try_from(query.len()).map_err(|e| e.to_string())?;
    stream
        .write_all(&length.to_be_bytes())
        .await
        .map_err(|e| e.to_string())?;
    stream.write_all(query).await.map_err(|e| e.to_string())?;
    let size = stream.read_u16().await.map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; usize::from(size)];
    stream
        .read_exact(&mut buffer)
        .await
        .map_err(|e| e.to_string())?;
    let answer = Message::from_vec(&buffer).map_err(|e| e.to_string())?;
    if answer.metadata.id != id {
        return Err("an answer to another question".to_owned());
    }
    Ok(answer)
}

/// Follows the CNAMEs in `answer` from `name`; returns where they end and
/// the addresses found for that name there.
fn follow(
    answer: &Message,
    name: &Name,
    chain: &mut Vec<String>,
) -> Result<(Name, Vec<IpAddr>), DnsError> {
    let cnames: HashMap<Name, Name> = answer
        .answers
        .iter()
        .filter_map(|record| match &record.data {
            RData::CNAME(CNAME(target)) => Some((record.name.to_lowercase(), target.clone())),
            _ => None,
        })
        .collect();
    let mut current = name.clone();
    let mut steps = 0;
    while let Some(next) = cnames.get(&current.to_lowercase()) {
        steps += 1;
        if steps > MAX_CNAMES {
            return Err(DnsError::TooManyCnames);
        }
        current = next.clone();
        let shown = display(&current);
        if !chain.contains(&shown) {
            chain.push(shown);
        }
    }
    let found = answer
        .answers
        .iter()
        .filter(|record| record.name.to_lowercase() == current.to_lowercase())
        .filter_map(|record| record.data.ip_addr())
        .collect();
    Ok((current, found))
}

fn collect(resolved: &mut Resolved, found: Vec<IpAddr>) {
    for ip in found {
        match ip {
            IpAddr::V4(v4) if !resolved.v4.contains(&v4) => resolved.v4.push(v4),
            IpAddr::V6(v6) if !resolved.v6.contains(&v6) => resolved.v6.push(v6),
            _ => {}
        }
    }
}

fn parse_name(host: &str) -> Result<Name, DnsError> {
    let mut name = Name::from_ascii(host).map_err(|_| DnsError::InvalidName(host.to_owned()))?;
    name.set_fqdn(true);
    Ok(name)
}

/// A name as people write it: lowercase, without the final dot.
fn display(name: &Name) -> String {
    name.to_lowercase()
        .to_ascii()
        .trim_end_matches('.')
        .to_owned()
}

/// A DNS server for tests.
#[doc(hidden)]
pub mod fake;

#[cfg(test)]
mod tests;
