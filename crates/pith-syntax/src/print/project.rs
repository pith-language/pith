//! The header clauses a project file opens with: its subject, its inputs
//! block, its host component, its workspace membership, and its includes.

use pith_hir::{
    DocumentHeader, InputLocator, InputsBlock, ProjectHost, ProjectInclude, ProjectModule,
    ProjectWorkspace,
};

use super::ModuleEvent;
use super::Printer;

impl<'a> Printer<'a> {
    /// The header clauses of the surface, when it carries any: the events
    /// the module printer interleaves with items and comments by span.
    pub(super) fn header_events(&self) -> Vec<ModuleEvent<'a>> {
        let DocumentHeader::Project(header) = &self.surface.header else {
            return Vec::new();
        };
        let mut events = vec![ModuleEvent::Inputs(&header.inputs)];
        if let Some(module) = header.module.as_ref() {
            events.push(ModuleEvent::Module(module));
        }
        if let Some(host) = header.host.as_ref() {
            events.push(ModuleEvent::Host(host));
        }
        if let Some(workspace) = header.workspace.as_ref() {
            events.push(ModuleEvent::Workspace(workspace));
        }
        events.extend(header.includes.iter().map(ModuleEvent::Include));
        events
    }

    pub(super) fn module_clause(&mut self, module: &ProjectModule) {
        self.out.push_str("module ");
        self.out.push_str(&module.subject.to_string());
        self.out.push(' ');
        self.out.push_str(&module.version.to_string());
    }

    /// The inputs block prints its entries in the order they were written,
    /// one per line, each keeping its comma.
    pub(super) fn inputs(&mut self, block: &InputsBlock) {
        enum Entry<'a> {
            Input(&'a pith_hir::ProjectInput),
            Registry(&'a pith_hir::ProjectRegistry),
            Domain(&'a pith_hir::ProjectDomain),
        }
        let mut entries = Vec::new();
        entries.extend(block.inputs.iter().map(Entry::Input));
        entries.extend(block.registries.iter().map(Entry::Registry));
        entries.extend(block.domains.iter().map(Entry::Domain));
        let start = |entry: &Entry<'_>| match entry {
            Entry::Input(input) => input.span.start.0,
            Entry::Registry(registry) => registry.span.start.0,
            Entry::Domain(domain) => domain.span.start.0,
        };
        entries.sort_by_key(start);
        self.out.push_str("inputs");
        self.open_block();
        for entry in entries {
            self.newline();
            match entry {
                Entry::Input(input) => self.input(input),
                Entry::Registry(registry) => self.registry(registry),
                Entry::Domain(domain) => self.domain(domain),
            }
            self.out.push(',');
        }
        self.close_block();
    }

    fn input(&mut self, input: &pith_hir::ProjectInput) {
        self.out.push_str(&input.name);
        self.out.push_str(" = ");
        self.locator(&input.locator);
    }

    /// A locator's canonical spelling. The default registry route writes
    /// nothing after the subject and its range.
    fn locator(&mut self, locator: &InputLocator) {
        match locator {
            InputLocator::Path { path, .. } => {
                self.out.push_str("path ");
                self.quoted(path);
            }
            InputLocator::Git {
                url,
                revision,
                subpath,
                ..
            } => {
                self.out.push_str("git ");
                self.quoted(url);
                self.out.push_str(" at ");
                self.quoted(revision);
                if let Some(subpath) = subpath {
                    self.out.push_str(" subpath ");
                    self.quoted(subpath);
                }
            }
            InputLocator::Archive { url, digest, .. } => {
                self.out.push_str("archive ");
                self.quoted(url);
                self.out.push_str(" digest ");
                self.quoted(digest);
            }
            InputLocator::Registry {
                subject,
                range,
                registry,
                ..
            } => {
                self.out.push_str(&subject.to_string());
                if !range.is_any() {
                    self.out.push(' ');
                    self.out.push_str(&range.to_string());
                }
                if let Some(name) = registry {
                    self.out.push_str(" from registry ");
                    self.out.push_str(name);
                }
            }
        }
    }

    pub(super) fn host_clause(&mut self, host: &ProjectHost) {
        self.out.push_str("host ");
        self.out.push_str(&host.adapter);
        self.out.push(' ');
        self.out.push_str(&host.artifact);
    }

    pub(super) fn workspace_clause(&mut self, workspace: &ProjectWorkspace) {
        self.out.push_str("workspace");
        self.open_block();
        self.newline();
        self.out.push_str("members: [");
        for (index, member) in workspace.members.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            self.quoted(&member.path);
        }
        self.out.push_str("],");
        self.close_block();
    }

    pub(super) fn include(&mut self, include: &ProjectInclude) {
        self.out.push_str("include ");
        self.quoted(&include.path);
    }

    fn registry(&mut self, registry: &pith_hir::ProjectRegistry) {
        self.out.push_str("registry ");
        self.out.push_str(&registry.name);
        self.out.push_str(" = ");
        self.quoted(&registry.locator);
        self.out.push_str(" root ");
        self.quoted(&registry.root_key.to_string());
    }

    fn domain(&mut self, domain: &pith_hir::ProjectDomain) {
        self.out.push_str("domain ");
        self.out.push_str(domain.domain.as_str());
        self.out.push_str(" from ");
        self.out.push_str(&domain.registry);
    }
}
