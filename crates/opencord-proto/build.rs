const PROTO_ROOT: &str = "../../proto";
const PROTO_FILES: [&str; 6] = [
    "opencord/v1/gateway.proto",
    "opencord/v1/models.proto",
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
        .compile_fds(descriptors)?;
    Ok(())
}
