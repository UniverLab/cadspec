//! Guards for the crate's public metadata: `Cargo.toml` links and README badges.
//!
//! This metadata is what readers and search engines follow, and it drifted once
//! (Bing backlink review 2026-09-29): `homepage` pointed at the repository, the
//! README advertised a crates.io badge for a crate that is not published there,
//! and nothing linked the experiment page. These tests read both files as text
//! and assert the facts the review asked for. No network access: the URLs were
//! verified with `curl -sI` when this spec was designed, not at test time.

use std::fs;
use std::path::Path;

const EXPERIMENT_HOME: &str = "https://univerlab.org/cadspec/";
const REPOSITORY: &str = "https://github.com/UniverLab/cadspec";

fn package_file(name: &str) -> String {
    // CARGO_MANIFEST_DIR, not the cwd: the test must not depend on where cargo
    // was invoked from.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Value of a top-level `key = "value"` line in the manifest.
fn manifest_field<'a>(manifest: &'a str, key: &str) -> &'a str {
    let prefix = format!("{key} = ");
    let line = manifest
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("Cargo.toml has no `{key}` field"));
    line.split_once("= ")
        .map(|(_, value)| value.trim_matches('"'))
        .expect("`key = value` line")
}

#[test]
fn cargo_manifest_homepage_is_the_experiment_page() {
    let manifest = package_file("Cargo.toml");
    assert_eq!(
        manifest_field(&manifest, "homepage"),
        EXPERIMENT_HOME,
        "homepage must be the experiment page, not the repository"
    );
    assert_eq!(
        manifest_field(&manifest, "repository"),
        REPOSITORY,
        "repository must stay the GitHub repo"
    );
}

#[test]
fn readme_has_no_crates_io_badge() {
    let readme = package_file("README.md");
    assert!(
        !readme.contains("img.shields.io/crates"),
        "cadspec is not published on crates.io, so no crates.io version badge"
    );
    // The same 404 URL survived once as a markdown link ("Available on
    // crates.io") above a `cargo install cadspec` that cannot resolve — the
    // badge-shaped assertions did not catch it. While the crate is
    // unpublished, no README route may mention crates.io at all.
    assert!(
        !readme.contains("crates.io"),
        "no README route may link crates.io while the crate is unpublished"
    );
    assert!(
        !readme.contains("cargo install cadspec"),
        "`cargo install cadspec` only resolves against the crates.io registry"
    );
}

#[test]
fn readme_keeps_the_badges_that_stay() {
    let readme = package_file("README.md");
    for badge in [
        "img.shields.io/github/actions/workflow/status",
        "img.shields.io/badge/Status-Active",
        "img.shields.io/badge/License-MIT",
    ] {
        assert!(
            readme.contains(badge),
            "README lost a badge it must keep: {badge}"
        );
    }
}

#[test]
fn readme_links_the_experiment_page_once_under_the_badges() {
    let readme = package_file("README.md");
    // Counting the scheme-qualified URL is what makes "once" mean once: the
    // install host `https://install.univerlab.org/cadspec` shares the tail
    // `univerlab.org/cadspec` but not this whole string.
    assert_eq!(
        readme.matches(EXPERIMENT_HOME).count(),
        1,
        "the experiment page must be linked exactly once"
    );
    let badges_end = readme.find("</p>").expect("badge block");
    let home = readme.find(EXPERIMENT_HOME).expect("home link");
    let features = readme.find("## Features").expect("Features heading");
    assert!(
        home > badges_end && home < features,
        "the home link belongs right under the badges, above the intro"
    );
}

#[test]
fn readme_primary_install_route_is_the_short_url() {
    let readme = package_file("README.md");
    assert!(
        readme.contains("curl -fsSL https://install.univerlab.org/cadspec | sh"),
        "install.univerlab.org/cadspec 302s to the repo's scripts/install.sh, so it is the primary route"
    );
    assert!(
        !readme.contains("raw.githubusercontent.com/UniverLab/cadspec/main/scripts/install.sh"),
        "the long raw install.sh URL was replaced, not duplicated"
    );
    // Case A only touches the curl line: the PowerShell route stays raw.
    assert!(
        readme
            .contains("irm https://raw.githubusercontent.com/UniverLab/cadspec/main/scripts/install.ps1 | iex"),
        "the Windows install line is out of scope and must be unchanged"
    );
}
