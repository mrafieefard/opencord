// SPDX-License-Identifier: MPL-2.0

//! `activity`: ending the plugin's own activity sessions (spec §15.4, plan
//! M3 Task 4).
//!
//! Ending is `activity.finish(session)`: `end` is a Luau keyword, so
//! `activity.end` couldn't be written.

use std::sync::Arc;

use mlua::{Lua, Value};
use opencord_proto::v1 as proto;

use super::{async_function, id_arg, raise};
use crate::plugins::activities;
use crate::plugins::permissions::PluginPermission;
use crate::plugins::runtime::PluginEnv;

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let activity = lua.create_table()?;
    let finish_env = env.clone();
    activity.raw_set(
        "finish",
        async_function(lua, move |_, session: Value| {
            let env = finish_env.clone();
            async move {
                env.require(PluginPermission::RegisterActivities)?;
                let session_id = session_arg(session)?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let ended = env
                    .on_server(
                        async move { activities::end_own(&state, &plugin_id, session_id).await },
                    )
                    .await?;
                if ended {
                    env.log.push(
                        proto::PluginLogLevel::Action,
                        &format!("ended activity session {session_id}"),
                    );
                }
                Ok(ended)
            }
        })?,
    )?;
    activity.set_readonly(true);
    lua.globals().raw_set("activity", activity)
}

/// A session table's `id`, or the id string itself.
fn session_arg(session: Value) -> mlua::Result<i64> {
    match session {
        Value::Table(table) => id_arg(table.raw_get("id")?, "session.id"),
        Value::String(_) => id_arg(session, "session"),
        _ => Err(raise(
            "INVALID_ARGUMENT: session must be a session or its id string",
        )),
    }
}
