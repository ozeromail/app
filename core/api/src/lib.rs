//! Facade the Android and iOS apps call through UniFFI.
//!
//! UniFFI bindings are added together with the Android app.

/// Version of the core, shown in About and attached to manual crash reports.
pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::core_version().is_empty());
    }
}
