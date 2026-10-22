// SPDX-License-Identifier: MPL-2.0

mod common;

use common::{TestServer, owner};

#[tokio::test]
async fn ready_says_whether_private_messages_are_on() {
    let default_server = TestServer::start().await;
    let (_client, ready) = owner(&default_server).await;
    assert!(ready.private_messages);

    let disabled_server =
        TestServer::start_with(|config| config.private_messages.enabled = false).await;
    let (_client, ready) = owner(&disabled_server).await;
    assert!(!ready.private_messages);
}
