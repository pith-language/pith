//! One description of a query view, rendered twice.
//!
//! The plain and pretty renderers differ in color, not in what they say. A
//! description is built once as styled lines here and painted or not at the
//! renderer, so the two shapes cannot drift into reporting different facts
//! about one view.

use crate::dto::{
    AboutValueRepr, ActionPlanView, ActionProgramRepr, AttemptStatusRepr, CheckReport,
    DeclarationBodyRepr, DeclarationView, DependenciesView, DependencyKindRepr, DependencyNodeRepr,
    DiagnosticRepr, EntryView, EvaluationSourceRepr, FmtReport, FmtStatus, GcPreview, ModuleView,
    QueryView, RuleCategoryRepr, RuleView, RunView, SelectionView, SeverityRepr, StateCheck,
    StateInfo, StoredContent, StoredContentKind, SumConstructorRepr, TreeEntryRepr, TreeListing,
    TypeRepr, ValueRepr,
};
use crate::palette::{self, Palette, Role};

#[derive(Default)]
pub(crate) struct Lines {
    lines: Vec<Line>,
}

struct Line {
    indent: usize,
    segments: Vec<Segment>,
}

struct Segment {
    role: Option<Role>,
    text: String,
}

impl Lines {
    fn line(&mut self, indent: usize) -> &mut Line {
        self.lines.push(Line {
            indent,
            segments: Vec::new(),
        });
        self.lines
            .last_mut()
            .unwrap_or_else(|| unreachable!("the line was just pushed"))
    }

    /// Render every line, painting roles only when a palette is supplied.
    pub(crate) fn render(&self, palette: Option<Palette>) -> String {
        let mut out = String::new();
        let mut separator = "";
        for line in &self.lines {
            out.push_str(separator);
            for _ in 0..line.indent {
                out.push_str("  ");
            }
            for segment in &line.segments {
                match (segment.role, palette) {
                    (Some(role), Some(palette)) => {
                        out.push_str(&palette::paint(palette.style(role), &segment.text));
                    }
                    _ => out.push_str(&segment.text),
                }
            }
            separator = "\n";
        }
        out
    }
}

impl Line {
    fn plain(&mut self, text: impl Into<String>) -> &mut Self {
        self.segments.push(Segment {
            role: None,
            text: text.into(),
        });
        self
    }

    fn styled(&mut self, role: Role, text: impl Into<String>) -> &mut Self {
        self.segments.push(Segment {
            role: Some(role),
            text: text.into(),
        });
        self
    }
}

pub(crate) fn query_view(view: &QueryView) -> Lines {
    let mut lines = Lines::default();
    match view {
        QueryView::Check(report) => check(&mut lines, report),
        QueryView::Format(report) => format(&mut lines, report),
        QueryView::Module(module) => self::module(&mut lines, module),
        QueryView::Tree(listing) => tree(&mut lines, listing),
        QueryView::Content(content) => stored(&mut lines, content),
        QueryView::State(info) => state(&mut lines, info),
        QueryView::StateCheck(check) => state_check(&mut lines, check),
        QueryView::Gc(preview) => gc(&mut lines, preview),
        QueryView::Run(run) => run_view(&mut lines, run),
        QueryView::Selection(selection) => selection_view(&mut lines, selection),
        QueryView::ActionPlan(plan) => action_plan(&mut lines, plan),
        QueryView::Dependencies(dependencies) => dependencies_view(&mut lines, dependencies),
    }
    lines
}

fn run_view(lines: &mut Lines, run: &RunView) {
    let source = match run.source {
        EvaluationSourceRepr::Computed => "computed",
        EvaluationSourceRepr::Reused => "reused",
        EvaluationSourceRepr::Hydrated => "hydrated",
    };
    lines
        .line(0)
        .styled(palette::HEADING, run.entry.as_ref())
        .plain(" ")
        .styled(palette::MUTED, source);
    lines.line(1).plain(value_text(&run.value));
}

fn selection_view(lines: &mut Lines, selection: &SelectionView) {
    let tier = selection.tier.as_str();
    lines
        .line(0)
        .styled(palette::HEADING, selection.entry.as_ref())
        .plain(" -> ")
        .styled(palette::LITERAL, selection.rule.as_ref());
    lines
        .line(1)
        .styled(palette::REUSE, tier)
        .plain(" ")
        .plain(selection.interface.rendered.as_ref());
}

fn action_plan(lines: &mut Lines, plan: &ActionPlanView) {
    lines
        .line(0)
        .styled(palette::HEADING, plan.entry.as_ref())
        .plain(" -> ")
        .styled(palette::LITERAL, plan.rule.as_ref());
    lines
        .line(1)
        .plain("contract ")
        .styled(palette::MUTED, short_digest(&plan.spec_digest));
    let executable = match &plan.contract.executable {
        ActionProgramRepr::HostPath { path } => path.as_ref(),
        ActionProgramRepr::Content { digest } => digest.as_ref(),
    };
    lines
        .line(1)
        .plain("exec ")
        .styled(palette::LITERAL, executable);
    for argument in &plan.contract.arguments {
        lines.line(2).plain(argument.as_ref());
    }
    for input in &plan.contract.inputs {
        lines
            .line(1)
            .plain("input ")
            .styled(palette::LITERAL, input.path.as_ref());
    }
    for output in &plan.contract.outputs {
        lines
            .line(1)
            .plain("output ")
            .styled(palette::LITERAL, output.path.as_ref());
    }
}

fn dependencies_view(lines: &mut Lines, dependencies: &DependenciesView) {
    lines
        .line(0)
        .styled(palette::HEADING, dependencies.entry.as_ref());
    let Some(root) = dependencies.root.as_deref() else {
        lines.line(1).styled(palette::MUTED, "no recorded attempt");
        return;
    };
    dependency_node(lines, root, 1);
}

fn dependency_node(lines: &mut Lines, node: &DependencyNodeRepr, indent: usize) {
    let kind = match &node.dependency {
        DependencyKindRepr::Pure { .. } => "pure",
        DependencyKindRepr::Action => "action",
        DependencyKindRepr::Observation => "observation",
        DependencyKindRepr::Blob { .. } => "blob",
        DependencyKindRepr::Capability { .. } => "capability",
    };
    let line = lines
        .line(indent)
        .styled(palette::LITERAL, kind)
        .plain(" ")
        .plain(node.label.as_ref());
    if let Some(status) = node.status {
        let status = match status {
            AttemptStatusRepr::Pending => "pending",
            AttemptStatusRepr::Complete => "complete",
            AttemptStatusRepr::Failed => "failed",
            AttemptStatusRepr::Cancelled => "cancelled",
        };
        line.plain(" ").styled(palette::MUTED, status);
    }
    for child in &node.children {
        dependency_node(lines, child, indent.saturating_add(1));
    }
}

fn value_text(value: &ValueRepr) -> String {
    let mut out = String::new();
    write_value(&mut out, value);
    out
}

/// One pass over the value, writing into a single buffer: a nested rendering
/// that formats and joins per level re-copies every subtree at each depth.
fn write_value(out: &mut String, value: &ValueRepr) {
    match value {
        ValueRepr::Unit => out.push_str("()"),
        ValueRepr::Bool { b } => {
            out.push_str(if *b { "true" } else { "false" });
        }
        ValueRepr::Int { decimal } => out.push_str(decimal),
        ValueRepr::Text { s } => out.push_str(s),
        ValueRepr::Bytes { len } => {
            out.push_str("bytes(");
            out.push_str(&len.to_string());
            out.push(')');
        }
        ValueRepr::Blob { digest } => {
            out.push_str("blob(");
            out.push_str(digest);
            out.push(')');
        }
        ValueRepr::Nominal {
            name,
            representation,
        } => {
            out.push_str(name);
            out.push('(');
            write_value(out, representation);
            out.push(')');
        }
        ValueRepr::List { elements } => {
            out.push('[');
            for (index, element) in elements.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_value(out, element);
            }
            out.push(']');
        }
        ValueRepr::Record { fields } => {
            out.push('{');
            for (index, (name, field_value)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                out.push_str(name);
                out.push_str(": ");
                write_value(out, field_value);
            }
            out.push('}');
        }
        ValueRepr::Sum {
            name,
            constructor,
            payload,
        } => {
            out.push_str(name);
            out.push_str("::");
            out.push_str(constructor);
            if let Some(payload) = payload {
                out.push('(');
                write_value(out, payload);
                out.push(')');
            }
        }
    }
}

fn check(lines: &mut Lines, report: &CheckReport) {
    lines
        .line(0)
        .styled(palette::HEADING, report.module.as_ref())
        .plain(" ")
        .styled(palette::MUTED, report.path.as_ref());
    for diagnostic in &report.diagnostics {
        diagnostic_line(lines, diagnostic);
    }
    let verdict = lines.line(1);
    if report.errors > 0 {
        verdict.styled(palette::FAILURE, format!("{} error(s)", report.errors));
    } else {
        verdict.styled(palette::SUCCESS, "ok");
    }
    if report.warnings > 0 {
        verdict.plain(", ").styled(
            palette::ATTENTION,
            format!("{} warning(s)", report.warnings),
        );
    }
    if let Some(digest) = report.abi_digest.as_deref() {
        verdict.plain(", abi ").styled(palette::MUTED, digest);
    }
}

fn diagnostic_line(lines: &mut Lines, diagnostic: &DiagnosticRepr) {
    let (style, tag) = match diagnostic.severity {
        SeverityRepr::Error => (palette::FAILURE, "error"),
        SeverityRepr::Warning => (palette::ATTENTION, "warning"),
        SeverityRepr::Info => (palette::REUSE, "info"),
        SeverityRepr::Note => (palette::MUTED, "note"),
    };
    let line = lines.line(1);
    line.styled(style, tag)
        .styled(palette::MUTED, format!("[{}] ", diagnostic.code));
    if let (Some(label), Some(row), Some(column)) = (
        diagnostic.label.as_deref(),
        diagnostic.line,
        diagnostic.column,
    ) {
        line.styled(palette::MUTED, format!("{label}:{row}:{column}: "));
    }
    line.plain(diagnostic.message.as_ref());
}

/// One of the three outcomes formatting a module has, each in the color the
/// rest of the report already uses for that verdict.
fn format(lines: &mut Lines, report: &FmtReport) {
    lines
        .line(0)
        .styled(palette::HEADING, report.module.as_ref())
        .plain(" ")
        .styled(palette::MUTED, report.path.as_ref());
    let (style, verdict) = match report.status {
        FmtStatus::Unchanged => (palette::SUCCESS, "unchanged"),
        FmtStatus::Formatted => (palette::REUSE, "formatted"),
        FmtStatus::WouldFormat => (palette::ATTENTION, "would reformat"),
    };
    lines.line(1).styled(style, verdict);
}

fn module(lines: &mut Lines, view: &ModuleView) {
    lines
        .line(0)
        .styled(palette::HEADING, view.module.as_ref())
        .plain(" ")
        .styled(palette::MUTED, view.path.as_ref());
    lines
        .line(1)
        .styled(palette::MUTED, "abi ")
        .styled(palette::MUTED, short_digest(&view.abi_digest));
    if !view.imports.is_empty() {
        lines.line(0).styled(palette::HEADING, "imports");
        for import in &view.imports {
            lines
                .line(1)
                .styled(palette::LITERAL, import.module.as_ref())
                .plain(" ")
                .styled(palette::MUTED, short_digest(&import.abi_digest));
        }
    }
    if !view.declarations.is_empty() {
        lines.line(0).styled(palette::HEADING, "declarations");
        for declaration in &view.declarations {
            declaration_line(lines, declaration);
        }
    }
    if !view.rules.is_empty() {
        lines.line(0).styled(palette::HEADING, "rules");
        for rule in &view.rules {
            rule_line(lines, rule);
        }
    }
    if !view.entries.is_empty() {
        lines.line(0).styled(palette::HEADING, "entries");
        for entry in &view.entries {
            entry_line(lines, entry);
        }
    }
    for about in &view.about {
        lines.line(0).styled(palette::HEADING, "about");
        for (name, value) in &about.fields {
            let rendered = match value {
                AboutValueRepr::Text { text } => format!("\"{text}\""),
                AboutValueRepr::List { elements } => format!(
                    "[{}]",
                    elements
                        .iter()
                        .map(|element| format!("\"{element}\""))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            lines
                .line(1)
                .styled(palette::LITERAL, name.as_ref())
                .plain(": ")
                .plain(rendered);
        }
        if !about.documentation.is_empty() {
            documentation(lines, &about.documentation);
        }
    }
}

fn entry_line(lines: &mut Lines, entry: &EntryView) {
    lines
        .line(1)
        .styled(palette::LITERAL, "entry")
        .plain(" ")
        .styled(palette::HEADING, entry.name.as_ref())
        .plain(" = ")
        .styled(palette::REUSE, entry.tier.as_str());
    lines
        .line(2)
        .plain("-> ")
        .plain(type_text(&entry.interface.output));
    if !entry.documentation.is_empty() {
        documentation(lines, &entry.documentation);
    }
}

fn declaration_line(lines: &mut Lines, declaration: &DeclarationView) {
    let keyword = match declaration.body {
        DeclarationBodyRepr::Nominal { .. } => "nominal",
        DeclarationBodyRepr::Sum { .. } => "sum",
        DeclarationBodyRepr::Alias { .. } => "type",
    };
    let heading = lines
        .line(1)
        .styled(palette::LITERAL, keyword)
        .plain(" ")
        .styled(palette::HEADING, declaration.name.as_ref());
    match &declaration.body {
        // A nominal or alias body is one type; inline it when it fits and
        // break it otherwise, exactly as a rule input would break.
        DeclarationBodyRepr::Nominal {
            representation: body,
        }
        | DeclarationBodyRepr::Alias { target: body } => {
            let inline = type_text(body);
            if compact(&inline) {
                heading.plain(" = ").plain(inline);
            } else {
                heading.plain(" =");
                render_type(lines, 2, body);
            }
        }
        DeclarationBodyRepr::Sum { constructors } => {
            let inline = sum_inline(constructors);
            if compact(&inline) {
                heading.plain(" = ").plain(inline);
            } else {
                heading.plain(" =");
                sum_lines(lines, 2, constructors);
            }
        }
    }
    if !declaration.documentation.is_empty() {
        documentation(lines, &declaration.documentation);
    }
}

fn rule_line(lines: &mut Lines, rule: &RuleView) {
    let category = match rule.category {
        RuleCategoryRepr::Pure => "pure",
        RuleCategoryRepr::Action => "action",
    };

    let tier = rule.tier.as_str();
    lines
        .line(1)
        .styled(palette::LITERAL, format!("{category} rule"))
        .plain(" ")
        .styled(palette::HEADING, rule.label.as_ref())
        .plain(" = ")
        .styled(palette::REUSE, tier);
    for input in rule.interface.inputs.iter() {
        render_type(lines, 2, input);
    }
    let output_inline = type_text(&rule.interface.output);
    if compact(&output_inline) {
        lines.line(2).plain("-> ").plain(output_inline);
    } else {
        lines.line(2).plain("->");
        render_type(lines, 3, &rule.interface.output);
    }
    if !rule.documentation.is_empty() {
        documentation(lines, &rule.documentation);
    }
}

/// How many characters a type may occupy before the renderer breaks it into
/// one construct per line. Chosen so a sum like `Forge(Text) | LocalPath(Text)
/// | Registry(Text)` stays inline while a resolver constraint list does not.
const INLINE_LIMIT: usize = 56;

/// Digests identity content a person compares, not content they read. The
/// full digest stays on the JSON surface; the description keeps enough to
/// recognize and diff at a glance.
const DIGEST_CHARS: usize = 12;

fn short_digest(digest: &str) -> &str {
    digest.get(..DIGEST_CHARS).unwrap_or(digest)
}

fn compact(text: &str) -> bool {
    text.chars().count() <= INLINE_LIMIT
}

fn type_text(ty: &TypeRepr) -> String {
    match ty {
        TypeRepr::Unit => "Unit".into(),
        TypeRepr::Bool => "Bool".into(),
        TypeRepr::Int => "Int".into(),
        TypeRepr::Text => "Text".into(),
        TypeRepr::Bytes => "Bytes".into(),
        TypeRepr::Blob => "Blob".into(),
        TypeRepr::Nominal { name } => name.as_ref().into(),
        TypeRepr::Cut => "...".into(),
        TypeRepr::List { element } => format!("List<{}>", type_text(element)),
        TypeRepr::Record { fields } => record_inline(fields),
        TypeRepr::Sum { constructors, .. } => sum_inline(constructors),
    }
}

fn record_inline(fields: &[(Box<str>, TypeRepr)]) -> String {
    if fields.is_empty() {
        return "{}".into();
    }
    let rendered: Vec<String> = fields
        .iter()
        .map(|(name, ty)| format!("{name}: {}", type_text(ty)))
        .collect();
    format!("{{ {} }}", rendered.join(", "))
}

fn sum_inline(constructors: &[SumConstructorRepr]) -> String {
    constructors
        .iter()
        .map(|constructor| match &constructor.payload {
            None => constructor.name.as_ref().into(),
            Some(payload) => format!("{}({})", constructor.name, type_text(payload)),
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Render one type, inline when it fits [`INLINE_LIMIT`] and broken one
/// construct per line otherwise. Breaking is recursive: a long record field
/// or constructor payload breaks again rather than overflowing its line.
fn render_type(lines: &mut Lines, indent: usize, ty: &TypeRepr) {
    let inline = type_text(ty);
    if compact(&inline) {
        lines.line(indent).plain(inline);
        return;
    }
    match ty {
        TypeRepr::List { element } => {
            lines.line(indent).plain("List<");
            render_type(lines, indent.saturating_add(1), element);
            lines.line(indent).plain(">");
        }
        TypeRepr::Record { fields } => {
            lines.line(indent).plain("{");
            field_lines(lines, indent.saturating_add(1), fields);
            lines.line(indent).plain("}");
        }
        TypeRepr::Sum { constructors, .. } => sum_lines(lines, indent, constructors),
        _ => {
            lines.line(indent).plain(inline);
        }
    }
}

fn field_lines(lines: &mut Lines, indent: usize, fields: &[(Box<str>, TypeRepr)]) {
    for (name, ty) in fields {
        let inline = type_text(ty);
        if compact(&inline) {
            lines
                .line(indent)
                .plain(name.as_ref())
                .plain(": ")
                .plain(inline);
        } else {
            lines.line(indent).plain(name.as_ref()).plain(":");
            render_type(lines, indent.saturating_add(1), ty);
        }
    }
}

fn sum_lines(lines: &mut Lines, indent: usize, constructors: &[SumConstructorRepr]) {
    for constructor in constructors {
        let Some(payload) = constructor.payload.as_deref() else {
            lines
                .line(indent)
                .plain("| ")
                .plain(constructor.name.as_ref());
            continue;
        };
        let inline = type_text(payload);
        if compact(&inline) {
            lines
                .line(indent)
                .plain("| ")
                .plain(constructor.name.as_ref())
                .plain("(")
                .plain(inline)
                .plain(")");
        } else if let TypeRepr::Record { fields } = payload {
            lines
                .line(indent)
                .plain("| ")
                .plain(constructor.name.as_ref())
                .plain("(");
            field_lines(lines, indent.saturating_add(1), fields);
            lines.line(indent).plain(")");
        } else {
            lines
                .line(indent)
                .plain("| ")
                .plain(constructor.name.as_ref())
                .plain("(");
            render_type(lines, indent.saturating_add(1), payload);
            lines.line(indent).plain(")");
        }
    }
}

fn documentation(lines: &mut Lines, text: &str) {
    for paragraph in text.lines() {
        lines.line(2).styled(palette::MUTED, paragraph);
    }
}

fn tree(lines: &mut Lines, listing: &TreeListing) {
    lines
        .line(0)
        .styled(palette::HEADING, "tree")
        .plain(" ")
        .styled(palette::MUTED, listing.tree.as_ref());
    for entry in &listing.entries {
        let line = lines.line(1);
        match entry {
            TreeEntryRepr::File {
                name,
                content,
                executable,
            } => {
                line.styled(palette::MUTED, if *executable { "x" } else { "f" })
                    .plain(" ")
                    .styled(palette::LITERAL, name.as_ref())
                    .plain(" ")
                    .styled(palette::MUTED, content.as_ref());
            }
            TreeEntryRepr::Tree { name, content } => {
                line.styled(palette::MUTED, "d")
                    .plain(" ")
                    .styled(palette::LITERAL, format!("{name}/"))
                    .plain(" ")
                    .styled(palette::MUTED, content.as_ref());
            }
            TreeEntryRepr::Symlink { name, target } => {
                line.styled(palette::MUTED, "l")
                    .plain(" ")
                    .styled(palette::LITERAL, name.as_ref())
                    .plain(" -> ")
                    .plain(target.as_ref());
            }
        }
    }
}

fn stored(lines: &mut Lines, content: &StoredContent) {
    let kind = match content.kind {
        StoredContentKind::Blob => "blob",
        StoredContentKind::Tree => "tree",
    };
    let line = lines.line(0);
    line.styled(palette::LITERAL, kind)
        .plain(" ")
        .styled(palette::HEADING, content.id.as_ref());
    if let Some(path) = content.path.as_deref() {
        line.plain(" ").styled(palette::MUTED, path);
    }
}

fn state(lines: &mut Lines, info: &StateInfo) {
    lines
        .line(0)
        .styled(palette::HEADING, "state")
        .plain(" ")
        .styled(palette::LITERAL, info.adapter.as_ref())
        .plain(" ")
        .styled(palette::MUTED, format!("schema {}", info.schema_version))
        .plain(" ")
        .styled(
            palette::MUTED,
            format!("encoding {}", info.semantic_encoding_version),
        );
    let counts = &info.attempts;
    lines
        .line(1)
        .plain("attempts ")
        .styled(palette::LITERAL, format!("{}", counts.total))
        .plain(" (")
        .styled(palette::SUCCESS, format!("{} complete", counts.complete))
        .plain(", ")
        .styled(palette::MUTED, format!("{} failed", counts.failed))
        .plain(", ")
        .styled(palette::MUTED, format!("{} cancelled", counts.cancelled))
        .plain(", ")
        .styled(palette::MUTED, format!("{} pending", counts.pending))
        .plain(")");
    lines
        .line(1)
        .plain("reusable index ")
        .styled(palette::REUSE, format!("{}", info.reusable_index));
}

fn state_check(lines: &mut Lines, check: &StateCheck) {
    lines
        .line(0)
        .styled(palette::HEADING, "state")
        .plain(" ")
        .styled(palette::SUCCESS, "ok");
    lines
        .line(1)
        .plain("decoded ")
        .styled(palette::LITERAL, format!("{}", check.records))
        .plain(" record(s)");
}

fn gc(lines: &mut Lines, preview: &GcPreview) {
    lines
        .line(0)
        .styled(palette::HEADING, "gc dry run")
        .plain(" ")
        .styled(palette::MUTED, "roots are the reusable index");
    lines
        .line(1)
        .plain("roots ")
        .styled(palette::REUSE, format!("{}", preview.roots))
        .plain(", retained attempts ")
        .styled(palette::REUSE, format!("{}", preview.retained_attempts))
        .plain(", reclaimable ")
        .styled(
            palette::ATTENTION,
            format!("{}", preview.reclaimable_attempts),
        );
    let content = &preview.content;
    lines
        .line(1)
        .plain("content ")
        .styled(palette::LITERAL, format!("{} blob(s)", content.blobs))
        .plain(" ")
        .styled(palette::LITERAL, format!("{} tree(s)", content.trees))
        .plain(", live ")
        .styled(
            palette::REUSE,
            format!("{} / {}", content.live_blobs, content.live_trees),
        )
        .plain(", reclaimable ")
        .styled(
            palette::ATTENTION,
            format!(
                "{} / {}",
                content.reclaimable_blobs, content.reclaimable_trees
            ),
        );
    lines
        .line(1)
        .plain("bytes ")
        .styled(palette::LITERAL, format!("{}", content.total_bytes))
        .plain(" total, ")
        .styled(palette::REUSE, format!("{}", content.live_bytes))
        .plain(" live, ")
        .styled(palette::ATTENTION, format!("{}", content.reclaimable_bytes))
        .plain(" reclaimable");
}
