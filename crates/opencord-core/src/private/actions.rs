//! Client actions for friends, message requests, blocks and privacy (Task 14).

use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;

use sqlx::{Row, SqliteConnection};

use crate::api::types::{
    CoreError, PrivacySettingsData, PrivateContact, PrivateEvent, SharedServer,
};
use crate::private::PrivateError;
use crate::private::contacts::{self, Contact, Conversation, FriendState};
use crate::private::outbox::{self, MOVED, SendOptions};
use crate::private::service::{Open, PrivateService, now_ms, parse_hex};

/// The contact as it is now. The receive worker may have changed it while a
/// request was out, so a change made after the server's answer starts here.
async fn fresh(
    open: &Open,
    conn: &mut SqliteConnection,
    before: Contact,
) -> Result<Contact, CoreError> {
    Ok(
        contacts::load(conn, open.store().sealer(), before.public_key)
            .await?
            .unwrap_or(before),
    )
}

/// Sends a friend request to `user_id` on server `server_key`.
pub async fn friend_request_send(
    service: &PrivateService,
    server_key: &str,
    user_id: i64,
) -> Result<(), CoreError> {
    let open = service.opened()?;
    let peer = service
        .members()
        .public_key_of(server_key, user_id)
        .ok_or_else(|| CoreError::InvalidInput {
            message: format!("unknown user {user_id} on {server_key}"),
        })?;
    let identity = service.identity_for(&open)?;
    let slip = identity.slip_for(&peer);
    let response = service
        .request(
            server_key,
            Request::SendFriendRequest(proto::SendFriendRequest {
                user_id,
                slip: slip.to_vec(),
            }),
        )
        .await?;
    let created = match response {
        Response::FriendRequestCreated(created) => created,
        other => {
            return Err(CoreError::Storage {
                message: format!("unexpected response to SendFriendRequest: {other:?}"),
            });
        }
    };
    let server_id = service
        .members()
        .server_id(server_key)
        .or_else(|| service.transport().server_id_of(server_key))
        .unwrap_or_default();

    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let mut contact = contacts::load(&mut conn, open.store().sealer(), peer)
        .await?
        .unwrap_or_else(|| Contact::new(peer));
    if let Some(name) = service.members().name_of(&peer)
        && contact.name.is_none()
    {
        contact.name = Some(name);
    }
    contacts::after_my_request(&mut contact, &server_id, created.request_id);
    contacts::save(&mut conn, open.store().sealer(), &contact, now_ms()).await?;
    let view = service.contact_view(&open, &mut conn, &contact).await?;
    drop(conn);
    service.emit(PrivateEvent::ContactChanged(view));
    Ok(())
}

/// Answers an incoming friend request from `peer` (hex).
pub async fn friend_request_answer(
    service: &PrivateService,
    peer: &str,
    accept: bool,
) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let open = service.opened()?;
    let (contact, request) = {
        let mut conn = open
            .store()
            .pool()
            .acquire()
            .await
            .map_err(PrivateError::from)?;
        let contact = contacts::load(&mut conn, open.store().sealer(), peer_bytes)
            .await?
            .ok_or_else(|| CoreError::InvalidInput {
                message: "no friend request waiting from that person".to_owned(),
            })?;
        if contact.friend_state != FriendState::Incoming {
            return Err(CoreError::InvalidInput {
                message: "no friend request waiting from that person".to_owned(),
            });
        }
        let request = contact
            .request
            .clone()
            .ok_or_else(|| CoreError::InvalidInput {
                message: "no friend request waiting from that person".to_owned(),
            })?;
        (contact, request)
    };

    let server_key = service
        .transport()
        .server_key_for(&request.server_id)
        .ok_or(CoreError::UnknownServer)?;
    if !service.members().is_connected(&server_key) {
        let server_name = service
            .transport()
            .server_name(&server_key)
            .unwrap_or_else(|| "The server".to_owned());
        return Err(CoreError::InvalidInput {
            message: format!("Couldn't answer: {server_name} isn't connected right now."),
        });
    }

    let slip = if accept {
        let identity = service.identity_for(&open)?;
        identity.slip_for(&peer_bytes).to_vec()
    } else {
        Vec::new()
    };

    service
        .request(
            &server_key,
            Request::AnswerFriendRequest(proto::AnswerFriendRequest {
                request_id: request.request_id,
                accept,
                slip,
            }),
        )
        .await?;

    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let mut contact = fresh(&open, &mut conn, contact).await?;
    contacts::after_my_answer(&mut contact, accept, &request.server_id);
    contacts::save(&mut conn, open.store().sealer(), &contact, now_ms()).await?;
    let view = service.contact_view(&open, &mut conn, &contact).await?;
    drop(conn);
    service.emit(PrivateEvent::ContactChanged(view));
    Ok(())
}

/// Cancels an outgoing friend request to `peer` (hex).
pub async fn friend_request_cancel(service: &PrivateService, peer: &str) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let open = service.opened()?;
    let (contact, request) = {
        let mut conn = open
            .store()
            .pool()
            .acquire()
            .await
            .map_err(PrivateError::from)?;
        let contact = contacts::load(&mut conn, open.store().sealer(), peer_bytes)
            .await?
            .ok_or_else(|| CoreError::InvalidInput {
                message: "no outgoing friend request waiting for that person".to_owned(),
            })?;
        if contact.friend_state != FriendState::Outgoing {
            return Err(CoreError::InvalidInput {
                message: "no outgoing friend request waiting for that person".to_owned(),
            });
        }
        let request = contact
            .request
            .clone()
            .ok_or_else(|| CoreError::InvalidInput {
                message: "no outgoing friend request waiting for that person".to_owned(),
            })?;
        (contact, request)
    };

    let server_key = service
        .transport()
        .server_key_for(&request.server_id)
        .ok_or(CoreError::UnknownServer)?;
    if !service.members().is_connected(&server_key) {
        let server_name = service
            .transport()
            .server_name(&server_key)
            .unwrap_or_else(|| "The server".to_owned());
        return Err(CoreError::InvalidInput {
            message: format!("Couldn't cancel: {server_name} isn't connected right now."),
        });
    }

    service
        .request(
            &server_key,
            Request::CancelFriendRequest(proto::CancelFriendRequest {
                request_id: request.request_id,
            }),
        )
        .await?;

    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let mut contact = fresh(&open, &mut conn, contact).await?;
    contacts::after_my_cancel(&mut contact);
    contacts::save(&mut conn, open.store().sealer(), &contact, now_ms()).await?;
    let view = service.contact_view(&open, &mut conn, &contact).await?;
    drop(conn);
    service.emit(PrivateEvent::ContactChanged(view));
    Ok(())
}

/// Removes a friend through every connected shared server with private messages.
pub async fn friend_remove(service: &PrivateService, peer: &str) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let open = service.opened()?;
    let shared = service.members().shared_servers(&peer_bytes);
    let targets: Vec<_> = shared
        .into_iter()
        .filter(|s| s.connected && s.private_messages)
        .collect();
    if targets.is_empty() {
        return Err(CoreError::InvalidInput {
            message: "You don't share a connected server with them right now.".to_owned(),
        });
    }

    let mut first_error = None;
    let mut any_ok = false;
    for target in targets {
        let req = Request::RemoveFriend(proto::RemoveFriend {
            user_id: target.peer_user_id,
        });
        match service.request(&target.server_key, req).await {
            Ok(_) => any_ok = true,
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }

    if any_ok {
        let mut conn = open
            .store()
            .pool()
            .acquire()
            .await
            .map_err(PrivateError::from)?;
        let contact = contacts::load(&mut conn, open.store().sealer(), peer_bytes).await?;
        if let Some(mut contact) = contact {
            contact.friend_state = FriendState::None;
            contact.friend_server_id = None;
            contacts::save(&mut conn, open.store().sealer(), &contact, now_ms()).await?;
            let view = service.contact_view(&open, &mut conn, &contact).await?;
            drop(conn);
            service.emit(PrivateEvent::ContactChanged(view));
        }
        Ok(())
    } else {
        Err(first_error.unwrap_or(CoreError::NotConnected))
    }
}

/// Accepts a message request from `peer` (hex).
pub async fn message_request_accept(service: &PrivateService, peer: &str) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let open = service.opened()?;
    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let contact = contacts::load(&mut conn, open.store().sealer(), peer_bytes).await?;
    let mut contact = match contact {
        Some(c) if c.conversation == Conversation::Request => c,
        _ => {
            return Err(CoreError::InvalidInput {
                message: "No message request waiting from that person.".to_owned(),
            });
        }
    };
    drop(conn);

    let relay = if let Some(ref relay_id) = contact.relay_server_id {
        service.transport().server_key_for(relay_id)
    } else {
        service
            .members()
            .shared_servers(&peer_bytes)
            .into_iter()
            .find(|s| s.connected && s.private_messages)
            .map(|s| s.server_key)
    };
    if contact.relay_server_id.is_none()
        && let Some(ref r_key) = relay
    {
        contact.relay_server_id = service
            .members()
            .server_id(r_key)
            .or_else(|| service.transport().server_id_of(r_key));
    }

    outbox::send(
        service,
        peer_bytes,
        proto::private_payload::Body::RequestAccepted(proto::RequestAccepted {}),
        SendOptions { relay },
    )
    .await?;

    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let relay_server_id = contact.relay_server_id.clone();
    let mut contact = fresh(&open, &mut conn, contact).await?;
    if contact.relay_server_id.is_none() {
        contact.relay_server_id = relay_server_id;
    }
    contact.conversation = Conversation::Accepted;
    contacts::save(&mut conn, open.store().sealer(), &contact, now_ms()).await?;
    let view = service.contact_view(&open, &mut conn, &contact).await?;
    drop(conn);
    service.emit(PrivateEvent::ContactChanged(view));
    Ok(())
}

/// Ignores a message request from `peer` (hex), deleting its messages.
pub async fn message_request_ignore(service: &PrivateService, peer: &str) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let open = service.opened()?;

    let mut tx = open
        .store()
        .pool()
        .begin()
        .await
        .map_err(PrivateError::from)?;
    sqlx::query("DELETE FROM messages WHERE peer = $1")
        .bind(&peer_bytes[..])
        .execute(&mut *tx)
        .await
        .map_err(PrivateError::from)?;

    let contact = contacts::load(&mut tx, open.store().sealer(), peer_bytes).await?;
    let outcome = if let Some(mut contact) = contact {
        if contact.friend_state == FriendState::None
            && contact.request.is_none()
            && !contact.blocked
        {
            contacts::delete(&mut tx, peer_bytes).await?;
            Some(None)
        } else {
            contact.conversation = Conversation::None;
            contacts::save(&mut tx, open.store().sealer(), &contact, now_ms()).await?;
            let view = service.contact_view(&open, &mut tx, &contact).await?;
            Some(Some(view))
        }
    } else {
        None
    };

    tx.commit().await.map_err(PrivateError::from)?;

    match outcome {
        Some(None) => service.emit(PrivateEvent::ContactRemoved {
            peer: hex::encode(peer_bytes),
        }),
        Some(Some(view)) => service.emit(PrivateEvent::ContactChanged(view)),
        None => {}
    }
    Ok(())
}

/// Blocks `peer` (hex): declines incoming requests, cancels outgoing, removes friendship,
/// applies after_block, and syncs UpdatePrivacy to servers where peer is a member.
pub async fn private_block(service: &PrivateService, peer: &str) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let open = service.opened()?;

    let contact = {
        let mut conn = open
            .store()
            .pool()
            .acquire()
            .await
            .map_err(PrivateError::from)?;
        contacts::load(&mut conn, open.store().sealer(), peer_bytes).await?
    };

    if let Some(contact) = &contact {
        if contact.friend_state == FriendState::Incoming
            && contact.request.is_some()
            && let Err(e) = friend_request_answer(service, peer, false).await
        {
            tracing::warn!(%e, "could not decline friend request while blocking");
        } else if contact.friend_state == FriendState::Outgoing
            && contact.request.is_some()
            && let Err(e) = friend_request_cancel(service, peer).await
        {
            tracing::warn!(%e, "could not cancel friend request while blocking");
        }
        if contact.friend_state == FriendState::Friends
            && let Err(e) = friend_remove(service, peer).await
        {
            tracing::warn!(%e, "could not remove friend while blocking");
        }
    }

    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let mut contact = contacts::load(&mut conn, open.store().sealer(), peer_bytes)
        .await?
        .unwrap_or_else(|| {
            let mut c = Contact::new(peer_bytes);
            if let Some(name) = service.members().name_of(&peer_bytes) {
                c.name = Some(name);
            }
            c
        });
    contacts::after_block(&mut contact);
    contacts::save(&mut conn, open.store().sealer(), &contact, now_ms()).await?;
    let view = service.contact_view(&open, &mut conn, &contact).await?;
    drop(conn);
    service.emit(PrivateEvent::ContactChanged(view));

    let shared = service.members().shared_servers(&peer_bytes);
    for server in shared {
        if server.connected
            && server.private_messages
            && let Err(e) = sync_privacy_to_server(service, &open, &server.server_key).await
        {
            tracing::warn!(%e, server_key = %server.server_key, "could not update privacy on block");
        }
    }
    Ok(())
}

/// Unblocks `peer` (hex) and syncs UpdatePrivacy.
pub async fn private_unblock(service: &PrivateService, peer: &str) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let open = service.opened()?;

    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let contact = contacts::load(&mut conn, open.store().sealer(), peer_bytes).await?;
    if let Some(mut contact) = contact
        && contact.blocked
    {
        contact.blocked = false;
        contacts::save(&mut conn, open.store().sealer(), &contact, now_ms()).await?;
        let view = service.contact_view(&open, &mut conn, &contact).await?;
        drop(conn);
        service.emit(PrivateEvent::ContactChanged(view));
    }

    let shared = service.members().shared_servers(&peer_bytes);
    for server in shared {
        if server.connected
            && server.private_messages
            && let Err(e) = sync_privacy_to_server(service, &open, &server.server_key).await
        {
            tracing::warn!(%e, server_key = %server.server_key, "could not update privacy on unblock");
        }
    }
    Ok(())
}

/// Reads the privacy settings.
pub async fn privacy_settings(service: &PrivateService) -> Result<PrivacySettingsData, CoreError> {
    let open = service.opened()?;
    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let privacy = contacts::privacy(&mut conn).await?;
    Ok(PrivacySettingsData {
        allow_friend_requests: privacy.allow_friend_requests,
        allow_message_requests: privacy.allow_message_requests,
    })
}

/// Saves the privacy settings and syncs UpdatePrivacy to every connected server with private messages.
pub async fn set_privacy_settings(
    service: &PrivateService,
    settings: PrivacySettingsData,
) -> Result<(), CoreError> {
    let open = service.opened()?;
    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    contacts::set_privacy(
        &mut conn,
        &contacts::PrivacySettings {
            allow_friend_requests: settings.allow_friend_requests,
            allow_message_requests: settings.allow_message_requests,
        },
    )
    .await?;
    drop(conn);

    let servers = service.members().private_servers();
    for key in servers {
        if let Err(e) = sync_privacy_to_server(service, &open, &key).await {
            tracing::warn!(%e, server_key = %key, "could not update privacy on settings change");
        }
    }
    Ok(())
}

/// Sends UpdatePrivacy to `server_key`.
pub async fn sync_privacy_to_server(
    service: &PrivateService,
    open: &Open,
    server_key: &str,
) -> Result<(), CoreError> {
    let (privacy, blocked) = {
        let mut conn = open
            .store()
            .pool()
            .acquire()
            .await
            .map_err(PrivateError::from)?;
        let privacy = contacts::privacy(&mut conn).await?;
        let blocked: Vec<[u8; 32]> = contacts::load_all(&mut conn, open.store().sealer())
            .await?
            .into_iter()
            .filter(|contact| contact.blocked)
            .map(|contact| contact.public_key)
            .collect();
        (privacy, blocked)
    };
    let blocked_user_ids = blocked
        .iter()
        .filter_map(|peer| service.members().user_id_on(server_key, peer))
        .collect();
    service
        .request(
            server_key,
            Request::UpdatePrivacy(proto::UpdatePrivacy {
                allow_friend_requests: privacy.allow_friend_requests,
                allow_message_requests: privacy.allow_message_requests,
                blocked_user_ids,
            }),
        )
        .await?;
    Ok(())
}

/// Opens a conversation from profile or Friends page.
pub async fn private_open_conversation(
    service: &PrivateService,
    server_key: &str,
    user_id: i64,
) -> Result<PrivateContact, CoreError> {
    let open = service.opened()?;
    let peer = service
        .members()
        .public_key_of(server_key, user_id)
        .ok_or_else(|| CoreError::InvalidInput {
            message: format!("unknown user {user_id} on {server_key}"),
        })?;

    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let mut contact = contacts::load(&mut conn, open.store().sealer(), peer)
        .await?
        .unwrap_or_else(|| {
            let mut c = Contact::new(peer);
            if let Some(name) = service.members().name_of(&peer) {
                c.name = Some(name);
            }
            c
        });

    let shared = service.members().shared_servers(&peer);
    let is_usable = |server_id: &str| -> bool {
        service
            .transport()
            .server_key_for(server_id)
            .and_then(|key| shared.iter().find(|s| s.server_key == key))
            .is_some_and(|s| s.connected && s.private_messages)
    };

    let current_usable = contact.relay_server_id.as_deref().is_some_and(is_usable);

    if !current_usable {
        let friend_usable = contact
            .friend_server_id
            .as_deref()
            .filter(|id| is_usable(id));

        if let Some(friend_id) = friend_usable {
            contact.relay_server_id = Some(friend_id.to_string());
        } else {
            contact.relay_server_id = service
                .members()
                .server_id(server_key)
                .or_else(|| service.transport().server_id_of(server_key));
        }
    }

    contact.hidden = false;
    contacts::save(&mut conn, open.store().sealer(), &contact, now_ms()).await?;
    let view = service.contact_view(&open, &mut conn, &contact).await?;
    drop(conn);
    service.emit(PrivateEvent::ContactChanged(view.clone()));
    Ok(view)
}

/// Returns the shared servers with `peer` in the order they were added.
pub fn private_shared_servers(
    service: &PrivateService,
    saved_order: &[String],
    peer: &str,
) -> Vec<SharedServer> {
    let Ok(peer_bytes) = parse_hex::<32>(peer, "invalid peer") else {
        return Vec::new();
    };
    let mut shared = service.members().shared_servers(&peer_bytes);
    let mut ordered = Vec::with_capacity(shared.len());
    for key in saved_order {
        if let Some(pos) = shared.iter().position(|s| &s.server_key == key) {
            ordered.push(shared.remove(pos));
        }
    }
    ordered.extend(shared);
    ordered
}

/// Moves a conversation's relay to `server_key`.
pub async fn private_move(
    service: &PrivateService,
    peer: &str,
    server_key: &str,
) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let open = service.opened()?;

    let (contact, relay) = {
        let mut conn = open
            .store()
            .pool()
            .acquire()
            .await
            .map_err(PrivateError::from)?;
        let contact = contacts::load(&mut conn, open.store().sealer(), peer_bytes).await?;
        let relay =
            outbox::current_relay(service, contact.as_ref(), &peer_bytes, Some(server_key))?;
        (contact, relay)
    };

    let server_id_bytes = hex::decode(&relay.server_id).map_err(|_| CoreError::Storage {
        message: "invalid server id hex".to_owned(),
    })?;
    let server_name = service
        .transport()
        .server_name(server_key)
        .unwrap_or_else(|| "The server".to_owned());

    let outcome = outbox::send_timed(
        service,
        peer_bytes,
        proto::private_payload::Body::RelayMoved(proto::RelayMoved {
            server_id: server_id_bytes,
            server_name: server_name.clone(),
        }),
        SendOptions {
            relay: Some(server_key.to_string()),
        },
    )
    .await?;

    let sent_at = outcome.sent_at_ms.unwrap_or_else(now_ms);
    let divider_id = outbox::new_message_id()?;
    outbox::insert_divider(
        &open,
        divider_id,
        peer_bytes,
        &server_name,
        &relay.server_id,
        sent_at,
    )
    .await?;

    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let mut contact = contacts::load(&mut conn, open.store().sealer(), peer_bytes)
        .await?
        .or(contact)
        .unwrap_or_else(|| Contact::new(peer_bytes));
    contact.relay_server_id = Some(relay.server_id.clone());
    contacts::save(&mut conn, open.store().sealer(), &contact, now_ms()).await?;

    let divider_view = outbox::view(service, &open, divider_id).await?;
    let contact_view = service.contact_view(&open, &mut conn, &contact).await?;
    drop(conn);

    service.emit(PrivateEvent::MessageAdded(divider_view));
    service.emit(PrivateEvent::ContactChanged(contact_view));
    Ok(())
}

/// Moves this device's relay to match a Moved card.
pub async fn private_move_too(
    service: &PrivateService,
    peer: &str,
    message_id: &str,
) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let message_id_bytes = parse_hex::<16>(message_id, "that message isn't valid")?;
    let open = service.opened()?;

    let (server_id, server_name) = {
        let row = sqlx::query(
            "SELECT kind, outgoing, body, server_id FROM messages WHERE id = $1 AND peer = $2",
        )
        .bind(message_id_bytes.as_slice())
        .bind(peer_bytes.as_slice())
        .fetch_optional(open.store().pool())
        .await
        .map_err(PrivateError::from)?
        .ok_or_else(|| CoreError::InvalidInput {
            message: "message not found".to_owned(),
        })?;

        let kind: i64 = row.try_get("kind").map_err(PrivateError::from)?;
        let outgoing: i64 = row.try_get("outgoing").map_err(PrivateError::from)?;
        if kind != MOVED || outgoing != 0 {
            return Err(CoreError::InvalidInput {
                message: "not a moved card".to_owned(),
            });
        }

        let server_id: String = row
            .try_get::<Option<String>, _>("server_id")
            .map_err(PrivateError::from)?
            .ok_or_else(|| CoreError::InvalidInput {
                message: "moved card missing server id".to_owned(),
            })?;

        let body_bytes: Option<Vec<u8>> = row.try_get("body").map_err(PrivateError::from)?;
        let server_name = match body_bytes {
            Some(body) => {
                let decrypted = open.store().sealer().open(
                    crate::private::service::MESSAGE_BODY_LABEL,
                    &message_id_bytes,
                    &body,
                )?;
                String::from_utf8(decrypted).unwrap_or_else(|_| "The server".to_owned())
            }
            None => "The server".to_owned(),
        };
        (server_id, server_name)
    };

    let server_key = service.transport().server_key_for(&server_id);
    let shared = service.members().shared_servers(&peer_bytes);
    let is_valid = server_key
        .as_ref()
        .and_then(|key| shared.iter().find(|s| &s.server_key == key))
        .is_some_and(|s| s.connected && s.private_messages);

    if !is_valid {
        return Err(CoreError::InvalidInput {
            message: format!("This device isn't on {server_name}."),
        });
    }

    let divider_id = outbox::new_message_id()?;
    let now = now_ms();
    outbox::insert_divider(&open, divider_id, peer_bytes, &server_name, &server_id, now).await?;

    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let mut contact = contacts::load(&mut conn, open.store().sealer(), peer_bytes)
        .await?
        .unwrap_or_else(|| Contact::new(peer_bytes));
    contact.relay_server_id = Some(server_id);
    contacts::save(&mut conn, open.store().sealer(), &contact, now).await?;

    let divider_view = outbox::view(service, &open, divider_id).await?;
    let contact_view = service.contact_view(&open, &mut conn, &contact).await?;
    drop(conn);

    service.emit(PrivateEvent::MessageAdded(divider_view));
    service.emit(PrivateEvent::ContactChanged(contact_view));
    Ok(())
}

/// Sends typing event for `peer`.
pub async fn private_typing(service: &PrivateService, peer: &str) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let open = service.opened()?;
    let (contact, relay) = {
        let mut conn = open
            .store()
            .pool()
            .acquire()
            .await
            .map_err(PrivateError::from)?;
        let contact = contacts::load(&mut conn, open.store().sealer(), peer_bytes).await?;
        let relay = outbox::current_relay(service, contact.as_ref(), &peer_bytes, None)?;
        (contact, relay)
    };
    let slip = contact
        .and_then(|c| c.their_slip)
        .map(|s| s.to_vec())
        .unwrap_or_default();
    service
        .request(
            &relay.key,
            Request::StartPrivateTyping(proto::StartPrivateTyping {
                recipient_id: relay.recipient_id,
                slip,
            }),
        )
        .await?;
    Ok(())
}

/// Marks messages with `peer` read up to the newest message.
pub async fn private_mark_read(service: &PrivateService, peer: &str) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let open = service.opened()?;
    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let newest: Option<i64> = sqlx::query_scalar(
        "SELECT sent_at FROM messages WHERE peer = $1 ORDER BY sent_at DESC LIMIT 1",
    )
    .bind(peer_bytes.as_slice())
    .fetch_optional(&mut *conn)
    .await
    .map_err(PrivateError::from)?;
    let sent_at = newest.unwrap_or_else(now_ms);
    if let Some(mut contact) = contacts::load(&mut conn, open.store().sealer(), peer_bytes).await? {
        contact.last_read_at = sent_at;
        contacts::save(&mut conn, open.store().sealer(), &contact, now_ms()).await?;
        let view = service.contact_view(&open, &mut conn, &contact).await?;
        drop(conn);
        service.emit(PrivateEvent::ContactChanged(view));
    }
    Ok(())
}

/// Sets whether notifications from `peer` are muted.
pub async fn private_set_muted(
    service: &PrivateService,
    peer: &str,
    muted: bool,
) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let open = service.opened()?;
    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    if let Some(mut contact) = contacts::load(&mut conn, open.store().sealer(), peer_bytes).await? {
        contact.muted = muted;
        contacts::save(&mut conn, open.store().sealer(), &contact, now_ms()).await?;
        let view = service.contact_view(&open, &mut conn, &contact).await?;
        drop(conn);
        service.emit(PrivateEvent::ContactChanged(view));
    }
    Ok(())
}

/// Hides conversation with `peer`.
pub async fn private_hide(service: &PrivateService, peer: &str) -> Result<(), CoreError> {
    let peer_bytes = parse_hex::<32>(peer, "that person isn't valid")?;
    let open = service.opened()?;
    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    if let Some(mut contact) = contacts::load(&mut conn, open.store().sealer(), peer_bytes).await? {
        contact.hidden = true;
        contacts::save(&mut conn, open.store().sealer(), &contact, now_ms()).await?;
        let view = service.contact_view(&open, &mut conn, &contact).await?;
        drop(conn);
        service.emit(PrivateEvent::ContactChanged(view));
    }
    Ok(())
}
