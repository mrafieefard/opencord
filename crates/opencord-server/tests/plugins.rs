// SPDX-License-Identifier: MPL-2.0

#[path = "common/mod.rs"]
mod common;
#[path = "plugins/events.rs"]
mod events;
#[path = "plugins/install.rs"]
mod install;
#[path = "plugins/roles.rs"]
mod roles;
#[path = "plugins/runtime.rs"]
mod runtime;
#[path = "plugins/support.rs"]
mod support;

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
