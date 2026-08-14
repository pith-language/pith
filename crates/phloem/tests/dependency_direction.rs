//! The dependency direction 0009 and scope's build-without-package base case
//! rest on: phloem produces build requests against interfaces xylem already
//! declares, and xylem knows nothing about packages or about phloem. The
//! resolved dependency graph is where that direction either holds or does
//! not, so the graph is what this asserts — read from `cargo metadata`,
//! never matched against manifest spelling, which diverges from the graph in
//! both directions: a reformatted manifest would break the spelling, and a
//! transitive edge through a third crate would skip it.

use std::collections::BTreeMap;
use std::process::Command;

use serde_json::Value as Json;

#[test]
fn phloem_consumes_xylem_and_xylem_cannot_reach_phloem() {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Json = serde_json::from_slice(&output.stdout).unwrap();

    // The resolved graph as package-name edges: every dependency of every
    // workspace member and of everything they pull in, transitive edges
    // included. Dev- and build-dependency edges are dependencies; the
    // direction claim has to hold over all of them or it holds over none.
    let mut names = BTreeMap::new();
    for package in metadata.get("packages").and_then(Json::as_array).unwrap() {
        names.insert(
            package
                .get("id")
                .and_then(Json::as_str)
                .unwrap()
                .to_string(),
            package
                .get("name")
                .and_then(Json::as_str)
                .unwrap()
                .to_string(),
        );
    }
    let nodes = metadata
        .get("resolve")
        .and_then(|resolve| resolve.get("nodes"))
        .and_then(Json::as_array)
        .unwrap();
    let mut edges = BTreeMap::new();
    for node in nodes {
        let from = names
            .get(node.get("id").and_then(Json::as_str).unwrap())
            .unwrap()
            .clone();
        let mut to = Vec::new();
        for dep in node.get("deps").and_then(Json::as_array).unwrap() {
            to.push(
                names
                    .get(dep.get("pkg").and_then(Json::as_str).unwrap())
                    .unwrap()
                    .clone(),
            );
        }
        edges.insert(from, to);
    }

    let phloem_deps = edges.get("phloem").unwrap();
    assert!(
        phloem_deps.iter().any(|dep| dep == "xylem"),
        "phloem should depend on xylem; its resolved dependencies are {phloem_deps:?}"
    );

    // Every path out of xylem, not just its direct edges: a phloem edge
    // smuggled through a third crate is still the reversed direction 0009
    // forbids, and the direct-edge check above would not see it.
    let mut reachable = vec!["xylem".to_string()];
    let mut visited = vec!["xylem".to_string()];
    while let Some(package) = reachable.pop() {
        for dependency in edges.get(&package).map_or(&[][..], Vec::as_slice) {
            assert_ne!(
                dependency, "phloem",
                "xylem reaches phloem through {package}: the dependency direction is reversed"
            );
            if !visited.contains(dependency) {
                visited.push(dependency.clone());
                reachable.push(dependency.clone());
            }
        }
    }
}
