//! The canonical spelling of a parsed manifest. The contract is the surface
//! printer's: printing is a pure function of the manifest, the printed text
//! re-parses to the same manifest, and formatting needs no dependency
//! resolution.

use pith_diag::{SourceFile, Span};
use pith_hir::{
    DependencySource, ManifestDomain, ManifestModule, ManifestRegistry, ManifestUse,
    ManifestWorkspace, ParsedManifest, SurfaceComment,
};

use super::layout::{INDENT, Spacing, slice};
use super::names::quoted;

/// The canonical text of `manifest`, whose comments are sliced from
/// `source`.
#[must_use]
pub fn print_manifest(manifest: &ParsedManifest, source: &SourceFile) -> String {
    let mut printer = ManifestPrinter {
        manifest,
        text: source.source_text(),
        out: String::new(),
        spacing: Spacing::new(),
    };
    printer.document();
    printer.out
}

struct ManifestPrinter<'a> {
    manifest: &'a ParsedManifest,
    text: &'a str,
    out: String,
    spacing: Spacing,
}

enum ManifestEvent<'a> {
    Comment(&'a SurfaceComment),
    Module(&'a ManifestModule),
    Workspace(&'a ManifestWorkspace),
    Registry(&'a ManifestRegistry),
    Domain(&'a ManifestDomain),
    Use(&'a ManifestUse),
}

impl ManifestEvent<'_> {
    fn start(&self) -> u32 {
        match self {
            Self::Comment(comment) => comment.span.start.0,
            Self::Module(module) => module.span.start.0,
            Self::Workspace(workspace) => workspace.span.start.0,
            Self::Registry(registry) => registry.span.start.0,
            Self::Domain(domain) => domain.span.start.0,
            Self::Use(use_) => use_.span.start.0,
        }
    }
}

impl<'a> ManifestPrinter<'a> {
    /// Clauses and comments remain in source order, as with module items:
    /// a comment stays attached to the clause it describes.
    fn document(&mut self) {
        let mut events = Vec::new();
        events.extend(self.manifest.comments.iter().map(ManifestEvent::Comment));
        if let Some(module) = self.manifest.module.as_ref() {
            events.push(ManifestEvent::Module(module));
        }
        if let Some(workspace) = self.manifest.workspace.as_ref() {
            events.push(ManifestEvent::Workspace(workspace));
        }
        events.extend(self.manifest.registries.iter().map(ManifestEvent::Registry));
        events.extend(self.manifest.domains.iter().map(ManifestEvent::Domain));
        events.extend(self.manifest.uses.iter().map(ManifestEvent::Use));
        events.sort_by_key(ManifestEvent::start);

        for event in events {
            match event {
                ManifestEvent::Comment(comment) if comment.trailing => {
                    self.trailing_comment(comment.span)
                }
                ManifestEvent::Comment(comment) => self.leading_comment(comment.span),
                ManifestEvent::Module(module) => {
                    self.spacing.begin_item(&mut self.out);
                    self.module(module);
                }
                ManifestEvent::Workspace(workspace) => {
                    self.spacing.begin_item(&mut self.out);
                    self.workspace(workspace);
                }
                ManifestEvent::Registry(registry) => {
                    self.spacing.begin_item(&mut self.out);
                    self.registry(registry);
                }
                ManifestEvent::Domain(domain) => {
                    self.spacing.begin_item(&mut self.out);
                    self.domain(domain);
                }
                ManifestEvent::Use(use_) => {
                    self.spacing.begin_item(&mut self.out);
                    self.use_(use_);
                }
            }
        }
        self.spacing.finish(&mut self.out);
    }

    fn module(&mut self, module: &ManifestModule) {
        self.out.push_str("module ");
        self.out.push_str(&module.subject.to_string());
        self.out.push(' ');
        self.out.push_str(&module.version.to_string());
    }

    fn workspace(&mut self, workspace: &ManifestWorkspace) {
        self.out.push_str("workspace {\n");
        self.out.push_str(INDENT);
        self.out.push_str("members: [");
        for (index, member) in workspace.members.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            quoted(&mut self.out, &member.path);
        }
        self.out.push_str("],\n}");
    }

    fn use_(&mut self, use_: &ManifestUse) {
        self.out.push_str("use ");
        self.out.push_str(&use_.alias);
        self.out.push_str(" = ");
        self.out.push_str(&use_.subject.to_string());
        if !use_.range.is_any() {
            self.out.push(' ');
            self.out.push_str(&use_.range.to_string());
        }
        self.source(&use_.source);
    }

    /// A route's canonical spelling. The default registry route writes
    /// nothing: `from registry` with no name says only what omitting it
    /// already says.
    fn source(&mut self, source: &DependencySource) {
        match source {
            DependencySource::Registry { registry: None, .. } => {}
            DependencySource::Registry {
                registry: Some(name),
                ..
            } => {
                self.out.push_str(" from registry ");
                self.out.push_str(name);
            }
            DependencySource::Path { path, .. } => {
                self.out.push_str(" from path ");
                quoted(&mut self.out, path);
            }
            DependencySource::Git {
                url,
                revision,
                subpath,
                ..
            } => {
                self.out.push_str(" from git ");
                quoted(&mut self.out, url);
                self.out.push_str(" revision ");
                quoted(&mut self.out, revision);
                if let Some(subpath) = subpath {
                    self.out.push_str(" subpath ");
                    quoted(&mut self.out, subpath);
                }
            }
            DependencySource::Archive { url, digest, .. } => {
                self.out.push_str(" from archive ");
                quoted(&mut self.out, url);
                self.out.push_str(" digest ");
                quoted(&mut self.out, digest);
            }
        }
    }

    fn registry(&mut self, registry: &ManifestRegistry) {
        self.out.push_str("registry ");
        self.out.push_str(&registry.name);
        self.out.push_str(" = ");
        quoted(&mut self.out, &registry.locator);
        self.out.push_str(" root ");
        quoted(&mut self.out, &registry.root_key.to_string());
    }

    fn domain(&mut self, domain: &ManifestDomain) {
        self.out.push_str("domain ");
        self.out.push_str(domain.domain.as_str());
        self.out.push_str(" from ");
        self.out.push_str(&domain.registry);
    }

    fn leading_comment(&mut self, span: Span) {
        self.spacing.begin_leading_comment(&mut self.out);
        Spacing::comment(&mut self.out, slice(self.text, span));
    }

    fn trailing_comment(&mut self, span: Span) {
        if !self.spacing.trailing_comment(&mut self.out) {
            self.leading_comment(span);
            return;
        }
        Spacing::comment(&mut self.out, slice(self.text, span));
    }
}

#[cfg(test)]
mod tests {
    use super::print_manifest;
    use crate::parse_manifest;
    use pith_diag::{SourceFile, SourceId};
    use std::sync::Arc;

    /// The canonical text of `text`, which must parse without a diagnostic.
    fn canonical(text: &str) -> String {
        let source = Arc::new(SourceFile::new(
            SourceId::from_raw(0),
            "module.pi",
            text.trim_start_matches('\n'),
        ));
        let (manifest, diagnostics) = parse_manifest(&source);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        print_manifest(&manifest, &source)
    }

    #[test]
    fn the_canonical_spelling_is_flat() {
        assert_eq!(
            canonical(
                "\nmodule   example/hello   0.1.0\n\n\nuse   greeting=example/greeting from \
                 path \"modules/greeting\"\n"
            ),
            "module example/hello 0.1.0\n\nuse greeting = example/greeting from path \
             \"modules/greeting\"\n"
        );
    }

    #[test]
    fn clauses_keep_source_order_and_comments_stay_attached() {
        assert_eq!(
            canonical(
                "-- the greeter\nmodule example/hello 0.1.0 -- trailing\n\nuse greeting = \
                 example/greeting from path \"modules/greeting\"\n"
            ),
            "-- the greeter\nmodule example/hello 0.1.0  -- trailing\n\nuse greeting = \
             example/greeting from path \"modules/greeting\"\n"
        );
    }

    #[test]
    fn one_member_takes_the_single_line_and_many_take_one_each() {
        assert_eq!(
            canonical("module a/b 0.1.0\n\nworkspace { members: [ \"one\" ] }"),
            "module a/b 0.1.0\n\nworkspace {\n  members: [\"one\"],\n}\n"
        );
        assert_eq!(
            canonical("module a/b 0.1.0\n\nworkspace { members: [ \"one\" , \"two\" , ] }"),
            "module a/b 0.1.0\n\nworkspace {\n  members: [\"one\", \"two\"],\n}\n"
        );
    }

    #[test]
    fn hyphenated_segments_and_paths_keep_their_spelling() {
        assert_eq!(
            canonical(
                "module my-org/my-module 1.2.3\n\nuse g = my-org/dep-a from path \"../a-b\"\n"
            ),
            "module my-org/my-module 1.2.3\n\nuse g = my-org/dep-a from path \"../a-b\"\n"
        );
    }

    #[test]
    fn versions_reprint_without_leading_zeros() {
        assert_eq!(canonical("module a/b 01.002.3\n"), "module a/b 1.2.3\n");
    }

    #[test]
    fn printing_is_idempotent_over_every_canonical_text_above() {
        let samples = [
            "module example/hello 0.1.0\n",
            "module a/b 0.1.0\n\nworkspace {\n  members: [\"one\"],\n}\n",
            "module a/b 0.1.0\n\nuse g = a/dep from path \"../dep\"\n",
        ];
        for sample in samples {
            let once = canonical(sample);
            assert_eq!(&once, sample, "the sample was not canonical");
            assert_eq!(canonical(&once), once, "printing printed text moved it");
        }
    }
}
