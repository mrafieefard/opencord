//! `http.request` (spec §10.3, §10.7): a request to one of the manifest's
//! domains, through the server's guard in `plugins::net`.

use std::sync::Arc;
use std::time::Duration;

use mlua::{Lua, Table, Value};
use opencord_plugins::limits::{HTTP_MAX_REQUEST_BYTES, HTTP_TIMEOUT};
use opencord_plugins::sandbox::paused;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Method, Url};

use super::{async_function, raise, string_arg};
use crate::plugins::net::{self, HttpRequest, HttpResponse, NetError};
use crate::plugins::permissions::PluginPermission;
use crate::plugins::runtime::PluginEnv;

/// The methods `http.request` sends.
const METHODS: [&str; 7] = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];

const MB: u64 = 1024 * 1024;

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let http = lua.create_table()?;
    let env = env.clone();
    http.raw_set(
        "request",
        async_function(lua, move |lua, options: Value| {
            let env = env.clone();
            // Who asks first, then what they ask for.
            let request = env
                .require(PluginPermission::Internet)
                .and_then(|()| parse(options));
            async move {
                let response = paused(&env.budget, net::request(&env, request?))
                    .await
                    .map_err(raised)?;
                response_table(&lua, response)
            }
        })?,
    )?;
    http.set_readonly(true);
    lua.globals().raw_set("http", http)
}

/// `{ method?, url, headers?, body?, timeout? }`, checked.
fn parse(options: Value) -> mlua::Result<HttpRequest> {
    let Value::Table(options) = options else {
        return Err(raise(
            "INVALID_ARGUMENT: http.request takes a table such as { url = \"https://…\" }",
        ));
    };
    let method = match options.raw_get::<Value>("method")? {
        Value::Nil => Method::GET,
        value => method(&string_arg(value, "method")?)?,
    };
    let url = string_arg(options.raw_get("url")?, "url")?;
    let url = Url::parse(&url).map_err(|error| {
        raise(format!(
            "INVALID_ARGUMENT: url isn't a valid address: {error}"
        ))
    })?;
    let headers = match options.raw_get::<Value>("headers")? {
        Value::Nil => HeaderMap::new(),
        Value::Table(headers) => headers_from(&headers)?,
        _ => return Err(raise("INVALID_ARGUMENT: headers must be a table")),
    };
    let body = match options.raw_get::<Value>("body")? {
        Value::Nil => None,
        Value::String(body) => {
            let body = body.as_bytes();
            if body.len() as u64 > HTTP_MAX_REQUEST_BYTES {
                return Err(raise(format!(
                    "INVALID_ARGUMENT: the body is larger than {} MB",
                    HTTP_MAX_REQUEST_BYTES / MB
                )));
            }
            Some(body.to_vec())
        }
        _ => return Err(raise("INVALID_ARGUMENT: body must be a string")),
    };
    let timeout = match options.raw_get::<Value>("timeout")? {
        Value::Nil => HTTP_TIMEOUT,
        Value::Integer(seconds) => seconds_arg(seconds as f64)?,
        Value::Number(seconds) => seconds_arg(seconds)?,
        _ => return Err(seconds_error()),
    };
    Ok(HttpRequest {
        method,
        url,
        headers,
        body,
        timeout,
    })
}

fn method(name: &str) -> mlua::Result<Method> {
    let name = name.to_ascii_uppercase();
    let refused = || {
        raise(format!(
            "INVALID_ARGUMENT: method must be one of {}",
            METHODS.join(", ")
        ))
    };
    if !METHODS.contains(&name.as_str()) {
        return Err(refused());
    }
    Method::from_bytes(name.as_bytes()).map_err(|_| refused())
}

fn headers_from(table: &Table) -> mlua::Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    for pair in table.pairs::<Value, Value>() {
        let (name, value) = pair?;
        let name = string_arg(name, "a header's name")?;
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            raise(format!(
                "INVALID_ARGUMENT: {name} isn't a valid header name"
            ))
        })?;
        if !net::may_set_header(&name) {
            return Err(raise(format!(
                "INVALID_ARGUMENT: the plugin may not set the {name} header"
            )));
        }
        let Value::String(value) = value else {
            return Err(raise(format!(
                "INVALID_ARGUMENT: the {name} header must be a string"
            )));
        };
        let value = HeaderValue::from_bytes(&value.as_bytes()).map_err(|_| {
            raise(format!(
                "INVALID_ARGUMENT: the {name} header's value isn't valid"
            ))
        })?;
        headers.append(name, value);
    }
    Ok(headers)
}

fn seconds_arg(seconds: f64) -> mlua::Result<Duration> {
    if seconds.is_finite() && seconds > 0.0 && seconds <= HTTP_TIMEOUT.as_secs_f64() {
        Ok(Duration::from_secs_f64(seconds))
    } else {
        Err(seconds_error())
    }
}

fn seconds_error() -> mlua::Error {
    raise(format!(
        "INVALID_ARGUMENT: timeout must be more than 0 and at most {} seconds",
        HTTP_TIMEOUT.as_secs()
    ))
}

/// `{ status, headers, body }`; a header sent more than once has its
/// values joined with `", "`.
fn response_table(lua: &Lua, response: HttpResponse) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set("status", response.status)?;
    let headers = lua.create_table()?;
    for name in response.headers.keys() {
        let values: Vec<&[u8]> = response
            .headers
            .get_all(name)
            .iter()
            .map(HeaderValue::as_bytes)
            .collect();
        headers.raw_set(name.as_str(), lua.create_string(values.join(&b", "[..]))?)?;
    }
    table.raw_set("headers", headers)?;
    table.raw_set("body", lua.create_string(&response.body)?)?;
    Ok(table)
}

fn raised(error: NetError) -> mlua::Error {
    match &error {
        NetError::NotInDomains(_) => raise(format!("FORBIDDEN: {error}")),
        NetError::Network(_) => raise(format!("NETWORK: {error}")),
    }
}
