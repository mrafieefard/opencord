//! Chat commands (Phase 3 plan §17): a small registry of built-in
//! commands, a feature of the server, not a bot. Results are messages by
//! whoever ran the command, or replies only they see. Music is the first
//! feature with commands.
//!
//! The server parses and checks everything; clients only build the input
//! from the specs.

use std::collections::BTreeMap;
use std::sync::RwLock;

use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use crate::db;
use crate::state::{AppState, Audience};

pub mod music;
pub mod options;

use proto::CommandOptionKind as Kind;

/// Where the settings are kept, in `server_meta`.
const SETTINGS_KEY: &str = "commands.settings";

/// A command's option, as specs describe it.
#[derive(Debug, Clone, Copy)]
pub struct OptionSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub kind: Kind,
    pub required: bool,
    pub autocomplete: bool,
    /// For choices: (shown, value).
    pub choices: &'static [(&'static str, &'static str)],
    /// For integers; (0, 0) for no bounds.
    pub range: (i64, i64),
}

impl OptionSpec {
    const fn new(name: &'static str, description: &'static str, kind: Kind) -> Self {
        Self {
            name,
            description,
            kind,
            required: false,
            autocomplete: false,
            choices: &[],
            range: (0, 0),
        }
    }

    const fn required(mut self) -> Self {
        self.required = true;
        self
    }

    const fn autocomplete(mut self) -> Self {
        self.autocomplete = true;
        self
    }

    const fn choices(mut self, choices: &'static [(&'static str, &'static str)]) -> Self {
        self.choices = choices;
        self
    }

    const fn range(mut self, min: i64, max: i64) -> Self {
        self.range = (min, max);
        self
    }

    fn to_proto(self) -> proto::CommandOptionSpec {
        proto::CommandOptionSpec {
            name: self.name.to_owned(),
            description: self.description.to_owned(),
            kind: self.kind as i32,
            required: self.required,
            autocomplete: self.autocomplete,
            choices: self
                .choices
                .iter()
                .map(|(name, value)| proto::CommandChoice {
                    name: (*name).to_owned(),
                    value: (*value).to_owned(),
                    artwork_id: String::new(),
                })
                .collect(),
            min: self.range.0,
            max: self.range.1,
        }
    }
}

/// A command.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub options: &'static [OptionSpec],
    pub feature: &'static str,
    /// The least it needs, for clients to dim it; the server checks more.
    pub permissions: Permissions,
    pub permission_summary: &'static str,
}

impl Spec {
    pub fn option(&self, name: &str) -> Option<&OptionSpec> {
        self.options.iter().find(|option| option.name == name)
    }
}

/// Every command there is (§17.2). `/volume` and `/music` are the app's
/// own and never reach the server.
pub const SPECS: &[Spec] = music::SPECS;

/// The command called `name`, or one of its aliases.
pub fn find(name: &str) -> Option<&'static Spec> {
    let name = name.trim_start_matches('/').to_ascii_lowercase();
    SPECS
        .iter()
        .find(|spec| spec.name == name || spec.aliases.contains(&name.as_str()))
}

/// What a server's admins chose on the Commands page (§17.4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandSettings {
    /// Commands turned off.
    pub disabled: Vec<String>,
    /// Where a command may be used; absent for every channel.
    pub channels: BTreeMap<String, Vec<i64>>,
    /// Results only to who ran the command (§17.3); public by default.
    pub results_private: bool,
}

impl CommandSettings {
    pub fn enabled(&self, name: &str) -> bool {
        !self.disabled.iter().any(|off| off == name)
    }

    /// Whether `name` may be used in `channel_id`.
    pub fn allowed_in(&self, name: &str, channel_id: i64) -> bool {
        self.channels
            .get(name)
            .is_none_or(|channels| channels.is_empty() || channels.contains(&channel_id))
    }
}

/// The settings, kept in memory and in `server_meta`.
#[derive(Debug, Default)]
pub struct Commands {
    settings: RwLock<CommandSettings>,
}

impl Commands {
    pub fn settings(&self) -> CommandSettings {
        self.settings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub async fn load(&self, conn: &mut SqliteConnection) -> anyhow::Result<()> {
        let saved = db::meta::load_prefixed(conn, SETTINGS_KEY).await?;
        let settings = saved
            .get(SETTINGS_KEY)
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default();
        *self
            .settings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = settings;
        Ok(())
    }

    pub async fn save(
        &self,
        conn: &mut SqliteConnection,
        settings: CommandSettings,
    ) -> Result<(), sqlx::Error> {
        let json = serde_json::to_string(&settings).unwrap_or_default();
        db::meta::put(conn, SETTINGS_KEY, Some(&json)).await?;
        *self
            .settings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = settings;
        Ok(())
    }
}

fn spec_proto(spec: &Spec, settings: &CommandSettings) -> proto::CommandSpec {
    proto::CommandSpec {
        name: spec.name.to_owned(),
        description: spec.description.to_owned(),
        options: spec
            .options
            .iter()
            .map(|option| option.to_proto())
            .collect(),
        feature: spec.feature.to_owned(),
        permission_summary: spec.permission_summary.to_owned(),
        aliases: spec
            .aliases
            .iter()
            .map(|alias| (*alias).to_owned())
            .collect(),
        permissions: spec.permissions.bits(),
        enabled: settings.enabled(spec.name),
        channel_ids: settings
            .channels
            .get(spec.name)
            .cloned()
            .unwrap_or_default(),
    }
}

/// Whether a feature is set up here.
fn feature_ready(state: &AppState, feature: &str) -> bool {
    match feature {
        "music" => crate::music::configured(state),
        _ => true,
    }
}

/// What members can use (§17.1): enabled commands of features set up.
pub fn available(state: &AppState) -> proto::Commands {
    let settings = state.commands.settings();
    proto::Commands {
        commands: SPECS
            .iter()
            .filter(|spec| settings.enabled(spec.name) && feature_ready(state, spec.feature))
            .map(|spec| spec_proto(spec, &settings))
            .collect(),
        results_public: !settings.results_private,
    }
}

/// Every command, on or off, for the Commands page.
pub fn all(state: &AppState) -> proto::Commands {
    let settings = state.commands.settings();
    proto::Commands {
        commands: SPECS
            .iter()
            .map(|spec| spec_proto(spec, &settings))
            .collect(),
        results_public: !settings.results_private,
    }
}

/// Tells everyone the commands they can use now.
pub fn announce(state: &AppState) {
    state.broadcast(
        proto::event::Kind::CommandsUpdate(proto::CommandsUpdate {
            commands: Some(available(state)),
        }),
        Audience::Everyone,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_found_by_name_or_alias() {
        assert_eq!(find("play").map(|spec| spec.name), Some("play"));
        assert_eq!(find("/NP").map(|spec| spec.name), Some("nowplaying"));
        assert!(find("volume").is_none(), "the app's own");
        assert!(find("nope").is_none());
        let mut names: Vec<&str> = SPECS.iter().map(|spec| spec.name).collect();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }

    #[test]
    fn settings_say_what_is_on_and_where() {
        let mut settings = CommandSettings::default();
        assert!(settings.enabled("play") && settings.allowed_in("play", 1));
        settings.disabled.push("play".to_owned());
        settings.channels.insert("skip".to_owned(), vec![7]);
        assert!(!settings.enabled("play"));
        assert!(settings.allowed_in("skip", 7) && !settings.allowed_in("skip", 8));
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<CommandSettings>(&json).unwrap(),
            settings
        );
    }
}
