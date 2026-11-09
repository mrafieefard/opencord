// SPDX-License-Identifier: MPL-2.0

use opencord_proto::v1::{
    ConfirmPluginReview, CreatePluginDevToken, CreatedPluginDevToken, DiscardPluginReview,
    ErrorCode, Event, FetchPluginDevTokens, FetchPluginLog, FetchPluginReviews, FetchPlugins,
    GithubCheck, Identify, Plugin, PluginConfigField, PluginConfigKind, PluginConfigValue,
    PluginDevToken, PluginDevTokens, PluginLog, PluginLogLevel, PluginLogLine,
    PluginPermissionGrant, PluginRemove, PluginReview, PluginReviewRemove, PluginReviewUpdate,
    PluginReviews, PluginSignature, PluginState, PluginUpdate, PluginUser, PluginUsersUpdate,
    Plugins, Ready, Request, Response, RestartPlugin, RevokePluginDevToken, Role, SetPluginConfig,
    SetPluginPermissions, SetPluginRunning, UninstallPlugin, WatchPluginLog, event, request,
    response,
};
use prost::Message as _;

#[test]
fn field_numbers_are_pinned() {
    let ready = Ready {
        plugin_users: vec![PluginUser {
            user_id: 1,
            plugin_id: "p".into(),
            name: "Bot".into(),
            avatar_hash: String::new(),
            installed: true,
        }],
        ..Default::default()
    };
    let ready_bytes = ready.encode_to_vec();
    assert_eq!(&ready_bytes[..2], &[0xC2, 0x01]); // field 24, wire type 2

    let identify = Identify {
        features: vec!["plugin_users".into()],
        ..Default::default()
    };
    assert_eq!(
        identify.encode_to_vec(),
        [
            0x52, 0x0C, b'p', b'l', b'u', b'g', b'i', b'n', b'_', b'u', b's', b'e', b'r', b's'
        ]
    );

    let role = Role {
        plugin_id: Some("x".into()),
        ..Default::default()
    };
    assert_eq!(role.encode_to_vec(), [0x4A, 0x01, b'x']);

    assert_eq!(ErrorCode::PluginPackageInvalid as i32, 54);
    assert_eq!(ErrorCode::PluginSignatureInvalid as i32, 55);
    assert_eq!(ErrorCode::PluginNotSameAuthor as i32, 56);
    assert_eq!(ErrorCode::PluginLimit as i32, 57);
    assert_eq!(ErrorCode::PluginUnavailable as i32, 58);
    assert_eq!(ErrorCode::PermissionDenied as i32, 59);

    let req = Request {
        kind: Some(request::Kind::FetchPlugins(FetchPlugins {})),
    };
    assert_eq!(&req.encode_to_vec()[..2], &[0x92, 0x0D]); // 210

    let resp = Response {
        result: Some(response::Result::Plugins(Plugins::default())),
    };
    assert_eq!(&resp.encode_to_vec()[..2], &[0xDA, 0x02]); // 43

    let ev = Event {
        kind: Some(event::Kind::PluginUpdate(PluginUpdate::default())),
    };
    assert_eq!(&ev.encode_to_vec()[..2], &[0x92, 0x08]); // 130
}

#[test]
fn plugin_messages_round_trip() {
    let plugin = Plugin {
        id: "tickets".into(),
        name: "Tickets".into(),
        version: "1.0.0".into(),
        author: "Amin".into(),
        description: "Support ticket system".into(),
        github: "https://github.com/amin/tickets".into(),
        license: "MIT".into(),
        signature: PluginSignature::Signed as i32,
        key_fingerprint: "0123456789abcdef".into(),
        github_check: GithubCheck::Verified as i32,
        state: PluginState::Running as i32,
        status_reason: "".into(),
        permissions: vec![PluginPermissionGrant {
            name: "send_messages".into(),
            granted: true,
            grantable: true,
        }],
        domains: vec!["api.example.com".into()],
        config_fields: vec![PluginConfigField {
            key: "api_key".into(),
            label: "API Key".into(),
            kind: PluginConfigKind::Secret as i32,
            choices: vec![],
            required: true,
        }],
        config: vec![PluginConfigValue {
            key: "api_key".into(),
            value: "".into(),
            has_value: true,
        }],
        user_id: 1001,
        role_id: 2001,
        installed_at_ms: 1000,
        updated_at_ms: 2000,
        cpu_permille: 5,
        memory_bytes: 1024 * 1024,
        database_bytes: 4096,
        files_bytes: 8192,
    };

    let encoded_plugin = plugin.encode_to_vec();
    let decoded_plugin = Plugin::decode(&encoded_plugin[..]).unwrap();
    assert_eq!(plugin, decoded_plugin);

    let review = PluginReview {
        review_id: "rev-1".into(),
        plugin: Some(plugin.clone()),
        installed: Some(plugin.clone()),
        new_permissions: vec!["internet".into()],
        license_needs_acceptance: false,
        expires_at_ms: 5000,
        dev: false,
    };
    let encoded_review = review.encode_to_vec();
    let decoded_review = PluginReview::decode(&encoded_review[..]).unwrap();
    assert_eq!(review, decoded_review);

    let plugins_list = Plugins {
        plugins: vec![plugin.clone()],
    };
    assert_eq!(
        plugins_list,
        Plugins::decode(&plugins_list.encode_to_vec()[..]).unwrap()
    );

    let reviews_list = PluginReviews {
        reviews: vec![review.clone()],
    };
    assert_eq!(
        reviews_list,
        PluginReviews::decode(&reviews_list.encode_to_vec()[..]).unwrap()
    );

    let dev_token = PluginDevToken {
        id: 1,
        plugin_id: "tickets".into(),
        created_by: 100,
        created_at_ms: 1000,
        expires_at_ms: 2000,
        last_used_at_ms: Some(1500),
    };
    let dev_tokens = PluginDevTokens {
        tokens: vec![dev_token.clone()],
    };
    assert_eq!(
        dev_tokens,
        PluginDevTokens::decode(&dev_tokens.encode_to_vec()[..]).unwrap()
    );

    let created_token = CreatedPluginDevToken {
        token: Some(dev_token.clone()),
        secret: "sec_123".into(),
        server_fingerprint: "fp_456".into(),
    };
    assert_eq!(
        created_token,
        CreatedPluginDevToken::decode(&created_token.encode_to_vec()[..]).unwrap()
    );

    let log = PluginLog {
        lines: vec![PluginLogLine {
            seq: 1,
            at_ms: 1000,
            level: PluginLogLevel::Info as i32,
            text: "started".into(),
        }],
    };
    assert_eq!(log, PluginLog::decode(&log.encode_to_vec()[..]).unwrap());

    // Test all requests inside Request
    let requests = vec![
        request::Kind::FetchPlugins(FetchPlugins {}),
        request::Kind::FetchPluginReviews(FetchPluginReviews {}),
        request::Kind::ConfirmPluginReview(ConfirmPluginReview {
            review_id: "rev-1".into(),
            granted: vec!["send_messages".into()],
            config: vec![],
            accept_license: true,
        }),
        request::Kind::DiscardPluginReview(DiscardPluginReview {
            review_id: "rev-1".into(),
        }),
        request::Kind::SetPluginPermissions(SetPluginPermissions {
            plugin_id: "tickets".into(),
            granted: vec!["send_messages".into()],
        }),
        request::Kind::SetPluginConfig(SetPluginConfig {
            plugin_id: "tickets".into(),
            config: vec![],
        }),
        request::Kind::SetPluginRunning(SetPluginRunning {
            plugin_id: "tickets".into(),
            running: true,
        }),
        request::Kind::RestartPlugin(RestartPlugin {
            plugin_id: "tickets".into(),
        }),
        request::Kind::UninstallPlugin(UninstallPlugin {
            plugin_id: "tickets".into(),
            delete_data: true,
        }),
        request::Kind::FetchPluginLog(FetchPluginLog {
            plugin_id: "tickets".into(),
            before: Some(100),
            limit: 50,
        }),
        request::Kind::WatchPluginLog(WatchPluginLog {
            plugin_id: "tickets".into(),
            watch: true,
        }),
        request::Kind::CreatePluginDevToken(CreatePluginDevToken {
            plugin_id: "tickets".into(),
            days: 7,
        }),
        request::Kind::RevokePluginDevToken(RevokePluginDevToken { token_id: 42 }),
        request::Kind::FetchPluginDevTokens(FetchPluginDevTokens {}),
    ];

    for kind in requests {
        let req = Request { kind: Some(kind) };
        let encoded = req.encode_to_vec();
        let decoded = Request::decode(&encoded[..]).unwrap();
        assert_eq!(req, decoded);
    }

    // Test all events inside Event
    let events = vec![
        event::Kind::PluginUpdate(PluginUpdate {
            plugin: Some(plugin.clone()),
        }),
        event::Kind::PluginRemove(PluginRemove {
            plugin_id: "tickets".into(),
        }),
        event::Kind::PluginReviewUpdate(Box::new(PluginReviewUpdate {
            review: Some(review.clone()),
        })),
        event::Kind::PluginReviewRemove(PluginReviewRemove {
            review_id: "rev-1".into(),
        }),
        event::Kind::PluginLogAppend(opencord_proto::v1::PluginLogAppend {
            plugin_id: "tickets".into(),
            lines: vec![PluginLogLine {
                seq: 1,
                at_ms: 1000,
                level: PluginLogLevel::Info as i32,
                text: "hello".into(),
            }],
        }),
        event::Kind::PluginUsersUpdate(PluginUsersUpdate {
            users: vec![PluginUser {
                user_id: 1001,
                plugin_id: "tickets".into(),
                name: "Tickets".into(),
                avatar_hash: "hash".into(),
                installed: true,
            }],
        }),
    ];

    for kind in events {
        let ev = Event { kind: Some(kind) };
        let encoded = ev.encode_to_vec();
        let decoded = Event::decode(&encoded[..]).unwrap();
        assert_eq!(ev, decoded);
    }
}
