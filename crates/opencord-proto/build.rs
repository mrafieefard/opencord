const PROTO_ROOT: &str = "../../proto";
const PROTO_FILES: [&str; 9] = [
    "opencord/v1/commands.proto",
    "opencord/v1/gateway.proto",
    "opencord/v1/models.proto",
    "opencord/v1/music.proto",
    "opencord/v1/network.proto",
    "opencord/v1/requests.proto",
    "opencord/v1/events.proto",
    "opencord/v1/voice.proto",
    "opencord/v1/internal.proto",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed={PROTO_ROOT}");
    let descriptors = protox::compile(PROTO_FILES, [PROTO_ROOT])?;
    prost_build::Config::new()
        // Ready is far larger than every other envelope payload.
        .boxed(".opencord.v1.Envelope.payload.ready")
        // So is the network's description, the owner's alone.
        .boxed(".opencord.v1.Event.kind.network_status")
        .boxed(".opencord.v1.Response.result.network_info")
        // And a Lavalink node's.
        .boxed(".opencord.v1.Event.kind.lavalink_node_status")
        .boxed(".opencord.v1.Response.result.lavalink_nodes")
        // And music's players and settings.
        .boxed(".opencord.v1.Event.kind.music_snapshot")
        .boxed(".opencord.v1.Event.kind.music_update")
        .boxed(".opencord.v1.Event.kind.music_settings_update")
        .boxed(".opencord.v1.Response.result.music_session")
        .boxed(".opencord.v1.Response.result.music_settings")
        .compile_fds(descriptors)?;
    Ok(())
}
