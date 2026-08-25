//! The items a module is made of: imports, declarations, rules with their
//! written bodies, module locals, entries, and `about` blocks.

use pith_hir::{
    RuleCategory, SurfaceAbout, SurfaceAboutValue, SurfaceBinder, SurfaceBody, SurfaceConstructor,
    SurfaceDeclaration, SurfaceEntry, SurfaceImport, SurfaceLocal, SurfaceParam, SurfaceRule,
    SurfaceRuleBody, SurfaceStatement, SurfaceTypeId, SurfaceWrittenBody,
};

use super::Printer;

impl<'a> Printer<'a> {
    pub(super) fn import(&mut self, import: &SurfaceImport) {
        self.out.push_str("import ");
        self.name(&import.module);
    }

    pub(super) fn declaration(&mut self, declaration: &SurfaceDeclaration) {
        match &declaration.body {
            SurfaceBody::Nominal(representation) => {
                self.flat_declaration("nominal", declaration, *representation);
            }
            SurfaceBody::Alias(target) => {
                self.flat_declaration("type", declaration, *target);
            }
            SurfaceBody::Sum(constructors) => self.sum(declaration, constructors),
        }
    }

    fn flat_declaration(
        &mut self,
        keyword: &str,
        declaration: &SurfaceDeclaration,
        body: SurfaceTypeId,
    ) {
        self.out.push_str(keyword);
        self.out.push(' ');
        self.name(&declaration.name);
        self.out.push_str(" = ");
        self.type_node(body);
    }

    /// One constructor shares the declaration's line; more than one takes a
    /// line each, behind a pipe, at one indent.
    fn sum(&mut self, declaration: &SurfaceDeclaration, constructors: &[SurfaceConstructor]) {
        self.out.push_str("sum ");
        self.name(&declaration.name);
        match constructors {
            [only] => {
                self.out.push_str(" = ");
                self.constructor(only);
            }
            many => {
                self.out.push_str(" =");
                self.indent = self.indent.saturating_add(1);
                for constructor in many {
                    self.newline();
                    self.out.push_str("| ");
                    self.constructor(constructor);
                }
                self.indent = self.indent.saturating_sub(1);
            }
        }
    }

    fn constructor(&mut self, constructor: &SurfaceConstructor) {
        self.name(&constructor.name);
        if let Some(payload) = constructor.payload {
            self.out.push('(');
            self.type_node(payload);
            self.out.push(')');
        }
    }

    pub(super) fn rule(&mut self, rule: &SurfaceRule) {
        let category = match rule.category {
            RuleCategory::Pure => "pure",
            RuleCategory::Action => "action",
        };
        self.out.push_str(category);
        self.out.push_str(" rule ");
        self.name(&rule.label);
        self.out.push('(');
        self.joined(rule.params.as_ref(), ", ", Self::param);
        self.out.push_str(") -> ");
        self.type_node(rule.output);
        self.out.push_str(" =");
        match &rule.body {
            SurfaceRuleBody::Host => self.out.push_str(" host"),
            SurfaceRuleBody::Written(body) => self.written_body(body),
        }
    }

    fn param(&mut self, param: &SurfaceParam) {
        if let Some((name, _)) = &param.name {
            self.name(name);
            self.out.push_str(": ");
        }
        self.type_node(param.payload);
    }

    fn written_body(&mut self, body: &SurfaceWrittenBody) {
        self.open_block();
        for statement in &body.statements {
            self.newline();
            self.statement(statement);
        }
        if let Some(tail) = &body.tail {
            self.newline();
            self.value(tail);
        }
        self.close_block();
    }

    fn statement(&mut self, statement: &SurfaceStatement) {
        self.out.push_str("let ");
        self.binder(&statement.binder);
        if let Some(annotation) = statement.annotation {
            self.out.push_str(" : ");
            self.type_node(annotation);
        }
        self.out.push_str(" = ");
        self.value(&statement.value);
    }

    fn binder(&mut self, binder: &SurfaceBinder) {
        match binder {
            SurfaceBinder::Name { name, .. } => self.name(name),
            SurfaceBinder::Group { names, .. } => {
                self.out.push('(');
                self.joined(names.as_ref(), ", ", Self::binder);
                self.out.push(')');
            }
        }
    }

    pub(super) fn local(&mut self, local: &SurfaceLocal) {
        self.out.push_str("let ");
        self.name(&local.name);
        self.out.push_str(" : ");
        self.type_node(local.annotation);
        self.out.push_str(" = ");
        self.value(&local.value);
    }

    pub(super) fn entry(&mut self, entry: &SurfaceEntry) {
        self.out.push_str("entry ");
        self.name(&entry.name);
        self.out.push_str(" : ");
        self.type_node(entry.output);
        self.out.push_str(" = ");
        self.request(&entry.request);
    }

    /// About fields take a line each and keep their comma, which is the
    /// block's own spelling in the surface notation.
    pub(super) fn about(&mut self, about: &SurfaceAbout) {
        self.out.push_str("about");
        self.open_block();
        for (key, value) in &about.fields {
            self.newline();
            self.name(key);
            self.out.push_str(": ");
            self.about_value(value);
            self.out.push(',');
        }
        self.close_block();
    }

    fn about_value(&mut self, value: &SurfaceAboutValue) {
        match value {
            SurfaceAboutValue::Text(text) => self.quoted(text),
            SurfaceAboutValue::List(items) => {
                self.out.push('[');
                self.joined(items.as_ref(), ", ", |printer, item| printer.quoted(item));
                self.out.push(']');
            }
        }
    }
}
