/// Version of the client core and the gateway protocol it speaks,
/// e.g. `0.1.0 (protocol v1)`.
#[flutter_rust_bridge::frb(sync)]
pub fn core_version() -> String {
    format!(
        "{} (protocol v{})",
        env!("CARGO_PKG_VERSION"),
        opencord_common::PROTOCOL_VERSION
    )
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_version_reports_crate_and_protocol_versions() {
        assert_eq!(
            core_version(),
            concat!(env!("CARGO_PKG_VERSION"), " (protocol v1)")
        );
    }
}
