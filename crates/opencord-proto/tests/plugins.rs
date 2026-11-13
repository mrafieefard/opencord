// SPDX-License-Identifier: MPL-2.0

use opencord_proto::v1::{
    Channel, ChannelKind, CloseView, ConfirmPluginReview, CreateChannel, CreatePluginDevToken,
    CreateUpload, CreatedPluginDevToken, DiscardPluginReview, ErrorCode, Event,
    FetchPluginDevTokens, FetchPluginLog, FetchPluginReviews, FetchPlugins, GithubCheck, Identify,
    OpenView, Plugin, PluginChannelState, PluginChannelType, PluginChannelTypesUpdate,
    PluginConfigField, PluginConfigKind, PluginConfigValue, PluginDevToken, PluginDevTokens,
    PluginFileRef, PluginLog, PluginLogLevel, PluginLogLine, PluginPermissionGrant, PluginRemove,
    PluginReview, PluginReviewRemove, PluginReviewUpdate, PluginReviews, PluginSignature,
    PluginState, PluginUpdate, PluginUser, PluginUsersUpdate, Plugins, Ready, ReportViewError,
    Request, Response, RestartPlugin, RevokePluginDevToken, Role, SendViewEvent, SetPluginConfig,
    SetPluginPermissions, SetPluginRunning, UninstallPlugin, ViewCloseReason, ViewClosed,
    ViewMessage, ViewOpened, ViewPlace, ViewScriptsUpdate, WatchPluginLog, event, request,
    response, view_place,
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

#[test]
fn plugin_channels_and_views_round_trip() {
    let chan_type = PluginChannelType {
        plugin_id: "tickets".into(),
        type_key: "inbox".into(),
        name: "Ticket Inbox".into(),
        description: "View open support tickets".into(),
        icon: "assets/inbox.png".into(),
        plugin_name: "Tickets".into(),
    };
    let file_ref = PluginFileRef {
        path: "client/main.luau".into(),
        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
        size_bytes: 4096,
    };

    // Ready with channel types
    let ready = Ready {
        plugin_channel_types: vec![chan_type.clone()],
        ..Default::default()
    };
    let ready_encoded = ready.encode_to_vec();
    let ready_decoded = Ready::decode(&ready_encoded[..]).unwrap();
    assert_eq!(ready, ready_decoded);
    assert_eq!(&ready_encoded[..2], &[0xCA, 0x01]); // field 25

    // Channel with plugin fields
    let channel = Channel {
        id: 100,
        kind: ChannelKind::Plugin as i32,
        name: "support".into(),
        plugin_id: Some("tickets".into()),
        type_key: Some("inbox".into()),
        plugin_state: PluginChannelState::Available as i32,
        ..Default::default()
    };
    let ch_encoded = channel.encode_to_vec();
    let ch_decoded = Channel::decode(&ch_encoded[..]).unwrap();
    assert_eq!(channel, ch_decoded);

    // CreateChannel with plugin fields
    let create_channel = CreateChannel {
        kind: ChannelKind::Plugin as i32,
        name: "support".into(),
        plugin_id: Some("tickets".into()),
        type_key: Some("inbox".into()),
        ..Default::default()
    };
    let cc_encoded = create_channel.encode_to_vec();
    let cc_decoded = CreateChannel::decode(&cc_encoded[..]).unwrap();
    assert_eq!(create_channel, cc_decoded);

    // CreateUpload with view_id
    let create_upload = CreateUpload {
        channel_id: 0,
        filename: "test.png".into(),
        size_bytes: 1024,
        view_id: Some(42),
        ..Default::default()
    };
    let cu_encoded = create_upload.encode_to_vec();
    let cu_decoded = CreateUpload::decode(&cu_encoded[..]).unwrap();
    assert_eq!(create_upload, cu_decoded);

    // Requests
    let requests = vec![
        request::Kind::OpenView(OpenView {
            place: Some(ViewPlace {
                kind: Some(view_place::Kind::ChannelId(100)),
            }),
            platform: "desktop".into(),
        }),
        request::Kind::CloseView(CloseView { view_id: 42 }),
        request::Kind::SendViewEvent(SendViewEvent {
            view_id: 42,
            name: "submit".into(),
            data_json: "{\"text\":\"help\"}".into(),
        }),
        request::Kind::ReportViewError(ReportViewError {
            view_id: 42,
            text: "main.luau:10: runtime error".into(),
        }),
    ];

    for kind in requests {
        let req = Request { kind: Some(kind) };
        let encoded = req.encode_to_vec();
        let decoded = Request::decode(&encoded[..]).unwrap();
        assert_eq!(req, decoded);
    }

    // Response
    let view_opened = ViewOpened {
        view_id: 42,
        plugin_id: "tickets".into(),
        api: 1,
        entry: "client/main.luau".into(),
        files: vec![file_ref.clone()],
        show_errors: true,
    };
    let resp = Response {
        result: Some(response::Result::ViewOpened(view_opened)),
    };
    let resp_encoded = resp.encode_to_vec();
    let resp_decoded = Response::decode(&resp_encoded[..]).unwrap();
    assert_eq!(resp, resp_decoded);

    // Events
    let events = vec![
        event::Kind::ViewMessage(ViewMessage {
            view_id: 42,
            name: "update".into(),
            data_json: "{\"count\":1}".into(),
        }),
        event::Kind::ViewClosed(ViewClosed {
            view_id: 42,
            reason: ViewCloseReason::PluginStopped as i32,
        }),
        event::Kind::ViewScriptsUpdate(ViewScriptsUpdate {
            plugin_id: "tickets".into(),
            entry: "client/main.luau".into(),
            files: vec![file_ref],
        }),
        event::Kind::PluginChannelTypesUpdate(PluginChannelTypesUpdate {
            types: vec![chan_type],
        }),
    ];

    for kind in events {
        let ev = Event { kind: Some(kind) };
        let encoded = ev.encode_to_vec();
        let decoded = Event::decode(&encoded[..]).unwrap();
        assert_eq!(ev, decoded);
    }
}

// fails if a field number or name below changes
#[test]
fn activities_round_trip() {
    use opencord_proto::v1::{
        ActivityEndReason, ActivitySession, ActivitySessionCreate, ActivitySessionDelete,
        ActivitySessionUpdate, EndActivity, PluginActivitiesUpdate, PluginActivity, StartActivity,
        ViewCloseReason, ViewPlace, event, request, response, view_place,
    };

    let activity = PluginActivity {
        plugin_id: "paint_plugin".into(),
        activity_key: "whiteboard".into(),
        name: "Whiteboard".into(),
        description: "Shared drawing board".into(),
        icon: "assets/icon.png".into(),
        plugin_name: "Paint".into(),
    };
    let activity_encoded = activity.encode_to_vec();
    let activity_decoded = PluginActivity::decode(&activity_encoded[..]).unwrap();
    assert_eq!(activity, activity_decoded);

    let session = ActivitySession {
        id: 1001,
        channel_id: 2002,
        plugin_id: "paint_plugin".into(),
        activity_key: "whiteboard".into(),
        name: "Whiteboard".into(),
        icon: "assets/icon.png".into(),
        plugin_name: "Paint".into(),
        starter_id: 3003,
        started_at_ms: 1700000000000,
        viewer_ids: vec![3003, 4004],
    };
    let session_encoded = session.encode_to_vec();
    let session_decoded = ActivitySession::decode(&session_encoded[..]).unwrap();
    assert_eq!(session, session_decoded);

    let start = StartActivity {
        channel_id: 2002,
        plugin_id: "paint_plugin".into(),
        activity_key: "whiteboard".into(),
    };
    let start_encoded = start.encode_to_vec();
    let start_decoded = StartActivity::decode(&start_encoded[..]).unwrap();
    assert_eq!(start, start_decoded);

    let end = EndActivity { session_id: 1001 };
    let end_encoded = end.encode_to_vec();
    let end_decoded = EndActivity::decode(&end_encoded[..]).unwrap();
    assert_eq!(end, end_decoded);

    let activities_update = PluginActivitiesUpdate {
        activities: vec![activity.clone()],
    };
    let activities_update_encoded = activities_update.encode_to_vec();
    let activities_update_decoded =
        PluginActivitiesUpdate::decode(&activities_update_encoded[..]).unwrap();
    assert_eq!(activities_update, activities_update_decoded);

    let session_create = ActivitySessionCreate {
        session: Some(session.clone()),
    };
    let session_create_encoded = session_create.encode_to_vec();
    let session_create_decoded =
        ActivitySessionCreate::decode(&session_create_encoded[..]).unwrap();
    assert_eq!(session_create, session_create_decoded);

    let session_update = ActivitySessionUpdate {
        session: Some(session.clone()),
    };
    let session_update_encoded = session_update.encode_to_vec();
    let session_update_decoded =
        ActivitySessionUpdate::decode(&session_update_encoded[..]).unwrap();
    assert_eq!(session_update, session_update_decoded);

    let session_delete = ActivitySessionDelete {
        session_id: 1001,
        channel_id: 2002,
        reason: ActivityEndReason::CallEmpty as i32,
    };
    let session_delete_encoded = session_delete.encode_to_vec();
    let session_delete_decoded =
        ActivitySessionDelete::decode(&session_delete_encoded[..]).unwrap();
    assert_eq!(session_delete, session_delete_decoded);

    let view_place_act = ViewPlace {
        kind: Some(view_place::Kind::ActivitySessionId(1001)),
    };
    let view_place_act_encoded = view_place_act.encode_to_vec();
    let view_place_act_decoded = ViewPlace::decode(&view_place_act_encoded[..]).unwrap();
    assert_eq!(view_place_act, view_place_act_decoded);

    assert_eq!(ViewCloseReason::ActivityEnded as i32, 5);
    assert_eq!(ViewCloseReason::LeftCall as i32, 6);

    assert_eq!(ActivityEndReason::Unspecified as i32, 0);
    assert_eq!(ActivityEndReason::Ended as i32, 1);
    assert_eq!(ActivityEndReason::Plugin as i32, 2);
    assert_eq!(ActivityEndReason::CallEmpty as i32, 3);
    assert_eq!(ActivityEndReason::PluginStopped as i32, 4);
    assert_eq!(ActivityEndReason::Unavailable as i32, 5);

    // Request wire enum numbers
    let req_start = Request {
        kind: Some(request::Kind::StartActivity(start)),
    };
    let req_start_encoded = req_start.encode_to_vec();
    let req_start_decoded = Request::decode(&req_start_encoded[..]).unwrap();
    assert_eq!(req_start, req_start_decoded);

    let req_end = Request {
        kind: Some(request::Kind::EndActivity(end)),
    };
    let req_end_encoded = req_end.encode_to_vec();
    let req_end_decoded = Request::decode(&req_end_encoded[..]).unwrap();
    assert_eq!(req_end, req_end_decoded);

    // Response wire enum numbers
    let resp_act = Response {
        result: Some(response::Result::ActivitySession(session.clone())),
    };
    let resp_act_encoded = resp_act.encode_to_vec();
    let resp_act_decoded = Response::decode(&resp_act_encoded[..]).unwrap();
    assert_eq!(resp_act, resp_act_decoded);

    // Event wire enum numbers
    let ev_update = Event {
        kind: Some(event::Kind::PluginActivitiesUpdate(activities_update)),
    };
    assert_eq!(
        ev_update,
        Event::decode(&ev_update.encode_to_vec()[..]).unwrap()
    );

    let ev_create = Event {
        kind: Some(event::Kind::ActivitySessionCreate(session_create)),
    };
    assert_eq!(
        ev_create,
        Event::decode(&ev_create.encode_to_vec()[..]).unwrap()
    );

    let ev_session_upd = Event {
        kind: Some(event::Kind::ActivitySessionUpdate(session_update)),
    };
    assert_eq!(
        ev_session_upd,
        Event::decode(&ev_session_upd.encode_to_vec()[..]).unwrap()
    );

    let ev_delete = Event {
        kind: Some(event::Kind::ActivitySessionDelete(session_delete)),
    };
    assert_eq!(
        ev_delete,
        Event::decode(&ev_delete.encode_to_vec()[..]).unwrap()
    );

    // Ready wire enum fields
    let ready = Ready {
        plugin_activities: vec![activity],
        activity_sessions: vec![session],
        ..Default::default()
    };
    assert_eq!(ready, Ready::decode(&ready.encode_to_vec()[..]).unwrap());
}
