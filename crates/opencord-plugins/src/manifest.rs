//! `manifest.lua` (spec §7.2): run in a VM with nothing in it, then checked.

use std::collections::HashSet;
use std::fmt;
use std::net::{IpAddr, Ipv6Addr};

use mlua::{ChunkMode, Table, Value};
use serde::{Deserialize, Serialize};

use crate::API_VERSION;
use crate::limits::{MANIFEST_MEMORY_BYTES, MANIFEST_TIME};
use crate::package::{Package, check_path, shown};
use crate::permissions;
use crate::sandbox::{Vm, VmError, VmLimits};

/// What a plugin says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: semver::Version,
    pub author: String,
    pub description: String,
    pub github: Option<GithubRepo>,
    pub license: String,
    pub api: u32,
    pub icon: Option<String>,
    pub public_key: Option<String>,
    pub permissions: Vec<String>,
    pub domains: Vec<String>,
    pub config: Vec<ConfigField>,
    pub server: Vec<String>,
    pub client: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubRepo {
    pub owner: String,
    pub repo: String,
}

/// A setting admins fill in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    pub kind: ConfigKind,
    pub choices: Vec<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigKind {
    Text,
    Secret,
    Number,
    Boolean,
    Choice,
    Role,
    Channel,
    User,
}

const KINDS: &[(&str, ConfigKind)] = &[
    ("text", ConfigKind::Text),
    ("secret", ConfigKind::Secret),
    ("number", ConfigKind::Number),
    ("boolean", ConfigKind::Boolean),
    ("choice", ConfigKind::Choice),
    ("role", ConfigKind::Role),
    ("channel", ConfigKind::Channel),
    ("user", ConfigKind::User),
];

/// Why a manifest was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// `manifest.lua` failed while it ran.
    Failed(String),
    TooLong,
    NotATable,
    NewerApi(u32),
    /// A field broke a rule.
    Invalid {
        field: String,
        problem: String,
    },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Failed(error) => write!(f, "manifest: {error}"),
            ManifestError::TooLong => write!(
                f,
                "manifest: it ran longer than {} ms",
                MANIFEST_TIME.as_millis()
            ),
            ManifestError::NotATable => f.write_str("manifest: it didn't return a table"),
            ManifestError::NewerApi(api) => {
                write!(f, "this plugin needs a newer Opencord (plugin API {api})")
            }
            ManifestError::Invalid { field, problem } => write!(f, "manifest: {field} {problem}"),
        }
    }
}

impl std::error::Error for ManifestError {}

impl From<VmError> for ManifestError {
    fn from(error: VmError) -> Self {
        match error {
            VmError::BudgetExceeded { .. } => ManifestError::TooLong,
            other => ManifestError::Failed(other.to_string()),
        }
    }
}

impl From<mlua::Error> for ManifestError {
    fn from(error: mlua::Error) -> Self {
        VmError::from(error).into()
    }
}

fn invalid(field: impl Into<String>, problem: impl Into<String>) -> ManifestError {
    ManifestError::Invalid {
        field: field.into(),
        problem: problem.into(),
    }
}

/// The only globals `manifest.lua` sees.
const MANIFEST_GLOBALS: &[&str] = &["string", "table", "math"];

const FIELDS: &[&str] = &[
    "id",
    "name",
    "version",
    "author",
    "description",
    "github",
    "license",
    "api",
    "icon",
    "public_key",
    "permissions",
    "domains",
    "config",
    "server",
    "client",
];
const CONFIG_FIELDS: &[&str] = &["key", "label", "kind", "choices", "required"];

const NAME_CHARS: usize = 60;
const DESCRIPTION_CHARS: usize = 300;
const LICENSE_CHARS: usize = 100;
const VERSION_CHARS: usize = 64;
const MAX_DOMAINS: usize = 20;
const HOST_MAX_BYTES: usize = 253;
const LABEL_MAX_BYTES: usize = 63;
const MAX_CONFIG: usize = 30;
const CONFIG_KEY_MAX_BYTES: usize = 40;
const MAX_CHOICES: usize = 50;
const MAX_SCRIPTS: usize = 50;
const GITHUB_OWNER_MAX_BYTES: usize = 39;
const GITHUB_REPO_MAX_BYTES: usize = 100;
const PUBLIC_KEY_CHARS: usize = 43;
/// The base64url characters whose two low bits are zero: the last of 43
/// characters holding 32 bytes carries only four bits.
const KEY_LAST_CHARS: &str = "AEIMQUYcgkosw048";
const ICONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

/// Runs `manifest.lua` and checks what it returns.
pub fn evaluate(source: &str) -> Result<Manifest, ManifestError> {
    let vm = Vm::new(&VmLimits {
        memory_bytes: MANIFEST_MEMORY_BYTES,
        handler_time: MANIFEST_TIME,
    })?;
    let Value::Table(table) = run(&vm, source)? else {
        return Err(ManifestError::NotATable);
    };
    read(&table)
}

fn run(vm: &Vm, source: &str) -> Result<Value, ManifestError> {
    let globals = vm.lua.globals();
    let env = vm.lua.create_table()?;
    for name in MANIFEST_GLOBALS {
        env.raw_set(*name, globals.get::<Value>(*name)?)?;
    }
    // Loaded in its own environment: the VM's globals (os, json, print…)
    // stay out of reach. Text only, as `Vm::load`.
    let function = vm
        .lua
        .load(source)
        .set_name("=manifest.lua")
        .set_mode(ChunkMode::Text)
        .set_environment(env)
        .into_function()?;
    vm.budget().start(MANIFEST_TIME);
    let result = function.call::<Value>(());
    vm.budget().stop();
    Ok(result?)
}

/// Reads the returned table. Reads are raw, and the budget is stopped, so
/// no Lua runs here.
fn read(table: &Table) -> Result<Manifest, ManifestError> {
    // First, so a manifest for a newer Opencord says so rather than naming
    // a field this one doesn't know.
    let api = read_api(table)?;
    check_keys(table, None, FIELDS, "a manifest field")?;

    let id = required(text(table, "id")?, "id")?;
    if !valid_id(&id) {
        return Err(invalid(
            "id",
            "must be 3 to 40 lowercase letters, digits and dashes, \
             starting and ending with a letter or digit",
        ));
    }
    let name = required(text(table, "name")?, "name")?;
    check_line("name", &name, NAME_CHARS)?;
    let version = read_version(table)?;
    let author = required(text(table, "author")?, "author")?;
    check_line("author", &author, NAME_CHARS)?;
    let description = text(table, "description")?.unwrap_or_default();
    check_description(&description)?;
    let github = text(table, "github")?
        .map(|url| {
            github_repo(&url).ok_or_else(|| {
                invalid(
                    "github",
                    "must be https://github.com/<owner>/<repo>, with nothing after",
                )
            })
        })
        .transpose()?;
    let license = required(text(table, "license")?, "license")?;
    check_line("license", &license, LICENSE_CHARS)?;
    let icon = text(table, "icon")?;
    if icon.as_deref().is_some_and(|icon| !valid_icon(icon)) {
        return Err(invalid(
            "icon",
            "must be a .png, .jpg, .jpeg or .webp file under assets/",
        ));
    }
    let public_key = text(table, "public_key")?;
    if public_key.as_deref().is_some_and(|key| !valid_key(key)) {
        return Err(invalid(
            "public_key",
            "must be ed25519: and 43 base64url characters",
        ));
    }
    let permissions = read_permissions(table)?;
    let domains = read_domains(table, &permissions)?;
    let config = read_config(table)?;
    let server = read_server(table)?;
    let client = text(table, "client")?;
    if client
        .as_deref()
        .is_some_and(|client| !script_under(client, "client/"))
    {
        return Err(invalid(
            "client",
            "must be a .lua or .luau file under client/",
        ));
    }

    Ok(Manifest {
        id,
        name,
        version,
        author,
        description,
        github,
        license,
        api,
        icon,
        public_key,
        permissions,
        domains,
        config,
        server,
        client,
    })
}

/// Checks that the files the manifest names are in the package.
pub fn check_files(manifest: &Manifest, package: &Package) -> Result<(), ManifestError> {
    let named = |field: &str, path: &str| {
        if package.get(path).is_some() {
            return Ok(());
        }
        Err(invalid(
            field,
            format!("names {}, which isn't in the plugin", shown(path)),
        ))
    };
    if let Some(icon) = &manifest.icon {
        named("icon", icon)?;
    }
    for path in &manifest.server {
        named("server", path)?;
    }
    if let Some(client) = &manifest.client {
        named("client", client)?;
    }
    Ok(())
}

/// Fails on any key of `table` that isn't in `known`. `at` names the table
/// inside the manifest (`config[1]`), `None` for the manifest itself.
fn check_keys(
    table: &Table,
    at: Option<&str>,
    known: &[&str],
    what: &str,
) -> Result<(), ManifestError> {
    for pair in table.pairs::<Value, Value>() {
        let (key, _) = pair?;
        let Value::String(key) = key else {
            return Err(invalid(at.unwrap_or("it"), "has an entry without a name"));
        };
        let key = key.to_string_lossy();
        if !known.contains(&key.as_str()) {
            let field = match at {
                Some(at) => format!("{at}.{}", shown(&key)),
                None => shown(&key),
            };
            return Err(invalid(field, format!("isn't {what}")));
        }
    }
    Ok(())
}

fn required<T>(value: Option<T>, field: &str) -> Result<T, ManifestError> {
    value.ok_or_else(|| invalid(field, "is missing"))
}

fn as_text(value: Value, field: &str) -> Result<String, ManifestError> {
    let Value::String(text) = value else {
        return Err(invalid(field, "must be text"));
    };
    let text = text
        .to_str()
        .map_err(|_| invalid(field, "must be text in UTF-8"))?;
    Ok((*text).to_owned())
}

fn text(table: &Table, field: &str) -> Result<Option<String>, ManifestError> {
    match table.raw_get::<Value>(field)? {
        Value::Nil => Ok(None),
        value => as_text(value, field).map(Some),
    }
}

/// A field's items, when it is a list: keys exactly 1 to its length.
fn list(table: &Table, field: &str) -> Result<Option<Vec<Value>>, ManifestError> {
    let list = match table.raw_get::<Value>(field)? {
        Value::Nil => return Ok(None),
        Value::Table(list) => list,
        _ => return Err(invalid(field, "must be a list")),
    };
    let length = list.raw_len();
    let mut count = 0;
    for pair in list.pairs::<Value, Value>() {
        let (key, _) = pair?;
        let index = match key {
            Value::Integer(index) => index as f64,
            Value::Number(index) => index,
            _ => return Err(invalid(field, "must be a list")),
        };
        if index.fract() != 0.0 || index < 1.0 || index > length as f64 {
            return Err(invalid(field, "must be a list"));
        }
        count += 1;
    }
    if count != length {
        return Err(invalid(field, "must be a list"));
    }
    (1..=length)
        .map(|index| list.raw_get::<Value>(index).map_err(ManifestError::from))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn texts(table: &Table, field: &str) -> Result<Option<Vec<String>>, ManifestError> {
    let Some(items) = list(table, field)? else {
        return Ok(None);
    };
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| as_text(item, &format!("{field}[{}]", index + 1)))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// Text shown on one line: 1 to `max` characters, not blank, no control
/// characters.
fn check_line(field: &str, text: &str, max: usize) -> Result<(), ManifestError> {
    let count = text.chars().count();
    if count == 0 || count > max {
        return Err(invalid(field, format!("must be 1 to {max} characters")));
    }
    if text.trim().is_empty() {
        return Err(invalid(field, "can't be blank"));
    }
    if text.chars().any(char::is_control) {
        return Err(invalid(field, "can't contain control characters"));
    }
    Ok(())
}

fn check_description(text: &str) -> Result<(), ManifestError> {
    if text.chars().count() > DESCRIPTION_CHARS {
        return Err(invalid(
            "description",
            format!("must be at most {DESCRIPTION_CHARS} characters"),
        ));
    }
    if text.chars().any(|c| c.is_control() && c != '\n') {
        return Err(invalid("description", "can't contain control characters"));
    }
    Ok(())
}

fn read_api(table: &Table) -> Result<u32, ManifestError> {
    let api = match table.raw_get::<Value>("api")? {
        Value::Nil => return Err(invalid("api", "is missing")),
        Value::Integer(api) => u32::try_from(api).ok(),
        Value::Number(api) if api.fract() == 0.0 && api >= 0.0 && api <= f64::from(u32::MAX) => {
            Some(api as u32)
        }
        _ => None,
    }
    .filter(|api| *api >= 1)
    .ok_or_else(|| invalid("api", "must be a whole number, 1 or more"))?;
    if api > API_VERSION {
        return Err(ManifestError::NewerApi(api));
    }
    Ok(api)
}

fn read_version(table: &Table) -> Result<semver::Version, ManifestError> {
    let version = required(text(table, "version")?, "version")?;
    let bad = || invalid("version", "must be a semantic version, like 1.2.0");
    if version.chars().count() > VERSION_CHARS {
        return Err(bad());
    }
    semver::Version::parse(&version).map_err(|_| bad())
}

/// Whether `id` is a plugin id: `^[a-z0-9][a-z0-9-]{1,38}[a-z0-9]$`.
pub fn valid_id(id: &str) -> bool {
    let edge = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let bytes = id.as_bytes();
    (3..=40).contains(&bytes.len())
        && bytes.iter().all(|&b| edge(b) || b == b'-')
        && bytes.first().copied().is_some_and(edge)
        && bytes.last().copied().is_some_and(edge)
}

fn github_repo(url: &str) -> Option<GithubRepo> {
    let (owner, repo) = url.strip_prefix("https://github.com/")?.split_once('/')?;
    let owner_ok = (1..=GITHUB_OWNER_MAX_BYTES).contains(&owner.len())
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !owner.starts_with('-')
        && !owner.ends_with('-');
    let repo_ok = (1..=GITHUB_REPO_MAX_BYTES).contains(&repo.len())
        && repo
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && repo != "."
        && repo != "..";
    (owner_ok && repo_ok).then(|| GithubRepo {
        owner: owner.to_owned(),
        repo: repo.to_owned(),
    })
}

fn valid_icon(path: &str) -> bool {
    check_path(path).is_ok()
        && path.starts_with("assets/")
        && path.rsplit_once('.').is_some_and(|(_, extension)| {
            ICONS
                .iter()
                .any(|icon| icon.eq_ignore_ascii_case(extension))
        })
}

fn valid_key(key: &str) -> bool {
    key.strip_prefix("ed25519:").is_some_and(|encoded| {
        encoded.len() == PUBLIC_KEY_CHARS
            && encoded
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            && encoded.ends_with(|c| KEY_LAST_CHARS.contains(c))
    })
}

/// A path the package rules accept, under `folder` (whose rules allow only
/// scripts).
fn script_under(path: &str, folder: &str) -> bool {
    path.starts_with(folder) && check_path(path).is_ok()
}

fn read_permissions(table: &Table) -> Result<Vec<String>, ManifestError> {
    let permissions = texts(table, "permissions")?.unwrap_or_default();
    let mut seen = HashSet::new();
    for name in &permissions {
        if !permissions::is_known(name) {
            return Err(invalid(
                "permissions",
                format!("has an unknown name: {}", shown(name)),
            ));
        }
        if !seen.insert(name.as_str()) {
            return Err(invalid("permissions", format!("lists {name} twice")));
        }
    }
    Ok(permissions)
}

fn read_domains(table: &Table, permissions: &[String]) -> Result<Vec<String>, ManifestError> {
    let domains = texts(table, "domains")?.unwrap_or_default();
    if domains.is_empty() {
        return Ok(domains);
    }
    if !permissions.iter().any(|name| name == "internet") {
        return Err(invalid("domains", "needs the internet permission"));
    }
    if domains.len() > MAX_DOMAINS {
        return Err(invalid(
            "domains",
            format!("has more than {MAX_DOMAINS} names"),
        ));
    }
    for domain in &domains {
        let host = domain.strip_prefix("*.").unwrap_or(domain);
        if is_ip(host) {
            return Err(invalid(
                "domains",
                format!("can't be an IP address: {}", shown(domain)),
            ));
        }
        if !valid_host(host) {
            return Err(invalid(
                "domains",
                format!("has an invalid name: {}", shown(domain)),
            ));
        }
    }
    Ok(domains)
}

fn is_ip(host: &str) -> bool {
    let bracketed = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'));
    host.parse::<IpAddr>().is_ok() || bracketed.is_some_and(|ip| ip.parse::<Ipv6Addr>().is_ok())
}

/// A lowercase host name. Its last label starts with a letter, as every
/// top-level domain does, so no form of an address a resolver might read
/// (`127.1`, `2130706433`, `0x7f000001`) passes.
fn valid_host(host: &str) -> bool {
    if host.is_empty() || host.len() > HOST_MAX_BYTES {
        return false;
    }
    let label_ok = |label: &str| {
        (1..=LABEL_MAX_BYTES).contains(&label.len())
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    };
    host.split('.').all(label_ok)
        && host
            .rsplit('.')
            .next()
            .is_some_and(|last| last.starts_with(|c: char| c.is_ascii_lowercase()))
}

fn read_config(table: &Table) -> Result<Vec<ConfigField>, ManifestError> {
    let Some(items) = list(table, "config")? else {
        return Ok(Vec::new());
    };
    if items.len() > MAX_CONFIG {
        return Err(invalid(
            "config",
            format!("has more than {MAX_CONFIG} fields"),
        ));
    }
    let mut keys = HashSet::new();
    let mut fields = Vec::with_capacity(items.len());
    for (index, item) in items.into_iter().enumerate() {
        let at = format!("config[{}]", index + 1);
        let Value::Table(entry) = item else {
            return Err(invalid(at, "must be a table"));
        };
        let field = read_config_field(&entry, &at)?;
        if !keys.insert(field.key.clone()) {
            return Err(invalid(
                format!("{at}.key"),
                format!("{} is already used", field.key),
            ));
        }
        fields.push(field);
    }
    Ok(fields)
}

fn read_config_field(entry: &Table, at: &str) -> Result<ConfigField, ManifestError> {
    check_keys(entry, Some(at), CONFIG_FIELDS, "a config field")?;
    let name = |field: &str| format!("{at}.{field}");

    let key = required(text(entry, "key")?, &name("key"))?;
    if !valid_config_key(&key) {
        return Err(invalid(
            name("key"),
            "must be a lowercase letter, then up to 39 lowercase letters, digits or underscores",
        ));
    }
    let label = required(text(entry, "label")?, &name("label"))?;
    check_line(&name("label"), &label, NAME_CHARS)?;
    let kind = required(text(entry, "kind")?, &name("kind"))?;
    let Some(&(_, kind)) = KINDS.iter().find(|(known, _)| *known == kind) else {
        let names: Vec<&str> = KINDS.iter().map(|(known, _)| *known).collect();
        return Err(invalid(
            name("kind"),
            format!("must be one of {}", names.join(", ")),
        ));
    };
    let choices = texts(entry, "choices")?;
    let choices = match (kind, choices) {
        (ConfigKind::Choice, Some(choices)) if (1..=MAX_CHOICES).contains(&choices.len()) => {
            for (index, choice) in choices.iter().enumerate() {
                check_line(&format!("{at}.choices[{}]", index + 1), choice, NAME_CHARS)?;
            }
            choices
        }
        (ConfigKind::Choice, _) => {
            return Err(invalid(
                name("choices"),
                format!("must list 1 to {MAX_CHOICES} choices"),
            ));
        }
        (_, Some(_)) => return Err(invalid(name("choices"), "is only for kind choice")),
        (_, None) => Vec::new(),
    };
    let required = match entry.raw_get::<Value>("required")? {
        Value::Nil => false,
        Value::Boolean(required) => required,
        _ => return Err(invalid(name("required"), "must be true or false")),
    };
    Ok(ConfigField {
        key,
        label,
        kind,
        choices,
        required,
    })
}

/// `^[a-z][a-z0-9_]{0,39}$`
fn valid_config_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    (1..=CONFIG_KEY_MAX_BYTES).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn read_server(table: &Table) -> Result<Vec<String>, ManifestError> {
    let server = required(texts(table, "server")?, "server")?;
    if !(1..=MAX_SCRIPTS).contains(&server.len()) {
        return Err(invalid(
            "server",
            format!("must list 1 to {MAX_SCRIPTS} scripts"),
        ));
    }
    let mut seen = HashSet::new();
    for (index, path) in server.iter().enumerate() {
        if !script_under(path, "server/") {
            return Err(invalid(
                format!("server[{}]", index + 1),
                "must be a .lua or .luau file under server/",
            ));
        }
        if !seen.insert(path.as_str()) {
            return Err(invalid("server", format!("lists {path} twice")));
        }
    }
    Ok(server)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::package::Package;
    use crate::package::tests::zip_of;

    const KEY: &str = "ed25519:5BnpXs4hkHAd7kvB3c9tY2Rmf8uQ1aLwZeTj0oGxNyA";

    /// §7.2's example. Its `public_key` is a placeholder there, so the test
    /// puts a real one in its place.
    const EXAMPLE: &str = r#"
return {
  id = "tickets",                 -- [a-z0-9-], 3–40 chars, unique on the server
  name = "Ticket System",
  version = "1.2.0",              -- semver
  author = "Amin",
  description = "Support tickets in a channel of their own.",
  github = "https://github.com/amin/opencord-tickets",   -- optional
  license = "MIT",                -- SPDX id, "All rights reserved", or a URL
  api = 1,                        -- plugin API version it was written for
  icon = "assets/icon.png",
  public_key = "ed25519:…",       -- optional; set by `opencord-plugin pack`

  permissions = {
    "send_messages", "manage_channels", "create_channel_types",
    "register_commands", "internet",
  },
  domains = { "api.example.com" },  -- only with "internet"

  config = {                        -- settings admins fill in (§17.3)
    { key = "support_role", label = "Support role", kind = "role" },
    { key = "api_key", label = "API key", kind = "secret" },
  },

  server = { "server/main.luau" },  -- run in this order at start
  client = "client/main.luau",      -- entry point of every view
}
"#;

    fn example() -> String {
        assert_eq!(KEY.len(), "ed25519:".len() + 43);
        EXAMPLE.replace("ed25519:…", KEY)
    }

    /// A valid manifest with the fewest fields, changed by `changes` (Lua
    /// statements on `m`).
    fn manifest_with(changes: &str) -> String {
        format!(
            r#"
local m = {{
  id = "tickets", name = "Ticket System", version = "1.2.0", author = "Amin",
  license = "MIT", api = 1, server = {{ "server/main.luau" }},
}}
{changes}
return m
"#
        )
    }

    fn refused(source: &str) -> String {
        evaluate(source).unwrap_err().to_string()
    }

    #[test]
    fn evaluates_the_example_manifest() {
        let manifest = evaluate(&example()).unwrap();
        assert_eq!(
            manifest,
            Manifest {
                id: "tickets".into(),
                name: "Ticket System".into(),
                version: semver::Version::new(1, 2, 0),
                author: "Amin".into(),
                description: "Support tickets in a channel of their own.".into(),
                github: Some(GithubRepo {
                    owner: "amin".into(),
                    repo: "opencord-tickets".into(),
                }),
                license: "MIT".into(),
                api: 1,
                icon: Some("assets/icon.png".into()),
                public_key: Some(KEY.into()),
                permissions: [
                    "send_messages",
                    "manage_channels",
                    "create_channel_types",
                    "register_commands",
                    "internet",
                ]
                .map(String::from)
                .to_vec(),
                domains: vec!["api.example.com".into()],
                config: vec![
                    ConfigField {
                        key: "support_role".into(),
                        label: "Support role".into(),
                        kind: ConfigKind::Role,
                        choices: vec![],
                        required: false,
                    },
                    ConfigField {
                        key: "api_key".into(),
                        label: "API key".into(),
                        kind: ConfigKind::Secret,
                        choices: vec![],
                        required: false,
                    },
                ],
                server: vec!["server/main.luau".into()],
                client: Some("client/main.luau".into()),
            }
        );
    }

    #[test]
    fn optional_fields_have_their_defaults() {
        let manifest = evaluate(&manifest_with("")).unwrap();
        assert_eq!(manifest.description, "");
        assert_eq!(manifest.github, None);
        assert_eq!(manifest.icon, None);
        assert_eq!(manifest.public_key, None);
        assert!(manifest.permissions.is_empty());
        assert!(manifest.domains.is_empty());
        assert!(manifest.config.is_empty());
        assert_eq!(manifest.client, None);

        let manifest = evaluate(&manifest_with(
            r#"m.config = { { key = "mode", label = "Mode", kind = "choice",
                choices = { "fast", "slow" }, required = true } }
               m.name = string.rep("é", 60)
               m.description = "two\nlines""#,
        ))
        .unwrap();
        assert_eq!(manifest.config[0].kind, ConfigKind::Choice);
        assert_eq!(manifest.config[0].choices, ["fast", "slow"]);
        assert!(manifest.config[0].required);
        assert_eq!(manifest.name.chars().count(), 60);
        assert_eq!(manifest.description, "two\nlines");
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let manifest = evaluate(&example()).unwrap();
        let json = serde_json::to_string(&manifest).unwrap();
        assert_eq!(serde_json::from_str::<Manifest>(&json).unwrap(), manifest);
    }

    #[test]
    fn refuses_each_invalid_field() {
        let id = "manifest: id must be 3 to 40 lowercase letters, digits and dashes, \
                  starting and ending with a letter or digit";
        let github =
            "manifest: github must be https://github.com/<owner>/<repo>, with nothing after";
        let icon = "manifest: icon must be a .png, .jpg, .jpeg or .webp file under assets/";
        let key = "manifest: public_key must be ed25519: and 43 base64url characters";
        let version = "manifest: version must be a semantic version, like 1.2.0";
        let api = "manifest: api must be a whole number, 1 or more";
        let config_key = "manifest: config[1].key must be a lowercase letter, then up to 39 \
                          lowercase letters, digits or underscores";
        let kind = "manifest: config[1].kind must be one of text, secret, number, boolean, \
                    choice, role, channel, user";
        let choices = "manifest: config[1].choices must list 1 to 50 choices";
        let server = "manifest: server[1] must be a .lua or .luau file under server/";
        let field =
            |extra: &str| format!(r#"m.config = {{ {{ key = "a", label = "A", {extra} }} }}"#);
        let many = |n: usize, item: &str| {
            format!(
                "{{ {} }}",
                (1..=n)
                    .map(|i| item.replace('#', &i.to_string()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let cases: Vec<(String, String)> = vec![
            ("m.permisions = {}".into(), "manifest: permisions isn't a manifest field".into()),
            ("m[1] = 'x'".into(), "manifest: it has an entry without a name".into()),
            ("m.id = nil".into(), "manifest: id is missing".into()),
            ("m.id = 5".into(), "manifest: id must be text".into()),
            ("m.id = 'ab'".into(), id.into()),
            ("m.id = '-abc'".into(), id.into()),
            ("m.id = 'abc-'".into(), id.into()),
            ("m.id = 'Tickets'".into(), id.into()),
            ("m.id = 'a_b_c'".into(), id.into()),
            ("m.id = string.rep('a', 41)".into(), id.into()),
            ("m.name = ''".into(), "manifest: name must be 1 to 60 characters".into()),
            ("m.name = string.rep('é', 61)".into(), "manifest: name must be 1 to 60 characters".into()),
            ("m.name = '   '".into(), "manifest: name can't be blank".into()),
            ("m.name = 'a\\nb'".into(), "manifest: name can't contain control characters".into()),
            ("m.name = 'a\\xff'".into(), "manifest: name must be text in UTF-8".into()),
            ("m.author = nil".into(), "manifest: author is missing".into()),
            ("m.author = string.rep('a', 61)".into(), "manifest: author must be 1 to 60 characters".into()),
            ("m.description = string.rep('a', 301)".into(), "manifest: description must be at most 300 characters".into()),
            ("m.description = 'a\\0b'".into(), "manifest: description can't contain control characters".into()),
            ("m.license = ''".into(), "manifest: license must be 1 to 100 characters".into()),
            ("m.license = string.rep('a', 101)".into(), "manifest: license must be 1 to 100 characters".into()),
            ("m.version = '1.2'".into(), version.into()),
            ("m.version = 1.2".into(), "manifest: version must be text".into()),
            ("m.version = '1.2.0-' .. string.rep('a', 64)".into(), version.into()),
            ("m.api = nil".into(), "manifest: api is missing".into()),
            ("m.api = 0".into(), api.into()),
            ("m.api = 1.5".into(), api.into()),
            ("m.api = '1'".into(), api.into()),
            ("m.github = 'https://github.com/amin'".into(), github.into()),
            ("m.github = 'https://github.com/amin/tickets/'".into(), github.into()),
            ("m.github = 'https://github.com/amin/tickets?tab=x'".into(), github.into()),
            ("m.github = 'https://github.com/amin/tickets#readme'".into(), github.into()),
            ("m.github = 'http://github.com/amin/tickets'".into(), github.into()),
            ("m.github = 'https://github.com/-amin/tickets'".into(), github.into()),
            ("m.github = 'https://github.com/amin/..'".into(), github.into()),
            ("m.github = 'https://github.com/amin/a%2fb'".into(), github.into()),
            ("m.icon = 'icon.png'".into(), icon.into()),
            ("m.icon = 'assets/icon.gif'".into(), icon.into()),
            ("m.icon = 'assets/../icon.png'".into(), icon.into()),
            ("m.icon = 'server/icon.png'".into(), icon.into()),
            ("m.public_key = 'ed25519:abc'".into(), key.into()),
            (format!("m.public_key = 'ed25519:{}B'", "A".repeat(42)), key.into()),
            (format!("m.public_key = 'ed25519:{}A='", "A".repeat(42)), key.into()),
            (format!("m.public_key = 'rsa:{}A'", "A".repeat(42)), key.into()),
            ("m.permissions = { 'send_mesages' }".into(), "manifest: permissions has an unknown name: send_mesages".into()),
            ("m.permissions = { 'kick_members', 'kick_members' }".into(), "manifest: permissions lists kick_members twice".into()),
            ("m.permissions = 'send_messages'".into(), "manifest: permissions must be a list".into()),
            ("m.permissions = { [1] = 'kick_members', [3] = 'ban_members' }".into(), "manifest: permissions must be a list".into()),
            ("m.permissions = { x = 'kick_members' }".into(), "manifest: permissions must be a list".into()),
            ("m.permissions = { 1 }".into(), "manifest: permissions[1] must be text".into()),
            ("m.permissions = { 'internet' } m.domains = { 'Example.com' }".into(), "manifest: domains has an invalid name: Example.com".into()),
            ("m.permissions = { 'internet' } m.domains = { '-a.com' }".into(), "manifest: domains has an invalid name: -a.com".into()),
            ("m.permissions = { 'internet' } m.domains = { 'a..com' }".into(), "manifest: domains has an invalid name: a..com".into()),
            ("m.permissions = { 'internet' } m.domains = { 'a.com.' }".into(), "manifest: domains has an invalid name: a.com.".into()),
            ("m.permissions = { 'internet' } m.domains = { '*.*.a.com' }".into(), "manifest: domains has an invalid name: *.*.a.com".into()),
            ("m.permissions = { 'internet' } m.domains = { 'a*.com' }".into(), "manifest: domains has an invalid name: a*.com".into()),
            ("m.permissions = { 'internet' } m.domains = { 'https://a.com' }".into(), "manifest: domains has an invalid name: https://a.com".into()),
            ("m.permissions = { 'internet' } m.domains = { '' }".into(), "manifest: domains has an invalid name: ".into()),
            ("m.permissions = { 'internet' } m.domains = { 'a.123' }".into(), "manifest: domains has an invalid name: a.123".into()),
            (format!("m.permissions = {{ 'internet' }} m.domains = {{ '{}.com' }}", "a".repeat(64)), format!("manifest: domains has an invalid name: {}.com", "a".repeat(64))),
            (format!("m.permissions = {{ 'internet' }} m.domains = {{ '{}com' }}", "a.".repeat(126)), "manifest: domains has an invalid name: ".to_string() + &"a.".repeat(126) + "com"),
            (format!("m.permissions = {{ 'internet' }} m.domains = {}", many(21, "'d#.example.com'")), "manifest: domains has more than 20 names".into()),
            (format!("m.config = {}", many(31, "{ key = 'k#', label = 'K', kind = 'text' }")), "manifest: config has more than 30 fields".into()),
            ("m.config = { 'x' }".into(), "manifest: config[1] must be a table".into()),
            (r#"m.config = { { key = "Support", label = "A", kind = "role" } }"#.into(), config_key.into()),
            (r#"m.config = { { key = "1a", label = "A", kind = "role" } }"#.into(), config_key.into()),
            (format!(r#"m.config = {{ {{ key = "{}", label = "A", kind = "role" }} }}"#, "a".repeat(41)), config_key.into()),
            (r#"m.config = { { key = "a", label = "A", kind = "role" }, { key = "a", label = "B", kind = "text" } }"#.into(), "manifest: config[2].key a is already used".into()),
            (r#"m.config = { { key = "a", label = "", kind = "role" } }"#.into(), "manifest: config[1].label must be 1 to 60 characters".into()),
            (r#"m.config = { { key = "a", label = "A" } }"#.into(), "manifest: config[1].kind is missing".into()),
            (field("kind = 'colour'"), kind.into()),
            (field("kind = 'choice'"), choices.into()),
            (field("kind = 'choice', choices = {}"), choices.into()),
            (field(&format!("kind = 'choice', choices = {}", many(51, "'c#'"))), choices.into()),
            (field("kind = 'text', choices = { 'x' }"), "manifest: config[1].choices is only for kind choice".into()),
            (field("kind = 'choice', choices = { '' }"), "manifest: config[1].choices[1] must be 1 to 60 characters".into()),
            (field("kind = 'text', required = 'yes'"), "manifest: config[1].required must be true or false".into()),
            (field("kind = 'text', default = 'x'"), "manifest: config[1].default isn't a config field".into()),
            ("m.server = nil".into(), "manifest: server is missing".into()),
            ("m.server = {}".into(), "manifest: server must list 1 to 50 scripts".into()),
            (format!("m.server = {}", many(51, "'server/#.lua'")), "manifest: server must list 1 to 50 scripts".into()),
            ("m.server = { 'client/main.luau' }".into(), server.into()),
            ("m.server = { 'server/main.txt' }".into(), server.into()),
            ("m.server = { 'server/../main.lua' }".into(), server.into()),
            ("m.server = { 'server/a.lua', 'server/a.lua' }".into(), "manifest: server lists server/a.lua twice".into()),
            ("m.client = 'server/main.luau'".into(), "manifest: client must be a .lua or .luau file under client/".into()),
        ];
        for (changes, message) in cases {
            assert_eq!(refused(&manifest_with(&changes)), message, "{changes}");
        }
    }

    #[test]
    fn a_manifest_that_fails_says_why() {
        assert_eq!(refused("return 5"), "manifest: it didn't return a table");
        assert_eq!(refused("return"), "manifest: it didn't return a table");
        // `error` itself isn't there, so fail another way.
        let failed = refused("local t return t.x");
        assert!(
            failed.starts_with("manifest: manifest.lua:1: attempt to index nil"),
            "{failed}"
        );
        let syntax = refused("return {");
        assert!(
            syntax.starts_with("manifest: syntax error: manifest.lua:1:"),
            "{syntax}"
        );
        // Bytecode is refused as in every VM.
        let bytecode = mlua::Compiler::new().compile("return {}").unwrap();
        assert!(evaluate(&String::from_utf8_lossy(&bytecode)).is_err());
    }

    // Catches: a manifest VM without the time or memory cap.
    #[test]
    fn a_manifest_that_loops_is_stopped() {
        let began = Instant::now();
        assert_eq!(
            refused("while true do end"),
            "manifest: it ran longer than 100 ms"
        );
        assert!(began.elapsed() < Duration::from_secs(1));

        assert_eq!(
            refused("local t = {} while true do t[#t + 1] = string.rep('x', 1048576) .. #t end"),
            "manifest: the plugin ran out of memory"
        );
    }

    // Catches: running the manifest in an ordinary VM, whose globals hold
    // os, json, print and the rest.
    #[test]
    fn a_manifest_cannot_reach_anything() {
        assert!(evaluate("return { id = tostring(os) }").is_err());

        let probe = manifest_with(
            r#"m.description = table.concat({
                os and "os " or "", print and "print " or "", json and "json " or "",
                pcall and "pcall " or "", require and "require " or "",
                getfenv and "getfenv " or "", setmetatable and "setmetatable " or "",
                getmetatable and "getmetatable " or "", coroutine and "coroutine " or "",
                ipairs and "ipairs " or "", pairs and "pairs " or "", tostring and "tostring " or "",
                type and "type " or "", error and "error " or "", buffer and "buffer " or "",
                utf8 and "utf8 " or "", bit32 and "bit32 " or "", _G and "_G " or "",
                load and "load " or "", collectgarbage and "collectgarbage " or "",
                string.upper("x") == "X" and "" or "no string ",
                math.max(1, 2) == 2 and "" or "no math ",
            }, "")"#,
        );
        assert_eq!(evaluate(&probe).unwrap().description, "");
    }

    #[test]
    fn domains_need_internet() {
        assert_eq!(
            refused(&manifest_with("m.domains = { 'api.example.com' }")),
            "manifest: domains needs the internet permission"
        );
        let manifest = evaluate(&manifest_with(
            "m.permissions = { 'internet' } m.domains = { 'api.example.com', '*.example.org', 'xn--bcher-kva.example' }",
        ))
        .unwrap();
        assert_eq!(
            manifest.domains,
            ["api.example.com", "*.example.org", "xn--bcher-kva.example"]
        );
    }

    // Catches: a host name check that lets digits and dots through.
    #[test]
    fn refuses_ip_literal_domains() {
        for domain in ["127.0.0.1", "10.0.0.1", "[::1]", "::1", "*.127.0.0.1"] {
            assert_eq!(
                refused(&manifest_with(&format!(
                    "m.permissions = {{ 'internet' }} m.domains = {{ '{domain}' }}"
                ))),
                format!("manifest: domains can't be an IP address: {domain}")
            );
        }
        // Other ways resolvers read an address: refused as names.
        for domain in ["2130706433", "0x7f000001", "127.1"] {
            assert!(
                evaluate(&manifest_with(&format!(
                    "m.permissions = {{ 'internet' }} m.domains = {{ '{domain}' }}"
                )))
                .is_err(),
                "{domain}"
            );
        }
    }

    #[test]
    fn refuses_a_newer_api() {
        assert_eq!(
            refused(&manifest_with("m.api = 2")),
            "this plugin needs a newer Opencord (plugin API 2)"
        );
    }

    #[test]
    fn check_files_finds_missing_scripts() {
        let manifest = evaluate(&manifest_with(
            r#"m.server = { "server/main.luau", "server/extra.lua" }
               m.client = "client/main.luau"
               m.icon = "assets/icon.png""#,
        ))
        .unwrap();
        let package = |files: &[&str]| {
            let mut all: Vec<(&str, &[u8])> = vec![("manifest.lua", b"return {}")];
            all.extend(files.iter().map(|path| (*path, &b"x"[..])));
            Package::read_zip(&zip_of(&all)).unwrap()
        };
        let everything = [
            "server/main.luau",
            "server/extra.lua",
            "client/main.luau",
            "assets/icon.png",
        ];
        assert!(check_files(&manifest, &package(&everything)).is_ok());

        let cases = [
            (
                1,
                "manifest: server names server/extra.lua, which isn't in the plugin",
            ),
            (
                2,
                "manifest: client names client/main.luau, which isn't in the plugin",
            ),
            (
                3,
                "manifest: icon names assets/icon.png, which isn't in the plugin",
            ),
        ];
        for (missing, message) in cases {
            let mut files = everything.to_vec();
            files.remove(missing);
            let error = check_files(&manifest, &package(&files)).unwrap_err();
            assert_eq!(error.to_string(), message);
        }
    }
}
