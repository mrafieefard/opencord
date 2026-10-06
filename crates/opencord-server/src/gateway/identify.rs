//! Authenticating new sessions and resuming old ones.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;

use opencord_common::auth::{self, Nonce};
use opencord_common::validation;
use opencord_proto::v1 as proto;
use proto::envelope::Payload;

use super::frames;
use super::session::{CloseCode, Connection, Session};
use crate::bootstrap::hash_claim_token;
use crate::db::invites;
use crate::db::meta::ServerMeta;
use crate::db::users::{self, UserRow};
use crate::db::{bans, members};
use crate::error::ApiError;
use crate::guild::{Member, User};
use crate::random;
use crate::state::{AppState, Audience, now_ms};

/// Why an `Identify` was refused, and how to close the connection.
#[derive(Debug)]
pub struct Rejection {
    pub error: ApiError,
    pub close: CloseCode,
}

impl Rejection {
    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            error: ApiError::unauthorized(message),
            close: CloseCode::AUTHENTICATION_FAILED,
        }
    }
}

impl From<ApiError> for Rejection {
    fn from(error: ApiError) -> Self {
        Self {
            error,
            close: CloseCode::AUTHENTICATION_FAILED,
        }
    }
}

impl From<sqlx::Error> for Rejection {
    fn from(error: sqlx::Error) -> Self {
        ApiError::from(error).into()
    }
}

/// Verifies the signature, admits or creates the member, and opens a
/// session. `Ready` is queued on the connection before anything else.
pub async fn identify(
    state: &Arc<AppState>,
    identify: proto::Identify,
    nonce: &Nonce,
    ip: IpAddr,
    connection: &Connection,
) -> Result<Arc<Session>, Rejection> {
    state
        .rate_limits
        .check_identify(ip)
        .map_err(|wait| Rejection {
            error: ApiError::rate_limited(wait),
            close: CloseCode::RATE_LIMITED,
        })?;
    if identify.protocol_version != opencord_common::PROTOCOL_VERSION {
        return Err(ApiError::invalid_argument(format!(
            "protocol version {} is not supported; this server speaks {}",
            identify.protocol_version,
            opencord_common::PROTOCOL_VERSION
        ))
        .into());
    }
    let challenge = auth::Challenge {
        server_id: state.guild().meta.server_id,
        certificate: state.fingerprint,
        nonce: *nonce,
        timestamp_ms: identify.timestamp_ms,
    };
    let now = now_ms();
    auth::verify(
        &identify.public_key,
        &identify.signature,
        &challenge,
        u64::try_from(now).unwrap_or_default(),
    )
    .map_err(|error| Rejection::unauthorized(error.to_string()))?;

    let _writes = state.write_lock().await;
    let joined = admit(state, &identify, now).await?;
    if joined.newly_joined {
        let member = state
            .guild()
            .members
            .get(&joined.user_id)
            .map(Member::to_proto);
        state.broadcast(
            proto::event::Kind::MemberJoin(proto::MemberJoin { member }),
            Audience::Everyone,
        );
    }
    if joined.claimed {
        let server = Some(state.guild().server_info());
        state.broadcast(
            proto::event::Kind::ServerUpdate(proto::ServerUpdate { server }),
            Audience::Everyone,
        );
    }
    Ok(open_session(state, joined.user_id, connection))
}

pub fn resume(
    state: &Arc<AppState>,
    resume: &proto::Resume,
    connection: &Connection,
) -> Result<Arc<Session>, ApiError> {
    let session = state
        .sessions
        .get(&resume.session_id)
        .ok_or_else(ApiError::invalid_session)?;
    session
        .resume(
            &resume.resume_token,
            resume.last_seq,
            connection.clone(),
            Instant::now(),
        )
        .map_err(|_| ApiError::invalid_session())?;
    Ok(session)
}

struct Joined {
    user_id: i64,
    newly_joined: bool,
    claimed: bool,
}

/// Decides whether the key may join, and records the membership.
async fn admit(
    state: &AppState,
    identify: &proto::Identify,
    now: i64,
) -> Result<Joined, Rejection> {
    let mut tx = state.db.begin().await?;
    let existing = users::find_by_public_key(&mut tx, &identify.public_key).await?;
    if let Some(user) = &existing
        && bans::is_banned(&mut tx, user.id).await?
    {
        return Err(Rejection::unauthorized("you are banned from this server"));
    }
    let (meta, is_member) = {
        let guild = state.guild();
        let is_member = existing
            .as_ref()
            .is_some_and(|user| guild.members.contains_key(&user.id));
        (guild.meta.clone(), is_member)
    };

    let claimed = match &identify.claim_token {
        Some(token) if meta.claim_token_hash == Some(hash_claim_token(token)) => true,
        Some(_) => {
            return Err(Rejection::unauthorized(
                "the claim token is invalid or has already been used",
            ));
        }
        None => false,
    };
    let invite = match (&identify.invite_code, is_member || claimed) {
        (_, true) => None,
        (Some(code), false) => Some(
            invites::find(&mut tx, code)
                .await?
                .filter(|invite| invite.is_usable(now))
                .ok_or_else(|| Rejection::unauthorized("the invite is invalid or has expired"))?,
        ),
        (None, false) if meta.open_join => None,
        (None, false) => {
            return Err(Rejection::unauthorized(
                "you are not a member of this server; join with an invite",
            ));
        }
    };

    let user = match existing {
        Some(user) => user,
        None => {
            let user = UserRow {
                id: state.ids.next_id(),
                public_key: identify.public_key.clone(),
                display_name: validation::display_name(&identify.display_name)
                    .map_err(ApiError::from)?,
            };
            users::insert(&mut tx, &user, now).await?;
            user
        }
    };
    if let Some(invite) = &invite {
        invites::increment_uses(&mut tx, &invite.code).await?;
    }
    if !is_member {
        members::insert(&mut tx, user.id, now).await?;
    }
    let claimed_meta = claimed.then_some(ServerMeta {
        owner_id: Some(user.id),
        claim_token_hash: None,
        ..meta
    });
    if let Some(meta) = &claimed_meta {
        meta.save(&mut tx).await?;
    }
    tx.commit().await?;

    let mut guild = state.guild_mut();
    if !is_member {
        guild.members.insert(
            user.id,
            Member {
                user: User {
                    id: user.id,
                    public_key: user.public_key,
                    display_name: user.display_name,
                },
                nickname: None,
                role_ids: Vec::new(),
                joined_at: now,
            },
        );
    }
    if let Some(meta) = claimed_meta {
        guild.meta = meta;
    }
    Ok(Joined {
        user_id: user.id,
        newly_joined: !is_member,
        claimed,
    })
}

fn open_session(state: &AppState, user_id: i64, connection: &Connection) -> Arc<Session> {
    let session_id = hex::encode(random::bytes::<16>());
    let resume_token = random::bytes::<32>();
    let session = Arc::new(Session::new(
        session_id.clone(),
        user_id,
        &resume_token,
        connection.clone(),
    ));
    let was_online = state.sessions.has_user(user_id);
    let ready = ready(state, user_id, session_id, resume_token.to_vec());
    if connection
        .outbound
        .try_send(frames::encode(Payload::Ready(Box::new(ready))))
        .is_err()
    {
        connection.closer.close(CloseCode::TOO_SLOW);
    }
    state.sessions.insert(Arc::clone(&session));
    if !was_online {
        state.broadcast_presence(user_id);
    }
    session
}

fn ready(
    state: &AppState,
    user_id: i64,
    session_id: String,
    resume_token: Vec<u8>,
) -> proto::Ready {
    let mut presences = state.presences();
    if !presences.iter().any(|presence| presence.user_id == user_id) {
        let status = state.presence.chosen(user_id);
        if status != proto::PresenceStatus::Offline {
            presences.push(proto::Presence {
                user_id,
                status: status as i32,
            });
        }
    }
    let guild = state.guild();
    let visible = guild.visible_channel_ids(user_id);
    proto::Ready {
        session_id,
        resume_token,
        self_user: guild
            .members
            .get(&user_id)
            .map(|member| member.user.to_proto()),
        server: Some(guild.server_info()),
        channels: guild
            .channels
            .values()
            .filter(|channel| visible.contains(&channel.id))
            .map(|channel| channel.to_proto())
            .collect(),
        roles: guild.roles.values().map(|role| role.to_proto()).collect(),
        members: guild.members.values().map(Member::to_proto).collect(),
        presences,
        server_permissions: guild.base_permissions(user_id).bits(),
        channel_permissions: visible
            .iter()
            .map(|channel_id| {
                (
                    *channel_id,
                    guild.channel_permissions(user_id, *channel_id).bits(),
                )
            })
            .collect(),
    }
}
