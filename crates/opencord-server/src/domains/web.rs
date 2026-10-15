//! What the server answers in a browser (Phase 3 plan §6.2, §7.6): the
//! token domain checks look for, and a small page for invite links, with no
//! JavaScript, under 20 KB, monochrome.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, HOST};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use opencord_common::address::{InviteLink, ServerAddress};

use crate::db::invites;
use crate::state::{AppState, now_ms};

/// `/.well-known/opencord/verify`: the token, for tunnel and proxy checks.
pub async fn verify(State(state): State<Arc<AppState>>) -> Response {
    (
        [(CONTENT_TYPE, "text/plain"), (CACHE_CONTROL, "no-store")],
        state.domains.verify_token(),
    )
        .into_response()
}

/// `/invite/{code}`: the server's name, its member count, and a button to
/// open the invite in the app.
pub async fn invite_page(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Response {
    let usable = match state.db.acquire().await {
        Ok(mut conn) => invites::find(&mut conn, &code)
            .await
            .ok()
            .flatten()
            .filter(|invite| invite.is_usable(now_ms()))
            .filter(|invite| {
                invite
                    .domain_id
                    .is_none_or(|id| state.domains.get(id).is_some_and(|d| d.active))
            }),
        Err(_) => None,
    };
    let (name, members) = {
        let guild = state.guild();
        (guild.meta.name.clone(), guild.members.len())
    };
    let Some(invite) = usable else {
        return page(
            StatusCode::NOT_FOUND,
            &name,
            "This invite is no longer valid",
            "Ask whoever sent it for a new one.",
            None,
        );
    };
    let host = headers
        .get(HOST)
        .and_then(|host| host.to_str().ok())
        .unwrap_or_default();
    let link = app_link(&state, host, &invite.code);
    let members = match members {
        1 => "1 member".to_owned(),
        count => format!("{count} members"),
    };
    page(
        StatusCode::OK,
        &name,
        &format!("You're invited to {name}"),
        &members,
        link.as_deref(),
    )
}

/// The `opencord://` link for the address the page was opened on: the
/// fingerprint goes along unless the domain has an authority's certificate.
fn app_link(state: &AppState, host: &str, code: &str) -> Option<String> {
    let mut address = ServerAddress::parse(host).ok()?;
    if !host.trim_end().ends_with(&format!(":{}", address.port)) {
        // No port in the Host header: the browser came on 443.
        address.port = 443;
    }
    let trusted = state
        .domains
        .active_named(&address.host)
        .is_some_and(|domain| domain.trusted_certificate());
    let link = InviteLink {
        address,
        code: code.to_owned(),
        fingerprint: (!trusted).then_some(state.fingerprint),
    };
    Some(link.to_string())
}

fn page(status: StatusCode, server: &str, title: &str, line: &str, link: Option<&str>) -> Response {
    let download = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");
    let button = link.map_or_else(String::new, |link| {
        format!(
            r#"<a class="primary" href="{}">Open in Opencord</a>"#,
            escape(link)
        )
    });
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>{title} · Opencord</title>
<style>
:root {{ color-scheme: light dark; --fg: #111; --bg: #fff; --muted: #666; --line: #ddd; }}
@media (prefers-color-scheme: dark) {{ :root {{ --fg: #eee; --bg: #111; --muted: #999; --line: #333; }} }}
body {{ margin: 0; min-height: 100vh; display: grid; place-items: center; background: var(--bg); color: var(--fg); font: 16px/1.5 system-ui, sans-serif; }}
main {{ max-width: 360px; padding: 32px; border: 1px solid var(--line); border-radius: 16px; text-align: center; }}
.mark {{ width: 56px; height: 56px; margin: 0 auto 16px; border-radius: 16px; background: var(--fg); color: var(--bg); display: grid; place-items: center; font-weight: 700; font-size: 24px; }}
h1 {{ font-size: 20px; margin: 0 0 4px; }}
p {{ margin: 0 0 24px; color: var(--muted); }}
a {{ display: block; padding: 12px 16px; border-radius: 10px; text-decoration: none; color: var(--fg); border: 1px solid var(--line); margin-top: 8px; }}
a.primary {{ background: var(--fg); color: var(--bg); border-color: var(--fg); font-weight: 600; }}
</style>
</head>
<body>
<main>
<div class="mark" aria-hidden="true">{initial}</div>
<h1>{title}</h1>
<p>{line}</p>
{button}
<a href="{download}">Get Opencord</a>
</main>
</body>
</html>
"#,
        title = escape(title),
        line = escape(line),
        initial = escape(
            &server
                .chars()
                .next()
                .unwrap_or('O')
                .to_uppercase()
                .to_string()
        ),
    );
    (
        status,
        [
            (CONTENT_TYPE, "text/html; charset=utf-8"),
            (CACHE_CONTROL, "no-store"),
        ],
        html,
    )
        .into_response()
}

fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&#39;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_escaped() {
        assert_eq!(
            escape(r#"<b>"Tom's"</b> & co"#),
            "&lt;b&gt;&quot;Tom&#39;s&quot;&lt;/b&gt; &amp; co"
        );
    }
}
