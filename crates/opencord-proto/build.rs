const PROTO_ROOT: &str = "../../proto";
const PROTO_FILES: [&str; 4] = [
    "opencord/v1/gateway.proto",
    "opencord/v1/models.proto",
    "opencord/v1/requests.proto",
    "opencord/v1/events.proto",
];

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed={PROTO_ROOT}");
    let files = PROTO_FILES.map(|file| format!("{PROTO_ROOT}/{file}"));
    prost_build::compile_protos(&files, &[PROTO_ROOT])
}
