//! Free subdomains (Phase 3 plan §9.1): a DuckDNS or deSEC name that the
//! server points at itself, keeps pointed at itself when its public address
//! changes, verifies, and gets a certificate for through the DNS challenge.

use std::net::IpAddr;
use std::sync::Arc;

use opencord_proto::v1 as proto;

use super::{DomainKind, hostname};
use crate::certs::providers::{Provider, ProviderKind};
use crate::error::ApiError;
use crate::state::AppState;

/// The free subdomain's full name: `name` under the provider's domain.
pub fn hostname_for(kind: ProviderKind, name: &str) -> Result<String, ApiError> {
    let suffix = match kind {
        ProviderKind::DuckDns => ".duckdns.org",
        ProviderKind::Desec => ".dedyn.io",
        ProviderKind::Cloudflare => {
            return Err(ApiError::invalid_argument(
                "free subdomains come from DuckDNS or deSEC",
            ));
        }
    };
    let name = name.trim().trim_end_matches('.').to_ascii_lowercase();
    let label = name.strip_suffix(suffix).unwrap_or(&name);
    let valid = !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(ApiError::new(
            proto::ErrorCode::DomainInvalid,
            format!(
                "{label} is not a {} name: letters, digits and hyphens",
                kind.label()
            ),
        ));
    }
    hostname::normalize(&format!("{label}{suffix}"))
        .map_err(|error| ApiError::new(proto::ErrorCode::DomainInvalid, error.to_string()))
}

/// Adds a free subdomain: connects the provider when a token comes with
/// it, points the name here, then checks it until it shows, and gets its
/// certificate.
pub async fn add(state: &Arc<AppState>, request: proto::AddFreeSubdomain) -> Result<(), ApiError> {
    let kind = match ProviderKind::parse(request.provider.trim()) {
        Some(kind @ (ProviderKind::DuckDns | ProviderKind::Desec)) => kind,
        _ => {
            return Err(ApiError::invalid_argument(
                "free subdomains come from DuckDNS or deSEC",
            ));
        }
    };
    let host = hostname_for(kind, &request.name)?;
    let certificates = &state.certificates;
    let token = request.token.trim();
    // A new token is saved once the provider took the update with it.
    let provider = if token.is_empty() {
        certificates.provider(kind).ok_or_else(|| {
            ApiError::invalid_argument(format!("paste your {} token", kind.label()))
        })?
    } else {
        Provider {
            kind,
            token: token.to_owned(),
        }
    };
    if !certificates.may_request(state) && !request.agree_terms {
        return Err(ApiError::new(
            proto::ErrorCode::AcmeFailed,
            "agree to Let's Encrypt's subscriber agreement first: the domain gets its \
             certificate by itself",
        ));
    }
    let ips = public_ips(state);
    if ips.is_empty() {
        return Err(ApiError::invalid_argument(
            "this server's public address is not known yet: detect it or set it first",
        ));
    }
    let email = request
        .email
        .map(|email| email.trim().to_owned())
        .filter(|email| !email.is_empty());
    if let Some(email) = &email
        && (!email.contains('@') || email.len() > 254 || email.contains(char::is_whitespace))
    {
        return Err(ApiError::invalid_argument(format!(
            "{email} is not an email address"
        )));
    }
    let domain = state
        .domains
        .add(state, &host, DomainKind::FreeSubdomain, None)
        .await?;
    let http = state.network.http();
    let endpoints = crate::certs::endpoints(state);
    for ip in ips {
        if let Err(error) = provider.set_address(http, &endpoints, &host, ip).await {
            // Nothing points anywhere yet: take the domain back.
            let _ = state.domains.remove_own(state, domain.id).await;
            return Err(ApiError::invalid_argument(format!(
                "{}: {error}",
                kind.label()
            )));
        }
    }
    if !token.is_empty()
        && let Err(error) = certificates.set_provider(state, kind, token).await
    {
        let _ = state.domains.remove_own(state, domain.id).await;
        return Err(error);
    }
    if request.agree_terms {
        certificates.agree(email);
    }
    tracing::info!(hostname = %host, provider = kind.as_str(), "a free subdomain was added");
    super::check_until_verified(state, domain.id);
    Ok(())
}

/// The addresses to point free subdomains at.
fn public_ips(state: &AppState) -> Vec<IpAddr> {
    let ips = *state.network.public_ips().borrow();
    ips.v4
        .map(IpAddr::V4)
        .into_iter()
        .chain(ips.v6.map(IpAddr::V6))
        .collect()
}

/// Points every free subdomain at the public address in use (dynamic DNS):
/// when it changes, and before the regular checks.
pub async fn follow(state: &Arc<AppState>) {
    let ips = public_ips(state);
    if ips.is_empty() {
        return;
    }
    let http = state.network.http();
    let endpoints = crate::certs::endpoints(state);
    for domain in state.domains.all() {
        if domain.kind != DomainKind::FreeSubdomain {
            continue;
        }
        let Some(kind) = provider_of(&domain.hostname) else {
            continue;
        };
        let Some(provider) = state.certificates.provider(kind) else {
            tracing::warn!(hostname = %domain.hostname, "no {} token to keep it pointed here", kind.label());
            continue;
        };
        for ip in &ips {
            if let Err(error) = provider
                .set_address(http, &endpoints, &domain.hostname, *ip)
                .await
            {
                tracing::warn!(hostname = %domain.hostname, %error, "a free subdomain could not be pointed here");
            }
        }
    }
}

/// Which provider a free subdomain is with, by its name.
pub fn provider_of(host: &str) -> Option<ProviderKind> {
    if host.ends_with(".duckdns.org") {
        Some(ProviderKind::DuckDns)
    } else if host.ends_with(".dedyn.io") {
        Some(ProviderKind::Desec)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_go_under_the_providers_domain() {
        assert_eq!(
            hostname_for(ProviderKind::DuckDns, " MyChat ").unwrap(),
            "mychat.duckdns.org"
        );
        assert_eq!(
            hostname_for(ProviderKind::DuckDns, "mychat.duckdns.org").unwrap(),
            "mychat.duckdns.org"
        );
        assert_eq!(
            hostname_for(ProviderKind::Desec, "my-chat").unwrap(),
            "my-chat.dedyn.io"
        );
        for bad in ["", "-x", "a.b", "my_chat", "x-"] {
            assert!(hostname_for(ProviderKind::DuckDns, bad).is_err(), "{bad}");
        }
        assert!(hostname_for(ProviderKind::Cloudflare, "x").is_err());
    }
}
