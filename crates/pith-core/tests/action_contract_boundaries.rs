//! Boundary and invariance tests for the declared action contract (A-1).
//!
//! These intentionally exercise the public API rather than codec internals:
//! callers must be able to validate a contract before planning or execution,
//! and its digest must distinguish semantic changes while ignoring ordering in
//! fields whose order has no meaning.

use pith_core::{
    ActionInput, ActionOutput, ActionProgram, ActionSpec, CapabilityRequirement, Content,
    EnvironmentVariable, NetworkPolicy, OutputKind, PlatformRequirement,
};
use pith_ids::ContentId;

fn valid_spec() -> ActionSpec {
    ActionSpec {
        executable: ActionProgram::HostPath("/bin/tool".into()),
        toolchain: Box::new([]),
        arguments: ["--mode".into(), "release".into()].into(),
        inputs: [
            ActionInput {
                path: "inputs/source".into(),
                content: Content::Blob(ContentId::of_blob(b"source")),
            },
            ActionInput {
                path: "inputs/includes".into(),
                content: Content::Tree(ContentId::of_tree(b"includes")),
            },
        ]
        .into(),
        outputs: [
            ActionOutput {
                path: "outputs/program".into(),
                kind: OutputKind::Blob,
            },
            ActionOutput {
                path: "outputs/debug".into(),
                kind: OutputKind::Tree,
            },
        ]
        .into(),
        environment: [
            EnvironmentVariable {
                name: "LANG".into(),
                value: "C.UTF-8".into(),
            },
            EnvironmentVariable {
                name: "MODE".into(),
                value: "release".into(),
            },
        ]
        .into(),
        platform: PlatformRequirement::Exact {
            operating_system: "linux".into(),
            architecture: "x86_64".into(),
        },
        capabilities: [
            CapabilityRequirement {
                name: "filesystem.read".into(),
                scope: "inputs".into(),
            },
            CapabilityRequirement {
                name: "filesystem.write".into(),
                scope: "outputs".into(),
            },
        ]
        .into(),
        network: NetworkPolicy::AllowHosts(
            ["registry.example".into(), "mirror.example".into()].into(),
        ),
    }
}

fn validation_message(spec: &ActionSpec) -> String {
    spec.validate()
        .err()
        .map(|diag| diag.to_string())
        .unwrap_or_default()
}

#[test]
fn a_fully_populated_contract_is_valid() {
    assert!(valid_spec().validate().is_ok());
}

#[test]
fn empty_and_unicode_arguments_are_valid() {
    let mut spec = valid_spec();
    spec.arguments = ["".into(), "Gruesse-世界".into()].into();

    assert!(spec.validate().is_ok());
}

#[test]
fn an_argument_containing_nul_is_rejected() {
    let mut spec = valid_spec();
    spec.arguments = ["before\0after".into()].into();

    assert!(validation_message(&spec).contains("argument"));
}

#[test]
fn every_non_relative_path_shape_is_rejected_for_inputs_and_outputs() {
    for path in [
        "",
        "/absolute",
        "trailing/",
        "double//component",
        "./child",
        "parent/../child",
        "windows\\separator",
        "nul\0component",
    ] {
        let mut input_spec = ActionSpec::isolated("/bin/tool");
        input_spec.inputs = [ActionInput {
            path: path.into(),
            content: Content::Blob(ContentId::of_blob(b"input")),
        }]
        .into();
        assert!(
            input_spec.validate().is_err(),
            "input path {path:?} unexpectedly validated"
        );

        let mut output_spec = ActionSpec::isolated("/bin/tool");
        output_spec.outputs = [ActionOutput {
            path: path.into(),
            kind: OutputKind::Blob,
        }]
        .into();
        assert!(
            output_spec.validate().is_err(),
            "output path {path:?} unexpectedly validated"
        );
    }
}

#[test]
fn duplicate_and_ancestor_input_paths_are_rejected() {
    for second in ["source", "source/generated"] {
        let mut spec = ActionSpec::isolated("/bin/tool");
        spec.inputs = [
            ActionInput {
                path: "source".into(),
                content: Content::Blob(ContentId::of_blob(b"first")),
            },
            ActionInput {
                path: second.into(),
                content: Content::Blob(ContentId::of_blob(b"second")),
            },
        ]
        .into();

        assert!(validation_message(&spec).contains("overlaps"));
    }
}

#[test]
fn duplicate_and_ancestor_output_paths_are_rejected() {
    for second in ["result", "result/nested"] {
        let mut spec = ActionSpec::isolated("/bin/tool");
        spec.outputs = [
            ActionOutput {
                path: "result".into(),
                kind: OutputKind::Blob,
            },
            ActionOutput {
                path: second.into(),
                kind: OutputKind::Tree,
            },
        ]
        .into();

        assert!(validation_message(&spec).contains("overlaps"));
    }
}

#[test]
fn input_output_overlap_is_rejected_in_both_ancestor_directions() {
    for (input, output) in [("work", "work/out"), ("work/source", "work")] {
        let mut spec = ActionSpec::isolated("/bin/tool");
        spec.inputs = [ActionInput {
            path: input.into(),
            content: Content::Blob(ContentId::of_blob(b"input")),
        }]
        .into();
        spec.outputs = [ActionOutput {
            path: output.into(),
            kind: OutputKind::Blob,
        }]
        .into();

        assert!(validation_message(&spec).contains("overlaps"));
    }
}

#[test]
fn lexical_prefixes_that_are_not_path_ancestors_do_not_overlap() {
    let mut spec = ActionSpec::isolated("/bin/tool");
    spec.inputs = [ActionInput {
        path: "source".into(),
        content: Content::Blob(ContentId::of_blob(b"input")),
    }]
    .into();
    spec.outputs = [ActionOutput {
        path: "source-map".into(),
        kind: OutputKind::Blob,
    }]
    .into();

    assert!(spec.validate().is_ok());
}

#[test]
fn invalid_environment_names_are_rejected() {
    for name in ["", "A=B", "A\0B"] {
        let mut spec = ActionSpec::isolated("/bin/tool");
        spec.environment = [EnvironmentVariable {
            name: name.into(),
            value: "value".into(),
        }]
        .into();

        assert!(validation_message(&spec).contains("environment variable name"));
    }
}

#[test]
fn environment_values_may_be_empty_but_not_contain_nul() {
    let mut empty = ActionSpec::isolated("/bin/tool");
    empty.environment = [EnvironmentVariable {
        name: "EMPTY".into(),
        value: "".into(),
    }]
    .into();
    assert!(empty.validate().is_ok());

    let mut nul = empty;
    if let Some(variable) = nul.environment.first_mut() {
        variable.value = "a\0b".into();
    }
    assert!(validation_message(&nul).contains("contains a NUL byte"));
}

#[test]
fn duplicate_environment_names_are_rejected_even_with_different_values() {
    let mut spec = ActionSpec::isolated("/bin/tool");
    spec.environment = [
        EnvironmentVariable {
            name: "MODE".into(),
            value: "debug".into(),
        },
        EnvironmentVariable {
            name: "MODE".into(),
            value: "release".into(),
        },
    ]
    .into();

    assert!(validation_message(&spec).contains("duplicate"));
}

#[test]
fn an_exact_platform_requires_both_components() {
    for (operating_system, architecture) in [("", "x86_64"), ("linux", "")] {
        let mut spec = ActionSpec::isolated("/bin/tool");
        spec.platform = PlatformRequirement::Exact {
            operating_system: operating_system.into(),
            architecture: architecture.into(),
        };

        assert!(validation_message(&spec).contains("platform"));
    }
}

#[test]
fn capabilities_require_nonempty_nul_free_names_and_scopes() {
    for (name, scope) in [
        ("", "scope"),
        ("name", ""),
        ("na\0me", "scope"),
        ("name", "sco\0pe"),
    ] {
        let mut spec = ActionSpec::isolated("/bin/tool");
        spec.capabilities = [CapabilityRequirement {
            name: name.into(),
            scope: scope.into(),
        }]
        .into();

        assert!(validation_message(&spec).contains("capability"));
    }
}

#[test]
fn only_exact_duplicate_capabilities_are_rejected() {
    let capability = CapabilityRequirement {
        name: "filesystem.read".into(),
        scope: "one".into(),
    };
    let mut duplicate = ActionSpec::isolated("/bin/tool");
    duplicate.capabilities = [capability.clone(), capability].into();
    assert!(validation_message(&duplicate).contains("duplicate"));

    let mut distinct_scope = ActionSpec::isolated("/bin/tool");
    distinct_scope.capabilities = [
        CapabilityRequirement {
            name: "filesystem.read".into(),
            scope: "one".into(),
        },
        CapabilityRequirement {
            name: "filesystem.read".into(),
            scope: "two".into(),
        },
    ]
    .into();
    assert!(distinct_scope.validate().is_ok());
}

#[test]
fn allowed_hosts_must_be_nonempty_nul_free_and_unique() {
    for hosts in [
        vec![""],
        vec!["bad\0host"],
        vec!["same.example", "same.example"],
    ] {
        let mut spec = ActionSpec::isolated("/bin/tool");
        spec.network = NetworkPolicy::AllowHosts(
            hosts
                .into_iter()
                .map(Box::<str>::from)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );

        assert!(spec.validate().is_err());
    }
}

#[test]
fn digest_ignores_order_for_set_like_contract_fields() {
    let first = valid_spec();
    let mut reordered = first.clone();
    reordered.inputs.reverse();
    reordered.outputs.reverse();
    reordered.environment.reverse();
    reordered.capabilities.reverse();
    if let NetworkPolicy::AllowHosts(hosts) = &mut reordered.network {
        hosts.reverse();
    }

    assert_eq!(first.digest().ok(), reordered.digest().ok());
}

#[test]
fn digest_preserves_argument_order() {
    let first = valid_spec();
    let mut reordered = first.clone();
    reordered.arguments.reverse();

    assert_ne!(first.digest().ok(), reordered.digest().ok());
}

#[test]
fn digest_distinguishes_blob_and_tree_discriminants() {
    let content = ContentId::of_blob(b"same identity payload");
    let mut blob_input = ActionSpec::isolated("/bin/tool");
    blob_input.inputs = [ActionInput {
        path: "input".into(),
        content: Content::Blob(content),
    }]
    .into();
    let mut tree_input = blob_input.clone();
    if let Some(input) = tree_input.inputs.first_mut() {
        input.content = Content::Tree(content);
    }
    assert_ne!(blob_input.digest().ok(), tree_input.digest().ok());

    let mut blob_output = ActionSpec::isolated("/bin/tool");
    blob_output.outputs = [ActionOutput {
        path: "output".into(),
        kind: OutputKind::Blob,
    }]
    .into();
    let mut tree_output = blob_output.clone();
    if let Some(output) = tree_output.outputs.first_mut() {
        output.kind = OutputKind::Tree;
    }
    assert_ne!(blob_output.digest().ok(), tree_output.digest().ok());
}

#[test]
fn stored_encoding_round_trip_preserves_declared_order() {
    let spec = valid_spec();
    let decoded = ActionSpec::decode_stored(&spec.encode_stored());

    assert_eq!(decoded.ok(), Some(spec));
}
