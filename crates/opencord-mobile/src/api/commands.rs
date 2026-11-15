//! Slash commands: running one and asking for an option's suggestions.

use opencord_core::api::types as core;

use crate::MobileError;
use crate::convert;
use crate::ids::{id, parse_id};
use crate::mobile::on_runtime;
use crate::types::{CommandChoiceRecord, CommandOptionValueRecord, CommandOutcomeRecord};

/// Runs a command typed in `channel_id`. An "only you" reply arrives as an
/// `EphemeralReceived` with `nonce`. `source` is empty for Opencord's
/// commands and `plugin:<id>` for a plugin's.
#[uniffi::export]
pub async fn command_execute(
    server_key: String,
    channel_id: String,
    name: String,
    source: String,
    options: Vec<CommandOptionValueRecord>,
    nonce: String,
) -> Result<CommandOutcomeRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    let options = options
        .into_iter()
        .map(|option| core::CommandOptionValue {
            name: option.name,
            value: option.value,
        })
        .collect();
    on_runtime(move |client| async move {
        let outcome = client
            .command_execute(&server_key, channel_id, name, source, options, nonce)
            .await?;
        Ok(CommandOutcomeRecord {
            message_id: outcome.message_id.map(id),
            ephemeral: outcome.ephemeral,
        })
    })
    .await
}

/// Suggestions for a command's option as typed so far.
#[uniffi::export]
pub async fn command_autocomplete(
    server_key: String,
    channel_id: String,
    name: String,
    source: String,
    option: String,
    value: String,
) -> Result<Vec<CommandChoiceRecord>, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        let found = client
            .command_autocomplete(&server_key, channel_id, name, source, option, value)
            .await?;
        Ok(found.into_iter().map(convert::command_choice).collect())
    })
    .await
}
