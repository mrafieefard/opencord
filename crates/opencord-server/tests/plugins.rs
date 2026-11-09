// SPDX-License-Identifier: MPL-2.0

#[path = "plugins/bus.rs"]
mod bus;
#[path = "plugins/channels.rs"]
mod channels;
#[path = "plugins/commands.rs"]
mod commands;
#[path = "common/mod.rs"]
mod common;
#[path = "plugins/config.rs"]
mod config;
#[path = "plugins/definitions.rs"]
mod definitions;
#[path = "plugins/dev.rs"]
mod dev;
#[path = "plugins/event_tables.rs"]
mod event_tables;
#[path = "plugins/events.rs"]
mod events;
#[path = "plugins/example.rs"]
mod example;
#[path = "plugins/files.rs"]
mod files;
#[path = "plugins/guild.rs"]
mod guild;
#[path = "plugins/hooks.rs"]
mod hooks;
#[path = "plugins/install.rs"]
mod install;
#[path = "plugins/messages.rs"]
mod messages;
#[path = "plugins/roles.rs"]
mod roles;
#[path = "plugins/runtime.rs"]
mod runtime;
#[path = "plugins/support.rs"]
mod support;
#[path = "plugins/timers.rs"]
mod timers;
#[path = "plugins/voice.rs"]
mod voice;

use common::TestServer;
use opencord_server::config::Config;

#[tokio::test]
async fn plugins_section_loads_with_defaults() {
    let config = Config::default();
    assert!(config.plugins.enabled);
    assert_eq!(config.plugins.max_plugins, 50);
    assert_eq!(config.plugins.max_package_mb, 50);
    assert_eq!(config.plugins.memory_mb, 128);
    assert_eq!(config.plugins.handler_ms, 100);
    assert_eq!(config.plugins.database_mb, 1024);
    assert_eq!(config.plugins.files_mb, 5120);

    let _server = TestServer::start().await;
}
