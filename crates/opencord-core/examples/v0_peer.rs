//! The server owner in the app's Phase 2 V0 check (app/tool/check_v0.sh),
//! on the same core as the app. It claims the server, adds a second voice
//! channel and writes an invite link for the app. Once the member is in,
//! it joins the General voice channel; when the member says in #general
//! that it is in General too, it moves them to Lounge; when the member says
//! it is in Lounge, it disconnects them. The app checks each step shows
//! without reconnecting.
//!
//!   v0_peer <address> <claim token> <invite file>

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use opencord_core::api::types::{
    AddServerOutcome, ChannelKind, CoreEvent, CoreEventPayload, ReadySnapshot,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use tokio::sync::mpsc::UnboundedReceiver;

/// The app is built before it joins, which takes a while.
const JOIN_WAIT: Duration = Duration::from_secs(600);
const WAIT: Duration = Duration::from_secs(120);

/// What the member writes in #general once it is in each channel.
const IN_GENERAL: &str = "I am in General";
const IN_LOUNGE: &str = "I am in Lounge";

type Failure = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [address, claim, invite_file] = args.as_slice() else {
        eprintln!("usage: v0_peer <address> <claim token> <invite file>");
        return ExitCode::from(2);
    };
    match run(address, claim, Path::new(invite_file)).await {
        Ok(()) => {
            println!("v0_peer: done");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("v0_peer: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(address: &str, claim: &str, invite_file: &Path) -> Result<(), Failure> {
    let dir = tempfile::tempdir()?;
    let (client, mut events) = Client::new(dir.path(), tokio::runtime::Handle::current())?;
    client.set_identity(Identity::generate(), "Owner".to_owned());

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
    let general_voice = channel(&ready, "General")?;
    let general_text = channel(&ready, "general")?;
    let lounge = client
        .create_channel(&key, ChannelKind::Voice, "Lounge".to_owned(), None, None)
        .await?
        .id;

    let invite = client.create_invite(&key, Some(1), None).await?;
    // Whole or not at all: the driver starts the app once the file is there.
    let partial = invite_file.with_extension("partial");
    std::fs::write(&partial, &invite.link)?;
    std::fs::rename(&partial, invite_file)?;
    println!("v0_peer: invite written, waiting for the member");

    let member_id = wait_for(&mut events, JOIN_WAIT, |payload| match payload {
        CoreEventPayload::MemberJoin(member) => Some(member.user.id),
        _ => None,
    })
    .await?;
    client.voice_join(&key, general_voice).await?;
    println!("v0_peer: in General, waiting for the member there");

    said(&mut events, member_id, general_text, IN_GENERAL).await?;
    client.move_member(&key, member_id, lounge).await?;
    println!("v0_peer: moved the member to Lounge");

    said(&mut events, member_id, general_text, IN_LOUNGE).await?;
    client.disconnect_member(&key, member_id).await?;
    println!("v0_peer: disconnected the member");
    Ok(())
}

/// Waits for `author` to write `content` in `channel`.
async fn said(
    events: &mut UnboundedReceiver<CoreEvent>,
    author: i64,
    channel: i64,
    content: &str,
) -> Result<(), Failure> {
    wait_for(events, WAIT, |payload| match payload {
        CoreEventPayload::MessageCreate(message)
            if message.author_id == author
                && message.channel_id == channel
                && message.content == content =>
        {
            Some(())
        }
        _ => None,
    })
    .await
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

fn channel(ready: &ReadySnapshot, name: &str) -> Result<i64, Failure> {
    ready
        .channels
        .iter()
        .find(|channel| channel.name == name)
        .map(|channel| channel.id)
        .ok_or_else(|| format!("the server has no channel named {name}").into())
}
