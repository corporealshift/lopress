#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Tests for hand-rolled `lopress` CLI argument parsing (no clap).

use lopress_build::cli::{parse, Command};
use std::path::PathBuf;

fn args(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn no_args_launches_gui() {
    assert!(matches!(parse(&args(&[])), Command::Gui));
}

#[test]
fn new_with_dir_defaults_title_to_basename_and_localhost_base_url() {
    match parse(&args(&["new", "my-blog"])) {
        Command::NewSite {
            dir,
            title,
            base_url,
        } => {
            assert_eq!(dir, PathBuf::from("my-blog"));
            assert_eq!(title, "my-blog");
            assert_eq!(base_url, "http://localhost:8080");
        }
        other => panic!("expected NewSite, got {other:?}"),
    }
}

#[test]
fn new_with_explicit_flags() {
    match parse(&args(&[
        "new",
        "site",
        "--title",
        "My Blog",
        "--base-url",
        "https://x.dev",
    ])) {
        Command::NewSite {
            title, base_url, ..
        } => {
            assert_eq!(title, "My Blog");
            assert_eq!(base_url, "https://x.dev");
        }
        other => panic!("expected NewSite, got {other:?}"),
    }
}

#[test]
fn new_without_dir_is_usage_error() {
    assert!(matches!(parse(&args(&["new"])), Command::Usage(_)));
}

#[test]
fn flag_without_value_is_usage_error() {
    assert!(matches!(
        parse(&args(&["new", "site", "--title"])),
        Command::Usage(_)
    ));
}

#[test]
fn unrecognized_first_arg_launches_gui() {
    // The binary is GUI-first: anything that isn't the `new` subcommand just
    // launches the editor, matching the pre-CLI behavior. This never errors a
    // user who runs `lopress <workspace>`.
    assert!(matches!(parse(&args(&["frobnicate"])), Command::Gui));
    assert!(matches!(parse(&args(&["my-workspace"])), Command::Gui));
}
