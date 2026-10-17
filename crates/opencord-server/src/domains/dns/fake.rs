//! A DNS server for tests: zones it answers for, as their authoritative
//! name server and as a public resolver at once, on UDP and TCP of one
//! port on 127.0.0.1. It can truncate answers over UDP, stay silent, and
//! count what it is asked.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use hickory_proto::op::{Message, MessageType, ResponseCode};
use hickory_proto::rr::rdata::{A, AAAA, CNAME, NS, SOA};
use hickory_proto::rr::{Name, RData, Record, RecordType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

const TTL: u32 = 300;

#[derive(Default)]
struct State {
    zones: Vec<Name>,
    records: Vec<Record>,
    truncated: HashSet<Name>,
    silent: HashSet<Name>,
    /// Every question: the name, its type, and whether recursion was asked.
    asked: Vec<(String, RecordType, bool)>,
}

#[derive(Clone)]
pub struct FakeDns {
    pub address: SocketAddr,
    state: Arc<Mutex<State>>,
}

fn name(text: &str) -> Name {
    let mut name = Name::from_ascii(text).unwrap();
    name.set_fqdn(true);
    name.to_lowercase()
}

impl FakeDns {
    pub async fn start() -> Self {
        let udp = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = udp.local_addr().unwrap();
        let tcp = TcpListener::bind(address).await.unwrap();
        let fake = Self {
            address,
            state: Arc::default(),
        };
        let served = fake.clone();
        tokio::spawn(async move {
            let mut buffer = vec![0u8; 4096];
            loop {
                let Ok((size, from)) = udp.recv_from(&mut buffer).await else {
                    return;
                };
                if let Some(answer) = served.answer(&buffer[..size], true) {
                    let _ = udp.send_to(&answer, from).await;
                }
            }
        });
        let served = fake.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = tcp.accept().await {
                let served = served.clone();
                tokio::spawn(async move {
                    let Ok(size) = stream.read_u16().await else {
                        return;
                    };
                    let mut query = vec![0u8; usize::from(size)];
                    if stream.read_exact(&mut query).await.is_err() {
                        return;
                    }
                    if let Some(answer) = served.answer(&query, false) {
                        let length = u16::try_from(answer.len()).unwrap();
                        let _ = stream.write_all(&length.to_be_bytes()).await;
                        let _ = stream.write_all(&answer).await;
                    }
                });
            }
        });
        fake
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A zone with its SOA and one name server, `ns1.<zone>`, at
    /// 127.0.0.1.
    pub fn zone(&self, apex: &str) -> &Self {
        let apex_name = name(apex);
        let ns = name(&format!("ns1.{apex}"));
        let mut state = self.lock();
        state.zones.push(apex_name.clone());
        state.records.push(Record::from_rdata(
            apex_name.clone(),
            TTL,
            RData::SOA(SOA::new(
                ns.clone(),
                name(&format!("admin.{apex}")),
                1,
                3600,
                600,
                86400,
                300,
            )),
        ));
        state.records.push(Record::from_rdata(
            apex_name,
            TTL,
            RData::NS(NS(ns.clone())),
        ));
        state.records.push(Record::from_rdata(
            ns,
            TTL,
            RData::A(A(Ipv4Addr::LOCALHOST)),
        ));
        drop(state);
        self
    }

    pub fn record(&self, owner: &str, data: RData) -> &Self {
        self.lock()
            .records
            .push(Record::from_rdata(name(owner), TTL, data));
        self
    }

    pub fn a(&self, owner: &str, ip: &str) -> &Self {
        self.record(owner, RData::A(A(ip.parse().unwrap())))
    }

    pub fn aaaa(&self, owner: &str, ip: &str) -> &Self {
        self.record(owner, RData::AAAA(AAAA(ip.parse().unwrap())))
    }

    pub fn cname(&self, owner: &str, target: &str) -> &Self {
        self.record(owner, RData::CNAME(CNAME(name(target))))
    }

    /// Answers about `owner` over UDP come truncated and empty.
    pub fn truncate(&self, owner: &str) -> &Self {
        self.lock().truncated.insert(name(owner));
        self
    }

    /// Questions about `owner` get no answer at all.
    pub fn silence(&self, owner: &str) -> &Self {
        self.lock().silent.insert(name(owner));
        self
    }

    /// Replaces `owner`'s addresses.
    pub fn repoint(&self, owner: &str, ip: &str) {
        let owner = name(owner);
        self.lock().records.retain(|record| {
            record.name != owner || !matches!(record.data, RData::A(_) | RData::AAAA(_))
        });
        let ip: IpAddr = ip.parse().unwrap();
        let data = match ip {
            IpAddr::V4(v4) => RData::A(A(v4)),
            IpAddr::V6(v6) => RData::AAAA(AAAA(v6)),
        };
        self.lock()
            .records
            .push(Record::from_rdata(owner, TTL, data));
    }

    pub fn asked(&self) -> Vec<(String, RecordType, bool)> {
        self.lock().asked.clone()
    }

    fn answer(&self, bytes: &[u8], over_udp: bool) -> Option<Vec<u8>> {
        let query = Message::from_vec(bytes).ok()?;
        let question = query.queries.first()?.clone();
        let qname = question.name().to_lowercase();
        let qtype = question.query_type();
        let recursion = query.metadata.recursion_desired;
        let mut state = self.lock();
        state.asked.push((
            qname.to_ascii().trim_end_matches('.').to_owned(),
            qtype,
            recursion,
        ));
        if state.silent.contains(&qname) {
            return None;
        }
        let mut answer = Message::response(query.metadata.id, query.metadata.op_code);
        answer.metadata.message_type = MessageType::Response;
        answer.metadata.recursion_desired = recursion;
        answer.add_query(question.clone());
        if over_udp && state.truncated.contains(&qname) {
            answer.metadata.truncation = true;
            return answer.to_vec().ok();
        }
        let Some(zone) = zone_for(&state.zones, &qname) else {
            answer.metadata.response_code = ResponseCode::NXDomain;
            return answer.to_vec().ok();
        };
        answer.metadata.authoritative = true;
        let soa = state
            .records
            .iter()
            .find(|record| record.name == zone && record.record_type() == RecordType::SOA)
            .cloned();
        let exists = state.records.iter().any(|record| record.name == qname);
        match qtype {
            RecordType::SOA | RecordType::NS if qname == zone => {
                for record in state.records.iter().filter(|r| r.name == zone) {
                    if record.record_type() == qtype {
                        answer.add_answer(record.clone());
                    }
                }
                if qtype == RecordType::NS {
                    for record in &state.records {
                        let glue = state.records.iter().any(
                            |ns| matches!(&ns.data, RData::NS(NS(host)) if *host == record.name),
                        );
                        if glue && record.data.ip_addr().is_some() {
                            answer.add_additional(record.clone());
                        }
                    }
                }
            }
            _ => {
                let mut current = qname.clone();
                for _ in 0..16 {
                    let in_zone = zone_for(&state.zones, &current) == Some(zone.clone());
                    if !in_zone && !recursion {
                        break;
                    }
                    let cname = state.records.iter().find(|record| {
                        record.name == current && record.record_type() == RecordType::CNAME
                    });
                    match cname {
                        Some(record) => {
                            answer.add_answer(record.clone());
                            let RData::CNAME(CNAME(target)) = &record.data else {
                                unreachable!()
                            };
                            current = target.to_lowercase();
                        }
                        None => {
                            for record in state.records.iter().filter(|record| {
                                record.name == current && record.record_type() == qtype
                            }) {
                                answer.add_answer(record.clone());
                            }
                            break;
                        }
                    }
                }
                if answer.answers.is_empty() {
                    if !exists {
                        answer.metadata.response_code = ResponseCode::NXDomain;
                    }
                    if let Some(soa) = soa {
                        answer.add_authority(soa);
                    }
                }
            }
        }
        answer.to_vec().ok()
    }
}

/// The deepest zone `name` is in.
fn zone_for(zones: &[Name], name: &Name) -> Option<Name> {
    zones
        .iter()
        .filter(|zone| zone.zone_of(name))
        .max_by_key(|zone| zone.num_labels())
        .cloned()
}
