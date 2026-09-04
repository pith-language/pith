//! The wire shape of the query API.
//!
//! `--output json` is the machine surface, so these snapshots are the contract
//! a reader parses. A diff here is a `QUERY_API_VERSION` bump, not a rename to
//! wave through.

use pith_output::dto::{
    AboutValueRepr, AboutView, ActionContractRepr, ActionPlanView, ActionProgramRepr,
    AttemptCounts, AttemptStatusRepr, CheckReport, ContentPreview, DeclarationBodyRepr,
    DeclarationView, DependenciesView, DependencyKindRepr, DependencyNodeRepr, DiagnosticRepr,
    EntryView, EvaluationSourceRepr, ExitStatusContractRepr, FmtReport, FmtStatus, GcPreview,
    ImportView, InterfaceRepr, ModuleView, NetworkPolicyRepr, PlatformRequirementRepr,
    QUERY_API_VERSION, QueryView, RuleCategoryRepr, RuleView, RunView, SelectionView, SeverityRepr,
    StateCheck, StateInfo, StoredContent, StoredContentKind, SumConstructorRepr, TierRepr,
    TreeEntryRepr, TreeListing, TypeRepr, ValueRepr,
};
use pith_output::{OutputRecord, PlainRenderer, Renderer};

const DIGEST: &str = "13e68a5b642bf49fc4ed28d527abe43f3bca517f0f742af916910104019a0ce0";

fn check_report() -> CheckReport {
    CheckReport {
        module: "example".into(),
        path: "fixtures/example.pi".into(),
        abi_digest: Some(DIGEST.into()),
        diagnostics: Box::new([DiagnosticRepr {
            severity: SeverityRepr::Warning,
            code: 3001,
            label: Some("example.pi".into()),
            line: Some(7),
            column: Some(3),
            message: "`Bindings` is imported and never used".into(),
        }]),
        errors: 0,
        warnings: 1,
    }
}

fn fmt_report() -> FmtReport {
    FmtReport {
        module: "example".into(),
        path: "fixtures/example.pi".into(),
        status: FmtStatus::WouldFormat,
    }
}

fn module_view() -> ModuleView {
    ModuleView {
        module: "example".into(),
        path: "fixtures/example.pi".into(),
        abi_digest: DIGEST.into(),
        imports: Box::new([ImportView {
            module: "xylem".into(),
            abi_digest: DIGEST.into(),
        }]),
        declarations: Box::new([
            DeclarationView {
                name: "Renderer".into(),
                body: DeclarationBodyRepr::Nominal {
                    representation: Box::new(TypeRepr::Blob),
                },
                rendered: "Blob".into(),
                digest: DIGEST.into(),
                documentation: "the renderer a document is produced by".into(),
            },
            DeclarationView {
                name: "Outcome".into(),
                body: DeclarationBodyRepr::Sum {
                    constructors: Box::new([
                        SumConstructorRepr {
                            name: "failed".into(),
                            payload: Some(Box::new(TypeRepr::Text)),
                        },
                        SumConstructorRepr {
                            name: "produced".into(),
                            payload: None,
                        },
                    ]),
                },
                rendered: "failed(Text) | produced".into(),
                digest: DIGEST.into(),
                documentation: String::new().into(),
            },
        ]),
        rules: Box::new([RuleView {
            label: "render".into(),
            category: RuleCategoryRepr::Action,
            tier: TierRepr::Host,
            interface: InterfaceRepr {
                inputs: Box::new([
                    TypeRepr::Nominal {
                        name: "example.Renderer".into(),
                    },
                    TypeRepr::Nominal {
                        name: "example.Template".into(),
                    },
                ]),
                output: Box::new(TypeRepr::Nominal {
                    name: "example.Document".into(),
                }),
                rendered: "(example.Renderer, example.Template) -> example.Document".into(),
            },
            documentation: String::new().into(),
        }]),
        entries: Box::new([EntryView {
            name: "build".into(),
            coordinate: "example::entry.build".into(),
            tier: TierRepr::Represented,
            interface: InterfaceRepr {
                inputs: Box::new([]),
                output: Box::new(TypeRepr::Nominal {
                    name: "example.Document".into(),
                }),
                rendered: "() -> example.Document".into(),
            },
            documentation: "produce the default document".into(),
        }]),
        about: Box::new([AboutView {
            fields: Box::new([
                (
                    "owners".into(),
                    AboutValueRepr::List {
                        elements: Box::new(["docs".into(), "release".into()]),
                    },
                ),
                (
                    "purpose".into(),
                    AboutValueRepr::Text {
                        text: "contract fixture".into(),
                    },
                ),
            ]),
            documentation: "module metadata".into(),
        }]),
    }
}

fn entry_interface() -> InterfaceRepr {
    InterfaceRepr {
        inputs: Box::new([]),
        output: Box::new(TypeRepr::Text),
        rendered: "() -> Text".into(),
    }
}

fn run_view() -> RunView {
    RunView {
        entry: "build".into(),
        coordinate: "example::entry.build".into(),
        interface: entry_interface(),
        source: EvaluationSourceRepr::Hydrated,
        value: ValueRepr::Text { s: "ready".into() },
    }
}

fn selection_view() -> SelectionView {
    SelectionView {
        entry: "build".into(),
        rule: "example::entry.build".into(),
        tier: TierRepr::Represented,
        interface: entry_interface(),
    }
}

fn action_plan_view() -> ActionPlanView {
    ActionPlanView {
        entry: "build".into(),
        rule: "example.compile".into(),
        spec_digest: DIGEST.into(),
        contract: ActionContractRepr {
            executable: ActionProgramRepr::HostPath {
                path: "/usr/bin/cc".into(),
            },
            toolchain: Box::new([DIGEST.into()]),
            arguments: Box::new(["-c".into(), "main.c".into()]),
            inputs: Box::new([]),
            outputs: Box::new([]),
            environment: Box::new([]),
            platform: PlatformRequirementRepr::Exact {
                operating_system: "linux".into(),
                architecture: "x86_64".into(),
            },
            capabilities: Box::new([]),
            network: NetworkPolicyRepr::Deny,
            exit_status: ExitStatusContractRepr::SuccessRequired,
        },
    }
}

fn dependencies_view() -> DependenciesView {
    DependenciesView {
        entry: "build".into(),
        root: Some(Box::new(DependencyNodeRepr {
            label: "example::entry.build".into(),
            attempt: Some(7),
            status: Some(AttemptStatusRepr::Complete),
            dependency: DependencyKindRepr::Pure {
                digest: DIGEST.into(),
            },
            children: Box::new([DependencyNodeRepr {
                label: "example.render".into(),
                attempt: Some(8),
                status: Some(AttemptStatusRepr::Complete),
                dependency: DependencyKindRepr::Action,
                children: Box::new([]),
            }]),
        })),
    }
}

fn tree_listing() -> TreeListing {
    TreeListing {
        tree: DIGEST.into(),
        entries: Box::new([
            TreeEntryRepr::File {
                name: "main.c".into(),
                content: DIGEST.into(),
                executable: false,
            },
            TreeEntryRepr::Symlink {
                name: "latest".into(),
                target: "../build/main".into(),
            },
            TreeEntryRepr::Tree {
                name: "include".into(),
                content: DIGEST.into(),
            },
        ]),
    }
}

fn state_info() -> StateInfo {
    StateInfo {
        adapter: "sqlite".into(),
        schema_version: 1,
        semantic_encoding_version: 1,
        attempts: AttemptCounts {
            total: 7,
            pending: 0,
            complete: 5,
            failed: 1,
            cancelled: 1,
        },
        reusable_index: 3,
    }
}

fn gc_preview() -> GcPreview {
    GcPreview {
        roots: 3,
        retained_attempts: 5,
        reclaimable_attempts: 2,
        content: ContentPreview {
            blobs: 9,
            trees: 2,
            live_blobs: 6,
            live_trees: 1,
            reclaimable_blobs: 3,
            reclaimable_trees: 1,
            total_bytes: 4096,
            live_bytes: 2048,
            reclaimable_bytes: 2048,
        },
    }
}

fn every_view() -> Vec<QueryView> {
    vec![
        QueryView::Check(check_report()),
        QueryView::Format(fmt_report()),
        QueryView::Module(module_view()),
        QueryView::Tree(tree_listing()),
        QueryView::Content(StoredContent {
            id: DIGEST.into(),
            kind: StoredContentKind::Tree,
            path: Some("fixtures".into()),
        }),
        QueryView::State(state_info()),
        QueryView::StateCheck(StateCheck { records: 7 }),
        QueryView::Gc(gc_preview()),
        QueryView::Run(run_view()),
        QueryView::Selection(selection_view()),
        QueryView::ActionPlan(action_plan_view()),
        QueryView::Dependencies(dependencies_view()),
    ]
}

#[test]
fn check_report_shape_is_stable() {
    let record = OutputRecord::query(QueryView::Check(check_report()));
    insta::assert_json_snapshot!(serde_json::to_value(&record).unwrap());
}

#[test]
fn fmt_report_shape_is_stable() {
    let record = OutputRecord::query(QueryView::Format(fmt_report()));
    insta::assert_json_snapshot!(serde_json::to_value(&record).unwrap());
}

#[test]
fn module_view_shape_is_stable() {
    let record = OutputRecord::query(QueryView::Module(module_view()));
    insta::assert_json_snapshot!(serde_json::to_value(&record).unwrap());
}

#[test]
fn tree_listing_shape_is_stable() {
    let record = OutputRecord::query(QueryView::Tree(tree_listing()));
    insta::assert_json_snapshot!(serde_json::to_value(&record).unwrap());
}

#[test]
fn stored_content_shape_is_stable() {
    let record = OutputRecord::query(QueryView::Content(StoredContent {
        id: DIGEST.into(),
        kind: StoredContentKind::Blob,
        path: None,
    }));
    insta::assert_json_snapshot!(serde_json::to_value(&record).unwrap());
}

#[test]
fn state_info_shape_is_stable() {
    let record = OutputRecord::query(QueryView::State(state_info()));
    insta::assert_json_snapshot!(serde_json::to_value(&record).unwrap());
}

#[test]
fn state_check_shape_is_stable() {
    let record = OutputRecord::query(QueryView::StateCheck(StateCheck { records: 12 }));
    insta::assert_json_snapshot!(serde_json::to_value(&record).unwrap());
}

#[test]
fn gc_preview_shape_is_stable() {
    let record = OutputRecord::query(QueryView::Gc(gc_preview()));
    insta::assert_json_snapshot!(serde_json::to_value(&record).unwrap());
}

#[test]
fn run_view_shape_is_stable() {
    let record = OutputRecord::query(QueryView::Run(run_view()));
    insta::assert_json_snapshot!(serde_json::to_value(&record).unwrap());
}

#[test]
fn selection_view_shape_is_stable() {
    let record = OutputRecord::query(QueryView::Selection(selection_view()));
    insta::assert_json_snapshot!(serde_json::to_value(&record).unwrap());
}

#[test]
fn action_plan_view_shape_is_stable() {
    let record = OutputRecord::query(QueryView::ActionPlan(action_plan_view()));
    insta::assert_json_snapshot!(serde_json::to_value(&record).unwrap());
}

#[test]
fn dependencies_view_shape_is_stable() {
    let record = OutputRecord::query(QueryView::Dependencies(dependencies_view()));
    insta::assert_json_snapshot!(serde_json::to_value(&record).unwrap());
}

/// A reader consuming one JSON line in isolation still has to know which
/// contract it is reading, so the version rides every query record and is
/// never passed in by the caller.
#[test]
fn every_query_record_carries_the_api_version() {
    for view in every_view() {
        let json = serde_json::to_value(OutputRecord::query(view)).unwrap();
        assert_eq!(
            json.get("api_version").and_then(serde_json::Value::as_u64),
            Some(u64::from(QUERY_API_VERSION)),
            "a query record went out without the contract version"
        );
        assert_eq!(
            json.get("kind").and_then(serde_json::Value::as_str),
            Some("query")
        );
    }
}

/// The view is nested under `query` rather than flattened into the envelope.
/// A DTO is free to name a field `kind` or `code`, and a flattened view would
/// overwrite the envelope's own field of that name — the shape this test
/// exists because `StoredContent.kind` silently did.
#[test]
fn a_view_field_cannot_overwrite_an_envelope_field() {
    let record = OutputRecord::query(QueryView::Content(StoredContent {
        id: DIGEST.into(),
        kind: StoredContentKind::Blob,
        path: None,
    }));
    let json = serde_json::to_value(&record).unwrap();

    assert_eq!(
        json.get("kind").and_then(serde_json::Value::as_str),
        Some("query"),
        "a view's own `kind` reached the envelope"
    );
    assert_eq!(
        json.pointer("/query/kind")
            .and_then(serde_json::Value::as_str),
        Some("blob"),
        "the view's `kind` is not where a reader would look for it"
    );
}

/// The `view` tag is what a reader switches on. Two views sharing one tag
/// would make the surface ambiguous in the one field that disambiguates it.
#[test]
fn every_view_has_its_own_tag() {
    let mut seen: Vec<String> = Vec::new();
    for view in every_view() {
        let json = serde_json::to_value(OutputRecord::query(view)).unwrap();
        let tag = json
            .pointer("/query/view")
            .and_then(serde_json::Value::as_str)
            .expect("a query record must carry its view tag")
            .to_owned();
        assert!(
            !seen.contains(&tag),
            "two query views share the tag `{tag}`"
        );
        seen.push(tag);
    }
    assert_eq!(seen.len(), 12, "a view was added without a snapshot");
}

/// The plain renderer is the default when stdout is not a TTY, so every view
/// has to say something there. An empty render would make `pith explore | less`
/// print nothing.
#[test]
fn every_view_renders_in_the_plain_shape() {
    for view in every_view() {
        let mut buffer: Vec<u8> = Vec::new();
        {
            let mut renderer = PlainRenderer::new(&mut buffer);
            renderer.emit(&OutputRecord::query(view)).unwrap();
            renderer.finish().unwrap();
        }
        let rendered = String::from_utf8(buffer).unwrap();
        assert!(
            rendered.starts_with("[query] ") && rendered.trim().len() > "[query]".len(),
            "a query view rendered as nothing in the plain shape: {rendered:?}"
        );
        assert!(
            rendered.lines().all(|line| line.starts_with("[query] ")),
            "a multiline query line lost its record frame: {rendered:?}"
        );
        assert!(
            rendered.is_ascii(),
            "the plain shape is ASCII only: {rendered:?}"
        );
    }
}
