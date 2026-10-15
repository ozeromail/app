//! Facade the Android and iOS apps call through UniFFI.

uniffi::setup_scaffolding!();

/// Version of the core, shown in About and in manual crash reports.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::core_version().is_empty());
    }
}
