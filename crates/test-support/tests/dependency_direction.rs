//! `domain` and `application` must never reach a concrete technology, not even transitively.

use std::collections::HashSet;
use std::process::Command;

use serde_json::Value;

const FORBIDDEN: [&str; 4] = ["tauri", "cpal", "reqwest", "windows"];

fn metadata() -> Value {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run cargo metadata");
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).expect("parse cargo metadata")
}

fn name_of(id: &str, packages: &[Value]) -> String {
    let package = packages.iter().find(|package| package["id"] == id);
    package.expect("known package")["name"]
        .as_str()
        .expect("package name")
        .to_owned()
}

/// Names of everything `root` needs to build, leaving out its dev-dependencies.
fn normal_dependencies(root: &str, metadata: &Value) -> HashSet<String> {
    let packages = metadata["packages"].as_array().expect("packages");
    let nodes = metadata["resolve"]["nodes"].as_array().expect("nodes");

    let root_id = packages
        .iter()
        .find(|package| package["name"] == root)
        .and_then(|package| package["id"].as_str())
        .expect("root package");

    let mut seen = HashSet::new();
    let mut pending = vec![root_id.to_owned()];
    while let Some(id) = pending.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let node = nodes
            .iter()
            .find(|node| node["id"] == id.as_str())
            .expect("node");
        for dependency in node["deps"].as_array().expect("deps") {
            let is_normal = dependency["dep_kinds"]
                .as_array()
                .expect("dep kinds")
                .iter()
                .any(|kind| kind["kind"].is_null());
            if is_normal {
                pending.push(dependency["pkg"].as_str().expect("pkg").to_owned());
            }
        }
    }

    seen.iter().map(|id| name_of(id, packages)).collect()
}

#[test]
fn the_core_crates_stay_free_of_concrete_technologies() {
    let metadata = metadata();

    for root in ["instantpad-domain", "instantpad-application"] {
        let dependencies = normal_dependencies(root, &metadata);
        for forbidden in FORBIDDEN {
            assert!(
                !dependencies.contains(forbidden),
                "{root} depends on {forbidden}"
            );
        }
    }
}
