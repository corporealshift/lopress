//! Lightweight env-gated timing spans.
//!
//! Enable by setting the `LOPRESS_TIMING` environment variable to any
//! non-empty value before launching. When enabled, dropping a [`Span`]
//! prints `[timing] <name>: <ms>ms` to stderr.
//!
//! When disabled (the default), [`span`] returns a guard holding no
//! `Instant` — effectively free.
//!
//! Note: CI does not set `LOPRESS_TIMING`, so spans are no-ops in CI runs.

use std::sync::OnceLock;
use std::time::Instant;

static ENABLED: OnceLock<bool> = OnceLock::new();

/// Read once at first use, cached for the process lifetime.
fn enabled() -> bool {
    *ENABLED.get_or_init(|| {
        std::env::var_os("LOPRESS_TIMING")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    })
}

/// A timing scope guard. When dropped, if recording, prints the elapsed
/// time to stderr in `[timing] <name>: <ms>ms` form.
pub struct Span {
    name: &'static str,
    started: Option<Instant>,
}

impl Span {
    /// Construct a span using the env-var-cached enabled flag.
    fn new(name: &'static str) -> Self {
        Self::new_with_enabled(name, enabled())
    }

    /// Test seam: construct a span with an explicit enabled flag, bypassing
    /// the env-var cache. Intended for tests that should not depend on
    /// process-wide environment state.
    pub fn new_with_enabled(name: &'static str, enabled: bool) -> Self {
        Self {
            name,
            started: if enabled { Some(Instant::now()) } else { None },
        }
    }

    /// Whether this span will produce output on drop. Useful for tests.
    pub fn is_recording(&self) -> bool {
        self.started.is_some()
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(t0) = self.started {
            let ms = t0.elapsed().as_millis();
            eprintln!("[timing] {name}: {ms}ms", name = self.name);
        }
    }
}

/// Start a timing span. Pair with `let _t = ...;` so the guard drops at
/// the end of the enclosing scope.
///
/// ```ignore
/// fn slow_thing() {
///     let _t = lopress_core::perf::span("module.slow_thing");
///     // ... work ...
/// }
/// ```
pub fn span(name: &'static str) -> Span {
    Span::new(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn disabled_span_does_not_record() {
        let s = Span::new_with_enabled("test.disabled", false);
        assert!(!s.is_recording());
    }

    #[test]
    fn enabled_span_records() {
        let s = Span::new_with_enabled("test.enabled", true);
        assert!(s.is_recording());
    }

    #[test]
    fn enabled_span_drop_is_safe_after_sleep() {
        let s = Span::new_with_enabled("test.drop", true);
        sleep(Duration::from_millis(5));
        drop(s); // must not panic
    }
}
