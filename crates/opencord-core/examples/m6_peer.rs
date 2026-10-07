//! The server owner in the app's Phase 1 M6 check (app/tool/check_m6.sh),
//! on the same core as the app. It claims the server and writes an invite
//! link for the app to join with; answers the member's first message in
//! #general (so the answer reaches a channel they have open); then gives
//! them a hoisted role that may not send messages there. The app checks
//! each step arrives without reconnecting.
//!
//!   m6_peer <address> <claim token> <invite file>

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use opencord_common::permissions::Permissions;
use opencord_core::api::types::{
    AddServerOutcome, CoreEvent, CoreEventPayload, OverwriteTargetKind, PermissionOverwrite,
    ReadySnapshot,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use tokio::sync::mpsc::UnboundedReceiver;

/// The app is built before it joins, which takes a while.
const JOIN_WAIT: Duration = Duration::from_secs(600);
const WAIT: Duration = Duration::from_secs(120);

const ANSWER: &str = "Hello from the owner";
const ROLE: &str = "Readers";

type Failure = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [address, claim, invite_file] = args.as_slice() else {
        eprintln!("usage: m6_peer <address> <claim token> <invite file>");
        return ExitCode::from(2);
    };
    match run(address, claim, Path::new(invite_file)).await {
        Ok(()) => {
            println!("m6_peer: done");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("m6_peer: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(address: &str, claim: &str, invite_file: &Path) -> Result<(), Failure> {
    let dir = tempfile::tempdir()?;
    let (client, mut events) = Client::new(dir.path(), tokio::runtime::Handle::current())?;
    client.set_identity(Identity::generate(), "Owner".to_owned());

    // A self-signed server: trust what it presents, as the app's owner did
    // in M5, then claim it.
    if let AddServerOutcome::NeedsTrust {
        address,
        fingerprint,
    } = client.add_server(address, Some(claim.to_owned())).await?
    {
        client.trust_fingerprint(&address, &fingerprint)?;
    }
    let AddServerOutcome::Added(server) =
        client.add_server(address, Some(claim.to_owned())).await?
    else {
        return Err("the server still wants its certificate trusted".into());
    };
    let key = server.key;
    let ready = wait_for(&mut events, WAIT, |payload| match payload {
        CoreEventPayload::Ready(ready) => Some(ready.clone()),
        _ => None,
    })
    .await?;
    let general = general(&ready)?;

    let invite = client.create_invite(&key, Some(1), None).await?;
    // Whole or not at all: the driver starts the app once the file is there.
    let partial = invite_file.with_extension("partial");
    std::fs::write(&partial, &invite.link)?;
    std::fs::rename(&partial, invite_file)?;
    println!("m6_peer: invite written, waiting for the member");

    let member = wait_for(&mut events, JOIN_WAIT, |payload| match payload {
        CoreEventPayload::MemberJoin(member) => Some(member.clone()),
        _ => None,
    })
    .await?;
    let member_id = member.user.id;
    println!("m6_peer: {} joined", member.user.display_name);

    let said = wait_for(&mut events, WAIT, |payload| match payload {
        CoreEventPayload::MessageCreate(message)
            if message.author_id == member_id && message.channel_id == general =>
        {
            Some(message.content.clone())
        }
        _ => None,
    })
    .await?;
    println!("m6_peer: the member said {said:?}");
    client
        .send_message(&key, general, ANSWER.to_owned(), "m6-answer".to_owned())
        .await?;

    let role = client
        .create_role(&key, ROLE.to_owned(), 0x3B_A5_5C, 0, true, false)
        .await?;
    client.add_member_role(&key, member_id, role.id).await?;
    client
        .set_channel_overwrite(
            &key,
            general,
            PermissionOverwrite {
                target_kind: OverwriteTargetKind::Role,
                target_id: role.id,
                allow: 0,
                deny: bits(Permissions::SEND_MESSAGES),
            },
        )
        .await?;
    println!("m6_peer: {ROLE} may not send in #general");
    Ok(())
}

/// Skips events until `pick` returns something.
async fn wait_for<T>(
    events: &mut UnboundedReceiver<CoreEvent>,
    wait: Duration,
    mut pick: impl FnMut(&CoreEventPayload) -> Option<T>,
) -> Result<T, Failure> {
    tokio::time::timeout(wait, async {
        while let Some(event) = events.recv().await {
            if let Some(found) = pick(&event.payload) {
                return Ok(found);
            }
        }
        Err("the event stream ended".into())
    })
    .await
    .map_err(|_| -> Failure { format!("nothing arrived in {}s", wait.as_secs()).into() })?
}

fn general(ready: &ReadySnapshot) -> Result<i64, Failure> {
    ready
        .channels
        .iter()
        .find(|channel| channel.name == "general")
        .map(|channel| channel.id)
        .ok_or_else(|| "the server has no #general".into())
}

/// Permission bits as the API carries them.
fn bits(permissions: Permissions) -> i64 {
    i64::from_ne_bytes(permissions.bits().to_ne_bytes())
}
