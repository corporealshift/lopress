// A justified allow: this comment is DIRECTLY above the attribute, so the gate
// treats it as the justification. Expected: NOT flagged.
#[allow(dead_code)]
fn justified_fn() {}

#[allow(dead_code)]
fn unjustified_fn() {} // expected: FLAGGED (no comment directly above)

#[cfg(test)]
mod tests {
    // exempt_fn's attribute has no comment above it, proving the exemption is
    // keyed on the lint set, not on an adjacent comment.
    #[allow(clippy::unwrap_used)]
    fn exempt_fn() {
        // expected: NOT flagged (canonical test-panic lint)
        let _ = Some(1).unwrap();
    }

    #[allow(dead_code)]
    fn flagged_in_test() {
        // expected: FLAGGED — dead_code is not in the exempt set, no comment above
        let _ = 42;
    }
}
