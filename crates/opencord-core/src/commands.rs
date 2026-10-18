//! Chat commands (Phase 3 plan §17): running one, its suggestions, and the
//! Commands page. `/volume` and `/music` are the app's own and never come
//! here.

use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;

use crate::api::types::{
    CommandChoice, CommandList, CommandOptionKind, CommandOptionSpec, CommandOptionValue,
    CommandOutcome, CommandSettingsChanges, CommandSpec, CoreError,
};
use crate::client::{Client, unexpected};
use crate::convert::bits_to_api;

impl Client {
    /// Runs a command typed in `channel_id`. An "only you" reply arrives as
    /// an `EphemeralMessage` with `nonce`.
    pub async fn command_execute(
        &self,
        key: &str,
        channel_id: i64,
        name: String,
        options: Vec<CommandOptionValue>,
        nonce: String,
    ) -> Result<CommandOutcome, CoreError> {
        let request = Request::ExecuteCommand(proto::ExecuteCommand {
            channel_id,
            name,
            options: options
                .into_iter()
                .map(|option| proto::CommandOptionValue {
                    name: option.name,
                    value: option.value,
                })
                .collect(),
            nonce,
        });
        match self.request(key, request).await? {
            Response::CommandResult(result) => Ok(CommandOutcome {
                message_id: result.message_id,
                ephemeral: result.ephemeral,
            }),
            other => Err(unexpected(&other)),
        }
    }

    /// Up to 10 suggestions for an option as typed so far.
    pub async fn command_autocomplete(
        &self,
        key: &str,
        channel_id: i64,
        name: String,
        option: String,
        value: String,
    ) -> Result<Vec<CommandChoice>, CoreError> {
        let request = Request::AutocompleteCommand(proto::AutocompleteCommand {
            channel_id,
            name,
            option,
            value,
        });
        match self.request(key, request).await? {
            Response::CommandChoices(found) => Ok(found.choices.into_iter().map(choice).collect()),
            other => Err(unexpected(&other)),
        }
    }

    /// Every command, on or off. Needs Manage server.
    pub async fn command_settings(&self, key: &str) -> Result<CommandList, CoreError> {
        let request = Request::FetchCommandSettings(proto::FetchCommandSettings {});
        list(self.request(key, request).await?)
    }

    /// Needs Manage server.
    pub async fn update_command_settings(
        &self,
        key: &str,
        changes: CommandSettingsChanges,
    ) -> Result<CommandList, CoreError> {
        let request = Request::UpdateCommandSettings(proto::UpdateCommandSettings {
            name: changes.name,
            enabled: changes.enabled,
            channel_ids: changes
                .channel_ids
                .map(|ids| proto::CommandChannels { ids }),
            results_public: changes.results_public,
        });
        list(self.request(key, request).await?)
    }
}

fn list(response: Response) -> Result<CommandList, CoreError> {
    match response {
        Response::Commands(commands) => Ok(list_from(commands)),
        other => Err(unexpected(&other)),
    }
}

fn choice(choice: proto::CommandChoice) -> CommandChoice {
    CommandChoice {
        name: choice.name,
        value: choice.value,
        artwork_id: choice.artwork_id,
    }
}

pub(crate) fn list_from(commands: proto::Commands) -> CommandList {
    CommandList {
        commands: commands
            .commands
            .into_iter()
            .map(|spec| CommandSpec {
                name: spec.name,
                description: spec.description,
                options: spec
                    .options
                    .into_iter()
                    .map(|option| CommandOptionSpec {
                        name: option.name,
                        description: option.description,
                        kind: match proto::CommandOptionKind::try_from(option.kind)
                            .unwrap_or_default()
                        {
                            proto::CommandOptionKind::Integer => CommandOptionKind::Integer,
                            proto::CommandOptionKind::Boolean => CommandOptionKind::Boolean,
                            proto::CommandOptionKind::Choice => CommandOptionKind::Choice,
                            proto::CommandOptionKind::User => CommandOptionKind::User,
                            proto::CommandOptionKind::Channel => CommandOptionKind::Channel,
                            proto::CommandOptionKind::Text
                            | proto::CommandOptionKind::Unspecified => CommandOptionKind::Text,
                        },
                        required: option.required,
                        autocomplete: option.autocomplete,
                        choices: option.choices.into_iter().map(choice).collect(),
                        min: option.min,
                        max: option.max,
                    })
                    .collect(),
                feature: spec.feature,
                permission_summary: spec.permission_summary,
                aliases: spec.aliases,
                permissions: bits_to_api(spec.permissions),
                enabled: spec.enabled,
                channel_ids: spec.channel_ids,
            })
            .collect(),
        results_public: commands.results_public,
    }
}
