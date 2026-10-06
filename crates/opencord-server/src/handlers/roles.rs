use std::collections::HashSet;

use opencord_common::limits::MAX_ROLES;
use opencord_common::permissions::{OverwriteTarget, Permissions};
use opencord_common::validation;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::response::Result as Response;

use super::{Ctx, ack};
use crate::db::{channels, members, roles};
use crate::error::ApiError;
use crate::guild::{Guild, Member, Role};
use crate::permissions::{require_base, require_grantable, require_outranks_role};
use crate::state::Audience;
use crate::visibility::Visibility;

const MAX_COLOR: u32 = 0x00FF_FFFF;

pub async fn create(ctx: &Ctx<'_>, request: proto::CreateRole) -> Result<Response, ApiError> {
    let name = validation::role_name(&request.name)?;
    let color = valid_color(request.color)?;
    let permissions = Permissions::from_bits_truncate(request.permissions);

    let _writes = ctx.state.write_lock().await;
    let shifted: Vec<Role> = {
        let guild = ctx.state.guild();
        let held = require_base(&guild, ctx.user_id, Permissions::MANAGE_ROLES)?;
        require_grantable(held, permissions)?;
        if guild.roles.len() >= MAX_ROLES {
            return Err(ApiError::conflict(format!(
                "a server can have at most {MAX_ROLES} roles"
            )));
        }
        guild
            .roles
            .values()
            .filter(|role| role.position >= 1)
            .map(|role| Role {
                position: role.position + 1,
                ..role.clone()
            })
            .collect()
    };
    let role = Role {
        id: ctx.state.ids.next_id(),
        name,
        color,
        position: 1,
        permissions,
        hoist: request.hoist,
        mentionable: request.mentionable,
    };
    let mut tx = ctx.state.db.begin().await?;
    for moved in &shifted {
        roles::update(&mut tx, &moved.to_row()).await?;
    }
    roles::insert(&mut tx, &role.to_row()).await?;
    tx.commit().await?;

    {
        let mut guild = ctx.state.guild_mut();
        for moved in &shifted {
            guild.roles.insert(moved.id, moved.clone());
        }
        guild.roles.insert(role.id, role.clone());
    }
    ctx.state.broadcast(
        Event::RoleCreate(proto::RoleCreate {
            role: Some(role.to_proto()),
        }),
        Audience::Everyone,
    );
    broadcast_role_updates(ctx, &shifted);
    Ok(Response::Role(role.to_proto()))
}

pub async fn update(ctx: &Ctx<'_>, request: proto::UpdateRole) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let updated = {
        let guild = ctx.state.guild();
        let role = existing_role(&guild, request.role_id)?;
        let held = require_base(&guild, ctx.user_id, Permissions::MANAGE_ROLES)?;
        require_outranks_role(&guild, ctx.user_id, role.position)?;
        let is_everyone = role.id == guild.meta.everyone_role_id;
        let name = match &request.name {
            Some(_) if is_everyone => {
                return Err(ApiError::invalid_argument("@everyone cannot be renamed"));
            }
            Some(name) => validation::role_name(name)?,
            None => role.name.clone(),
        };
        let permissions = match request.permissions {
            Some(bits) => {
                let permissions = Permissions::from_bits_truncate(bits);
                require_grantable(held, permissions.symmetric_difference(role.permissions))?;
                permissions
            }
            None => role.permissions,
        };
        Role {
            name,
            color: request.color.map_or(Ok(role.color), valid_color)?,
            permissions,
            hoist: request.hoist.unwrap_or(role.hoist),
            mentionable: request.mentionable.unwrap_or(role.mentionable),
            ..role.clone()
        }
    };
    roles::update(&mut *ctx.state.db.acquire().await?, &updated.to_row()).await?;

    let before = Visibility::capture(ctx.state);
    ctx.state
        .guild_mut()
        .roles
        .insert(updated.id, updated.clone());
    broadcast_role_updates(ctx, std::slice::from_ref(&updated));
    before.announce(ctx.state, &[]);
    Ok(Response::Role(updated.to_proto()))
}

pub async fn delete(ctx: &Ctx<'_>, request: proto::DeleteRole) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let (holders, channels_with_overwrites) = {
        let guild = ctx.state.guild();
        let role = existing_role(&guild, request.role_id)?;
        if role.id == guild.meta.everyone_role_id {
            return Err(ApiError::invalid_argument("@everyone cannot be deleted"));
        }
        require_base(&guild, ctx.user_id, Permissions::MANAGE_ROLES)?;
        require_outranks_role(&guild, ctx.user_id, role.position)?;
        let holders: Vec<i64> = guild
            .members
            .values()
            .filter(|member| member.role_ids.contains(&role.id))
            .map(|member| member.user.id)
            .collect();
        let target = OverwriteTarget::Role(role.id);
        let channels: Vec<i64> = guild
            .channels
            .values()
            .filter(|channel| channel.overwrites.iter().any(|o| o.target == target))
            .map(|channel| channel.id)
            .collect();
        (holders, channels)
    };
    let mut tx = ctx.state.db.begin().await?;
    channels::delete_overwrites_for_target(&mut tx, "role", request.role_id).await?;
    roles::delete(&mut tx, request.role_id).await?;
    tx.commit().await?;

    let before = Visibility::capture(ctx.state);
    let updated_members: Vec<Member> = {
        let mut guild = ctx.state.guild_mut();
        guild.roles.remove(&request.role_id);
        let target = OverwriteTarget::Role(request.role_id);
        for channel_id in &channels_with_overwrites {
            if let Some(channel) = guild.channels.get_mut(channel_id) {
                channel
                    .overwrites
                    .retain(|overwrite| overwrite.target != target);
            }
        }
        holders
            .iter()
            .filter_map(|user_id| {
                let member = guild.members.get_mut(user_id)?;
                member.role_ids.retain(|id| *id != request.role_id);
                Some(member.clone())
            })
            .collect()
    };
    ctx.state.broadcast(
        Event::RoleDelete(proto::RoleDelete {
            role_id: request.role_id,
        }),
        Audience::Everyone,
    );
    for member in &updated_members {
        broadcast_member_update(ctx, member);
    }
    before.announce(ctx.state, &channels_with_overwrites);
    Ok(ack())
}

pub async fn reorder(ctx: &Ctx<'_>, request: proto::ReorderRoles) -> Result<Response, ApiError> {
    let unique: HashSet<i64> = request.role_ids.iter().copied().collect();
    if request.role_ids.is_empty() || unique.len() != request.role_ids.len() {
        return Err(ApiError::invalid_argument(
            "list each role to reorder exactly once",
        ));
    }
    let _writes = ctx.state.write_lock().await;
    let moved: Vec<Role> = {
        let guild = ctx.state.guild();
        require_base(&guild, ctx.user_id, Permissions::MANAGE_ROLES)?;
        let listed: Vec<&Role> = request
            .role_ids
            .iter()
            .map(|role_id| {
                let role = existing_role(&guild, *role_id)?;
                if role.id == guild.meta.everyone_role_id {
                    return Err(ApiError::invalid_argument("@everyone cannot be moved"));
                }
                require_outranks_role(&guild, ctx.user_id, role.position)?;
                Ok(role)
            })
            .collect::<Result<_, ApiError>>()?;
        let mut positions: Vec<i32> = listed.iter().map(|role| role.position).collect();
        positions.sort_unstable();
        listed
            .into_iter()
            .zip(positions)
            .filter(|(role, position)| role.position != *position)
            .map(|(role, position)| Role {
                position,
                ..role.clone()
            })
            .collect()
    };
    let mut tx = ctx.state.db.begin().await?;
    for role in &moved {
        roles::update(&mut tx, &role.to_row()).await?;
    }
    tx.commit().await?;
    {
        let mut guild = ctx.state.guild_mut();
        for role in &moved {
            guild.roles.insert(role.id, role.clone());
        }
    }
    broadcast_role_updates(ctx, &moved);
    Ok(ack())
}

pub async fn add_to_member(
    ctx: &Ctx<'_>,
    request: proto::AddMemberRole,
) -> Result<Response, ApiError> {
    change_member_role(ctx, request.user_id, request.role_id, true).await
}

pub async fn remove_from_member(
    ctx: &Ctx<'_>,
    request: proto::RemoveMemberRole,
) -> Result<Response, ApiError> {
    change_member_role(ctx, request.user_id, request.role_id, false).await
}

async fn change_member_role(
    ctx: &Ctx<'_>,
    user_id: i64,
    role_id: i64,
    add: bool,
) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let (member, changed) = {
        let guild = ctx.state.guild();
        let held = require_base(&guild, ctx.user_id, Permissions::MANAGE_ROLES)?;
        let role = existing_role(&guild, role_id)?;
        if role.id == guild.meta.everyone_role_id {
            return Err(ApiError::invalid_argument(
                "every member has @everyone; it cannot be added or removed",
            ));
        }
        require_outranks_role(&guild, ctx.user_id, role.position)?;
        if add {
            require_grantable(held, role.permissions)?;
        }
        let member = guild
            .members
            .get(&user_id)
            .ok_or_else(|| ApiError::not_found("member"))?;
        let has_role = member.role_ids.contains(&role_id);
        let role_ids = match (add, has_role) {
            (true, false) => member.role_ids.iter().copied().chain([role_id]).collect(),
            (false, true) => member
                .role_ids
                .iter()
                .copied()
                .filter(|id| *id != role_id)
                .collect(),
            _ => member.role_ids.clone(),
        };
        let changed = add != has_role;
        (
            Member {
                role_ids,
                ..member.clone()
            },
            changed,
        )
    };
    if !changed {
        return Ok(Response::Member(member.to_proto()));
    }
    let mut conn = ctx.state.db.acquire().await?;
    if add {
        members::add_role(&mut conn, user_id, role_id).await?;
    } else {
        members::remove_role(&mut conn, user_id, role_id).await?;
    }

    let before = Visibility::capture(ctx.state);
    ctx.state
        .guild_mut()
        .members
        .insert(user_id, member.clone());
    broadcast_member_update(ctx, &member);
    before.announce(ctx.state, &[]);
    Ok(Response::Member(member.to_proto()))
}

fn existing_role(guild: &Guild, role_id: i64) -> Result<&Role, ApiError> {
    guild
        .roles
        .get(&role_id)
        .ok_or_else(|| ApiError::not_found("role"))
}

fn valid_color(color: u32) -> Result<u32, ApiError> {
    if color <= MAX_COLOR {
        Ok(color)
    } else {
        Err(ApiError::invalid_argument("color must be a 0xRRGGBB value"))
    }
}

fn broadcast_role_updates(ctx: &Ctx<'_>, roles: &[Role]) {
    for role in roles {
        ctx.state.broadcast(
            Event::RoleUpdate(proto::RoleUpdate {
                role: Some(role.to_proto()),
            }),
            Audience::Everyone,
        );
    }
}

pub(super) fn broadcast_member_update(ctx: &Ctx<'_>, member: &Member) {
    ctx.state.broadcast(
        Event::MemberUpdate(proto::MemberUpdate {
            member: Some(member.to_proto()),
        }),
        Audience::Everyone,
    );
}
