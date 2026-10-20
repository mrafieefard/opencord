//! The client in the media uploads plan's transfer check (Milestone A8,
//! app/tool/check_transfer.sh), on the same core as the app. It claims the
//! server, sends a file of the given size to #general, saves it back, checks
//! the copy is the same, and prints how fast each way went. The driver
//! watches the server's memory meanwhile.
//!
//!   transfer_load <address> <claim token> <size in MiB> <folder>

use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use opencord_core::api::types::{AddServerOutcome, CoreEventPayload, OutgoingFile};
use opencord_core::client::Client;
use opencord_core::identity::Identity;

const WAIT: Duration = Duration::from_secs(120);
const MIB: u64 = 1024 * 1024;

type Failure = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [address, claim, size_mib, folder] = args.as_slice() else {
        eprintln!("usage: transfer_load <address> <claim token> <size in MiB> <folder>");
        return ExitCode::from(2);
    };
    let Ok(size_mib) = size_mib.parse::<u64>() else {
        eprintln!("transfer_load: the size must be a number of MiB");
        return ExitCode::from(2);
    };
    match run(address, claim, size_mib * MIB, Path::new(folder)).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("transfer_load: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(address: &str, claim: &str, size: u64, folder: &Path) -> Result<(), Failure> {
    let original = folder.join("original.bin");
    let copy = folder.join("copy.bin");
    write_file(&original, size)?;

    let (client, mut events) =
        Client::new(&folder.join("client"), tokio::runtime::Handle::current())?;
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
    let general = tokio::time::timeout(WAIT, async {
        while let Some(event) = events.recv().await {
            if let CoreEventPayload::Ready(ready) = event.payload {
                return ready
                    .channels
                    .iter()
                    .find(|channel| channel.name == "general")
                    .map(|channel| channel.id);
            }
        }
        None
    })
    .await?
    .ok_or("the server has no #general")?;

    println!("transfer_load: uploading");
    let started = Instant::now();
    let message = client
        .send_message_with_files(
            &key,
            general,
            String::new(),
            "transfer-load".to_owned(),
            vec![OutgoingFile {
                path: Some(original.to_string_lossy().into_owned()),
                bytes: None,
                filename: "load.bin".to_owned(),
            }],
        )
        .await?;
    let upload = started.elapsed();
    let attachment = message
        .attachments
        .first()
        .ok_or("the message has no file")?;

    println!("transfer_load: downloading");
    let started = Instant::now();
    client
        .download_attachment(&key, attachment.id, copy.to_string_lossy().into_owned())
        .await?;
    let download = started.elapsed();
    if !same_contents(&original, &copy)? {
        return Err("the saved copy differs from the file sent".into());
    }

    let mb = size as f64 / 1e6;
    println!(
        "transfer_load: upload {:.1} MB/s ({mb:.0} MB in {:.1} s, until the message was sent)",
        mb / upload.as_secs_f64(),
        upload.as_secs_f64()
    );
    println!(
        "transfer_load: download {:.1} MB/s ({mb:.0} MB in {:.1} s)",
        mb / download.as_secs_f64(),
        download.as_secs_f64()
    );
    Ok(())
}

/// `size` bytes that don't repeat within a block, so nothing on the way can
/// shortcut them.
fn write_file(path: &Path, size: u64) -> std::io::Result<()> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut block = vec![0u8; MIB as usize];
    let mut left = size;
    while left > 0 {
        for chunk in block.as_chunks_mut::<8>().0 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *chunk = state.to_le_bytes();
        }
        let count = left.min(MIB) as usize;
        file.write_all(&block[..count])?;
        left -= count as u64;
    }
    file.flush()
}

fn same_contents(a: &Path, b: &Path) -> std::io::Result<bool> {
    let (mut a, mut b) = (std::fs::File::open(a)?, std::fs::File::open(b)?);
    if a.metadata()?.len() != b.metadata()?.len() {
        return Ok(false);
    }
    let (mut left, mut right) = (vec![0u8; MIB as usize], vec![0u8; MIB as usize]);
    loop {
        let count = a.read(&mut left)?;
        if count == 0 {
            return Ok(true);
        }
        b.read_exact(&mut right[..count])?;
        if left[..count] != right[..count] {
            return Ok(false);
        }
    }
}
