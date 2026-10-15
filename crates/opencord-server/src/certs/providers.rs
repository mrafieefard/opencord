//! DNS providers (Phase 3 plan §7.2, §9.1): Cloudflare, DuckDNS and deSEC,
//! for the DNS-01 challenge's TXT record, and (D4) for keeping a free
//! subdomain pointed at the server.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Cloudflare,
    DuckDns,
    Desec,
}

impl ProviderKind {
    pub const ALL: [Self; 3] = [Self::Cloudflare, Self::DuckDns, Self::Desec];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cloudflare => "cloudflare",
            Self::DuckDns => "duckdns",
            Self::Desec => "desec",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cloudflare => "Cloudflare",
            Self::DuckDns => "DuckDNS",
            Self::Desec => "deSEC",
        }
    }
}

/// The providers' API addresses.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub cloudflare: String,
    pub duckdns: String,
    pub desec: String,
}

/// A TXT record set, to remove again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxtRecord {
    Cloudflare { zone_id: String, record_id: String },
    DuckDns { subdomain: String },
    Desec { domain: String, subname: String },
}

#[derive(Debug, Clone)]
pub struct Provider {
    pub kind: ProviderKind,
    pub token: String,
}

impl Provider {
    /// Sets the TXT record `name` (a full name, like
    /// `_acme-challenge.chat.example.com`) to `value`, if this provider has
    /// the name; `Ok(None)` when it does not.
    pub async fn set_txt(
        &self,
        http: &reqwest::Client,
        endpoints: &Endpoints,
        name: &str,
        value: &str,
    ) -> Result<Option<TxtRecord>, String> {
        match self.kind {
            ProviderKind::Cloudflare => {
                let Some(zone) = self.cloudflare_zone(http, endpoints, name).await? else {
                    return Ok(None);
                };
                let created: CloudflareResponse<CloudflareId> = self
                    .cloudflare(
                        http,
                        reqwest::Method::POST,
                        &format!("{}/zones/{}/dns_records", endpoints.cloudflare, zone.id),
                    )
                    .json(&json!({ "type": "TXT", "name": name, "content": value, "ttl": 60 }))
                    .send()
                    .await
                    .map_err(plain)?
                    .json()
                    .await
                    .map_err(plain)?;
                let record = created.ok("create the TXT record")?;
                Ok(Some(TxtRecord::Cloudflare {
                    zone_id: zone.id,
                    record_id: record.id,
                }))
            }
            ProviderKind::DuckDns => {
                let Some(subdomain) = duckdns_subdomain(name) else {
                    return Ok(None);
                };
                self.duckdns(http, endpoints, &subdomain, &[("txt", value)])
                    .await?;
                Ok(Some(TxtRecord::DuckDns { subdomain }))
            }
            ProviderKind::Desec => {
                let Some((domain, subname)) = self.desec_split(http, endpoints, name).await? else {
                    return Ok(None);
                };
                self.desec_txt(http, endpoints, &domain, &subname, &[value])
                    .await?;
                Ok(Some(TxtRecord::Desec { domain, subname }))
            }
        }
    }

    pub async fn clear_txt(
        &self,
        http: &reqwest::Client,
        endpoints: &Endpoints,
        record: &TxtRecord,
    ) -> Result<(), String> {
        match record {
            TxtRecord::Cloudflare { zone_id, record_id } => {
                let response = self
                    .cloudflare(
                        http,
                        reqwest::Method::DELETE,
                        &format!(
                            "{}/zones/{zone_id}/dns_records/{record_id}",
                            endpoints.cloudflare
                        ),
                    )
                    .send()
                    .await
                    .map_err(plain)?;
                if !response.status().is_success() {
                    return Err(format!("Cloudflare answered {}", response.status()));
                }
                Ok(())
            }
            TxtRecord::DuckDns { subdomain } => {
                self.duckdns(
                    http,
                    endpoints,
                    subdomain,
                    &[("clear", "true"), ("txt", "")],
                )
                .await
            }
            TxtRecord::Desec { domain, subname } => {
                self.desec_txt(http, endpoints, domain, subname, &[]).await
            }
        }
    }

    /// Whether this provider holds `name`'s zone.
    pub async fn has(
        &self,
        http: &reqwest::Client,
        endpoints: &Endpoints,
        name: &str,
    ) -> Result<bool, String> {
        match self.kind {
            ProviderKind::Cloudflare => {
                Ok(self.cloudflare_zone(http, endpoints, name).await?.is_some())
            }
            ProviderKind::DuckDns => Ok(duckdns_subdomain(name).is_some()),
            ProviderKind::Desec => Ok(self.desec_split(http, endpoints, name).await?.is_some()),
        }
    }

    /// Points a free subdomain at `ip` (D4: dynamic DNS).
    pub async fn set_address(
        &self,
        http: &reqwest::Client,
        endpoints: &Endpoints,
        host: &str,
        ip: std::net::IpAddr,
    ) -> Result<(), String> {
        match self.kind {
            ProviderKind::DuckDns => {
                let subdomain = duckdns_subdomain(host)
                    .ok_or_else(|| format!("{host} is not a DuckDNS name"))?;
                let field = if ip.is_ipv4() { "ip" } else { "ipv6" };
                self.duckdns(http, endpoints, &subdomain, &[(field, &ip.to_string())])
                    .await
            }
            ProviderKind::Desec => {
                let (domain, subname) = self
                    .desec_split(http, endpoints, host)
                    .await?
                    .ok_or_else(|| format!("{host} is not one of your deSEC domains"))?;
                let kind = if ip.is_ipv4() { "A" } else { "AAAA" };
                self.desec_rrset(
                    http,
                    endpoints,
                    &domain,
                    &subname,
                    kind,
                    &[&ip.to_string()],
                    3600,
                )
                .await
            }
            ProviderKind::Cloudflare => Err("Cloudflare names are not kept updated here".into()),
        }
    }

    /// Whether the token works: Cloudflare and deSEC say so; DuckDNS is
    /// tried when it is used.
    pub async fn verify(
        &self,
        http: &reqwest::Client,
        endpoints: &Endpoints,
    ) -> Result<(), String> {
        match self.kind {
            ProviderKind::Cloudflare => {
                let answer: CloudflareResponse<Value> = self
                    .cloudflare(
                        http,
                        reqwest::Method::GET,
                        &format!("{}/user/tokens/verify", endpoints.cloudflare),
                    )
                    .send()
                    .await
                    .map_err(plain)?
                    .json()
                    .await
                    .map_err(plain)?;
                answer.ok("check the token").map(|_| ())
            }
            ProviderKind::Desec => {
                let response = self
                    .desec(
                        http,
                        reqwest::Method::GET,
                        &format!("{}/domains/", endpoints.desec),
                    )
                    .send()
                    .await
                    .map_err(plain)?;
                if response.status().is_success() {
                    Ok(())
                } else {
                    Err(format!("deSEC refused the token ({})", response.status()))
                }
            }
            ProviderKind::DuckDns => {
                if self.token.len() >= 32 && !self.token.contains(char::is_whitespace) {
                    Ok(())
                } else {
                    Err("a DuckDNS token is 36 characters, like 1a2b3c4d-....".into())
                }
            }
        }
    }

    fn cloudflare(
        &self,
        http: &reqwest::Client,
        method: reqwest::Method,
        url: &str,
    ) -> reqwest::RequestBuilder {
        http.request(method, url)
            .bearer_auth(&self.token)
            .timeout(TIMEOUT)
    }

    /// The Cloudflare zone `name` is in: its longest suffix the token sees.
    async fn cloudflare_zone(
        &self,
        http: &reqwest::Client,
        endpoints: &Endpoints,
        name: &str,
    ) -> Result<Option<CloudflareZone>, String> {
        let labels: Vec<&str> = name.trim_end_matches('.').split('.').collect();
        for start in 0..labels.len().saturating_sub(1) {
            let candidate = labels[start..].join(".");
            let found: CloudflareResponse<Vec<CloudflareZone>> = self
                .cloudflare(
                    http,
                    reqwest::Method::GET,
                    &format!("{}/zones", endpoints.cloudflare),
                )
                .query(&[("name", candidate.as_str())])
                .send()
                .await
                .map_err(plain)?
                .json()
                .await
                .map_err(plain)?;
            if let Some(zone) = found.ok("find the zone")?.into_iter().next() {
                return Ok(Some(zone));
            }
        }
        Ok(None)
    }

    async fn duckdns(
        &self,
        http: &reqwest::Client,
        endpoints: &Endpoints,
        subdomain: &str,
        extra: &[(&str, &str)],
    ) -> Result<(), String> {
        let mut query = vec![("domains", subdomain), ("token", self.token.as_str())];
        query.extend_from_slice(extra);
        let answer = http
            .get(format!("{}/update", endpoints.duckdns))
            .query(&query)
            .timeout(TIMEOUT)
            .send()
            .await
            .map_err(plain)?
            .text()
            .await
            .map_err(plain)?;
        if answer.trim().starts_with("OK") {
            Ok(())
        } else {
            Err("DuckDNS refused: check the token and the subdomain".into())
        }
    }

    fn desec(
        &self,
        http: &reqwest::Client,
        method: reqwest::Method,
        url: &str,
    ) -> reqwest::RequestBuilder {
        http.request(method, url)
            .header("Authorization", format!("Token {}", self.token))
            .timeout(TIMEOUT)
    }

    /// The deSEC domain `name` is in, and the rest of the name.
    async fn desec_split(
        &self,
        http: &reqwest::Client,
        endpoints: &Endpoints,
        name: &str,
    ) -> Result<Option<(String, String)>, String> {
        let response = self
            .desec(
                http,
                reqwest::Method::GET,
                &format!("{}/domains/", endpoints.desec),
            )
            .send()
            .await
            .map_err(plain)?;
        if !response.status().is_success() {
            return Err(format!("deSEC refused the token ({})", response.status()));
        }
        let domains: Vec<DesecDomain> = response.json().await.map_err(plain)?;
        let name = name.trim_end_matches('.').to_ascii_lowercase();
        Ok(domains
            .into_iter()
            .filter(|domain| name == domain.name || name.ends_with(&format!(".{}", domain.name)))
            .max_by_key(|domain| domain.name.len())
            .map(|domain| {
                let subname = name
                    .strip_suffix(&domain.name)
                    .unwrap_or_default()
                    .trim_end_matches('.')
                    .to_owned();
                (domain.name, subname)
            }))
    }

    async fn desec_txt(
        &self,
        http: &reqwest::Client,
        endpoints: &Endpoints,
        domain: &str,
        subname: &str,
        values: &[&str],
    ) -> Result<(), String> {
        let quoted: Vec<String> = values.iter().map(|value| format!("\"{value}\"")).collect();
        let records: Vec<&str> = quoted.iter().map(String::as_str).collect();
        self.desec_rrset(http, endpoints, domain, subname, "TXT", &records, 3600)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn desec_rrset(
        &self,
        http: &reqwest::Client,
        endpoints: &Endpoints,
        domain: &str,
        subname: &str,
        kind: &str,
        records: &[&str],
        ttl: u32,
    ) -> Result<(), String> {
        let response = self
            .desec(
                http,
                reqwest::Method::PATCH,
                &format!("{}/domains/{domain}/rrsets/", endpoints.desec),
            )
            .json(&json!([{ "subname": subname, "type": kind, "ttl": ttl, "records": records }]))
            .send()
            .await
            .map_err(plain)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("deSEC refused the record ({})", response.status()))
        }
    }
}

/// `x` for any name under `x.duckdns.org`.
fn duckdns_subdomain(name: &str) -> Option<String> {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    let rest = name.strip_suffix(".duckdns.org")?;
    rest.rsplit('.')
        .next()
        .filter(|label| !label.is_empty())
        .map(str::to_owned)
}

#[derive(Deserialize)]
struct CloudflareResponse<T> {
    success: bool,
    #[serde(default)]
    errors: Vec<CloudflareError>,
    result: Option<T>,
}

impl<T> CloudflareResponse<T> {
    fn ok(self, what: &str) -> Result<T, String> {
        match (self.success, self.result) {
            (true, Some(result)) => Ok(result),
            _ => Err(format!(
                "Cloudflare could not {what}: {}",
                self.errors
                    .iter()
                    .map(|error| error.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            )),
        }
    }
}

#[derive(Deserialize)]
struct CloudflareError {
    message: String,
}

#[derive(Deserialize)]
struct CloudflareZone {
    id: String,
}

#[derive(Deserialize)]
struct CloudflareId {
    id: String,
}

#[derive(Deserialize)]
struct DesecDomain {
    name: String,
}

fn plain(error: reqwest::Error) -> String {
    let mut source: &dyn std::error::Error = &error;
    while let Some(inner) = source.source() {
        source = inner;
    }
    format!("the DNS provider could not be reached: {source}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duckdns_names_are_their_subdomain() {
        assert_eq!(
            duckdns_subdomain("_acme-challenge.chat.duckdns.org"),
            Some("chat".into())
        );
        assert_eq!(duckdns_subdomain("chat.duckdns.org."), Some("chat".into()));
        assert_eq!(duckdns_subdomain("duckdns.org"), None);
        assert_eq!(duckdns_subdomain("chat.example.com"), None);
    }
}
