//! A plugin's managed role and plugin user, seen from members' apps
//! (design 2026-11-07 §8.2).

use opencord_proto::v1 as proto;
use proto::request::Kind;

use crate::common::{TestClient, TestServer, invite, join, owner, self_id};

/// The plugin user and managed role of a plugin the owner installed.
async fn install_plugin(_server: &TestServer, _owner: &mut TestClient) -> proto::PluginUser {
    todo!("Task 8: install a plugin with support::install and return its user")
}

fn role_of(ready: &proto::Ready, plugin_id: &str) -> proto::Role {
    ready
        .roles
        .iter()
        .find(|role| role.plugin_id.as_deref() == Some(plugin_id))
        .cloned()
        .unwrap_or_else(|| panic!("no role for {plugin_id}"))
}

#[tokio::test]
#[ignore = "needs Task 8"]
async fn a_plugin_role_cannot_be_given_to_members() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let plugin = install_plugin(&server, &mut owner).await;
    let code = invite(&mut owner, None).await;
    let (_alice, alice_ready) = join(&server, 2, &code).await;
    let role = role_of(&alice_ready, &plugin.plugin_id);

    let error = owner
        .error(Kind::AddMemberRole(proto::AddMemberRole {
            user_id: self_id(&alice_ready),
            role_id: role.id,
        }))
        .await;

    assert_eq!(error.code, proto::ErrorCode::Forbidden as i32);
    assert_eq!(error.message, "this role belongs to a plugin");
}

#[tokio::test]
#[ignore = "needs Task 8"]
async fn a_plugin_role_cannot_be_deleted() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let plugin = install_plugin(&server, &mut owner).await;
    let (_again, ready) = crate::common::owner_again(&server).await;
    let role = role_of(&ready, &plugin.plugin_id);

    let error = owner
        .error(Kind::DeleteRole(proto::DeleteRole { role_id: role.id }))
        .await;

    assert_eq!(error.code, proto::ErrorCode::Forbidden as i32);
    assert_eq!(error.message, "this role belongs to a plugin");
}

#[tokio::test]
#[ignore = "needs Task 8"]
async fn a_plugin_roles_permissions_cannot_be_edited() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let plugin = install_plugin(&server, &mut owner).await;
    let (_again, ready) = crate::common::owner_again(&server).await;
    let role = role_of(&ready, &plugin.plugin_id);

    let error = owner
        .error(Kind::UpdateRole(proto::UpdateRole {
            role_id: role.id,
            permissions: Some(role.permissions | (1 << 7)),
            ..Default::default()
        }))
        .await;
    let renamed = owner
        .ok(Kind::UpdateRole(proto::UpdateRole {
            role_id: role.id,
            name: Some("Bots".to_owned()),
            ..Default::default()
        }))
        .await;

    assert_eq!(error.code, proto::ErrorCode::Forbidden as i32);
    assert_eq!(
        error.message,
        "a plugin's role follows the plugin's permissions"
    );
    let proto::response::Result::Role(renamed) = renamed else {
        panic!("{renamed:?}");
    };
    assert_eq!(renamed.name, "Bots");
    assert_eq!(renamed.permissions, role.permissions);
}

#[tokio::test]
#[ignore = "needs Task 8"]
async fn ready_lists_plugin_users() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let plugin = install_plugin(&server, &mut owner).await;
    let code = invite(&mut owner, None).await;

    let (_alice, ready) = join(&server, 2, &code).await;

    assert_eq!(ready.plugin_users, vec![plugin.clone()]);
    assert!(plugin.installed);
    assert_eq!(
        role_of(&ready, &plugin.plugin_id).name,
        format!("Plugin: {}", plugin.name)
    );
    assert!(
        ready
            .members
            .iter()
            .all(|member| member.user.as_ref().unwrap().id != plugin.user_id)
    );
}
