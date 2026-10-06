fn main() {
    println!(
        "opencord-server {} (protocol v{})",
        env!("CARGO_PKG_VERSION"),
        opencord_common::PROTOCOL_VERSION
    );
}
