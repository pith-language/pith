//! The expression grammar: literals, names, records, lists, the control
//! forms, and the binary operators under the precedence the parser's three
//! tiers fix.

use pith_core::Value;
use pith_hir::{SurfaceArm, SurfaceExpr, SurfaceExprId, SurfaceOperator, SurfaceValueField};

use super::Printer;

use std::fmt::Write as _;

/// Whether a record literal may appear where the expression is printed. The
/// parser threads `allow_record` down its expression chain, and an
/// expression the chain refused needs its parentheses back to re-parse.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Records {
    Allowed,
    Barred,
}

/// The binding powers the parser's three binary tiers fix, loosest to
/// tightest. A binary node prints in parentheses when it binds more loosely
/// than its position requires.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Level {
    Equality,
    Additive,
    Multiplication,
    Atom,
}

impl<'a> Printer<'a> {
    pub(super) fn expression(&mut self, id: SurfaceExprId, minimum: Level, records: Records) {
        let Some(expr) = self.surface.exprs.get(id) else {
            return;
        };
        if self.is_parenthesized(expr, minimum, records) {
            self.out.push('(');
            self.spelling(expr, records);
            self.out.push(')');
        } else {
            self.spelling(expr, records);
        }
    }

    /// Parentheses go exactly where the parser would have needed them to
    /// build this tree: around a binary node looser than its position
    /// demands, and around a record literal in a position the parser
    /// refuses one.
    fn is_parenthesized(&self, expr: &SurfaceExpr, minimum: Level, records: Records) -> bool {
        match expr {
            SurfaceExpr::Binary { operator, .. } => level_of(*operator) < minimum,
            SurfaceExpr::Record { .. } => records == Records::Barred,
            _ => false,
        }
    }

    fn spelling(&mut self, expr: &SurfaceExpr, records: Records) {
        match expr {
            SurfaceExpr::Literal { value, .. } => self.literal(value),
            SurfaceExpr::Name { name, .. } => self.name(name),
            SurfaceExpr::Field { record, name, .. } => {
                self.expression(*record, Level::Atom, records);
                self.out.push('.');
                self.name(name);
            }
            SurfaceExpr::Record { fields, .. } => self.record(fields.as_ref()),
            SurfaceExpr::List { items, .. } => self.list(items.as_ref()),
            SurfaceExpr::Construct {
                name, arguments, ..
            } => {
                self.name(name);
                self.arguments(arguments.as_ref());
            }
            SurfaceExpr::Unwrap { value, .. } => {
                self.out.push_str("unwrap ");
                self.expression(*value, Level::Atom, Records::Allowed);
            }
            SurfaceExpr::If {
                condition,
                then,
                otherwise,
                ..
            } => self.if_expression(*condition, *then, *otherwise),
            SurfaceExpr::Match {
                scrutinee, arms, ..
            } => self.match_expression(*scrutinee, arms),
            SurfaceExpr::Fold {
                source,
                init,
                element,
                accumulator,
                step,
                ..
            } => self.fold_expression(*source, *init, element, accumulator, *step),
            SurfaceExpr::Binary {
                operator,
                left,
                right,
                ..
            } => {
                let level = level_of(*operator);
                self.expression(*left, level, records);
                self.out.push(' ');
                self.out.push_str(operator_spelling(*operator));
                self.out.push(' ');
                self.expression(*right, tighter(level), records);
            }
        }
    }

    fn literal(&mut self, value: &Value) {
        match value {
            Value::Unit => self.out.push_str("()"),
            Value::Bool(b) => self.out.push_str(if *b { "true" } else { "false" }),
            Value::Int(n) => {
                let _ = write!(self.out, "{n}");
            }
            Value::Text(t) => self.quoted(t),
            other => unreachable!(
                "a parsed literal is unit, bool, int, or text: {}",
                other.describe()
            ),
        }
    }

    fn record(&mut self, fields: &[SurfaceValueField]) {
        self.out.push('{');
        self.joined(fields, ", ", Self::record_field);
        self.out.push('}');
    }

    fn record_field(&mut self, field: &SurfaceValueField) {
        self.name(&field.name);
        self.out.push_str(": ");
        self.expression(field.value, Level::Equality, Records::Allowed);
    }

    fn list(&mut self, items: &[SurfaceExprId]) {
        self.out.push('[');
        self.joined(items, ", ", Self::argument);
        self.out.push(']');
    }

    /// `else` chains into a nested `if` on one line, which is the only
    /// place the printer joins two keywords.
    fn if_expression(
        &mut self,
        condition: SurfaceExprId,
        then: SurfaceExprId,
        otherwise: SurfaceExprId,
    ) {
        self.out.push_str("if ");
        self.expression(condition, Level::Equality, Records::Barred);
        self.open_block();
        self.body_line(then);
        self.close_block();
        self.out.push_str(" else");
        match self.surface.exprs.get(otherwise) {
            Some(SurfaceExpr::If {
                condition,
                then,
                otherwise,
                ..
            }) => {
                self.out.push(' ');
                self.if_expression(*condition, *then, *otherwise);
            }
            _ => {
                self.open_block();
                self.body_line(otherwise);
                self.close_block();
            }
        }
    }

    fn match_expression(&mut self, scrutinee: SurfaceExprId, arms: &[SurfaceArm]) {
        self.out.push_str("match ");
        self.expression(scrutinee, Level::Equality, Records::Barred);
        self.open_block();
        for arm in arms {
            self.newline();
            self.name(&arm.constructor);
            if let Some(binder) = &arm.binder {
                self.out.push('(');
                self.name(binder);
                self.out.push(')');
            }
            self.open_block();
            self.body_line(arm.body);
            self.close_block();
        }
        self.close_block();
    }

    fn fold_expression(
        &mut self,
        source: SurfaceExprId,
        init: SurfaceExprId,
        element: &str,
        accumulator: &str,
        step: SurfaceExprId,
    ) {
        self.out.push_str("fold ");
        self.expression(source, Level::Equality, Records::Allowed);
        self.out.push_str(" from ");
        self.expression(init, Level::Equality, Records::Allowed);
        self.open_block();
        self.newline();
        self.out.push('(');
        self.name(element);
        self.out.push_str(", ");
        self.name(accumulator);
        self.out.push_str(") -> ");
        self.expression(step, Level::Equality, Records::Allowed);
        self.close_block();
    }

    /// A block's single line, at the block's own indentation.
    fn body_line(&mut self, id: SurfaceExprId) {
        self.newline();
        self.expression(id, Level::Equality, Records::Allowed);
    }
}

fn level_of(operator: SurfaceOperator) -> Level {
    match operator {
        SurfaceOperator::Equal | SurfaceOperator::NotEqual => Level::Equality,
        SurfaceOperator::IntAdd | SurfaceOperator::IntSubtract => Level::Additive,
        SurfaceOperator::IntMultiply => Level::Multiplication,
    }
}

/// The binding power above `level`, which is what a left-associative
/// operator's right operand prints under.
fn tighter(level: Level) -> Level {
    match level {
        Level::Equality => Level::Additive,
        Level::Additive => Level::Multiplication,
        Level::Multiplication | Level::Atom => Level::Atom,
    }
}

fn operator_spelling(operator: SurfaceOperator) -> &'static str {
    match operator {
        SurfaceOperator::Equal => "==",
        SurfaceOperator::NotEqual => "!=",
        SurfaceOperator::IntAdd => "+",
        SurfaceOperator::IntSubtract => "-",
        SurfaceOperator::IntMultiply => "*",
    }
}
