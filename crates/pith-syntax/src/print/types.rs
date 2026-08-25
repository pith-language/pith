//! The type grammar: scalars, lists, records, and qualified references.

use core::range::Range;

use pith_hir::{ParsedSurface, SurfaceField, SurfaceTypeId, SurfaceTypeNode};

use super::Printer;

impl<'a> Printer<'a> {
    pub(super) fn type_node(&mut self, id: SurfaceTypeId) {
        if let Some(node) = self.surface.types.get(id) {
            self.type_node_kind(node);
        }
    }

    fn type_node_kind(&mut self, node: &SurfaceTypeNode) {
        match node {
            SurfaceTypeNode::Unit => self.out.push_str("Unit"),
            SurfaceTypeNode::Bool => self.out.push_str("Bool"),
            SurfaceTypeNode::Int => self.out.push_str("Int"),
            SurfaceTypeNode::Text => self.out.push_str("Text"),
            SurfaceTypeNode::Bytes => self.out.push_str("Bytes"),
            SurfaceTypeNode::Blob => self.out.push_str("Blob"),
            SurfaceTypeNode::List(element) => {
                self.out.push_str("List<");
                self.type_node(*element);
                self.out.push('>');
            }
            SurfaceTypeNode::Record { fields } => {
                self.out.push('{');
                self.joined(record_fields(self.surface, fields), ", ", Self::type_field);
                self.out.push('}');
            }
            SurfaceTypeNode::Reference { module, name, .. } => {
                if let Some(module) = module {
                    self.name(module);
                    self.out.push('.');
                }
                self.name(name);
            }
        }
    }

    fn type_field(&mut self, field: &SurfaceField) {
        self.name(&field.name);
        self.out.push_str(": ");
        self.type_node(field.payload);
    }
}

/// The record's fields, when the range points inside the side table.
fn record_fields<'surface>(
    surface: &'surface ParsedSurface,
    range: &Range<u32>,
) -> &'surface [SurfaceField] {
    let start = usize::try_from(range.start).unwrap_or(0);
    let end = usize::try_from(range.end).unwrap_or(0);
    surface.fields.get(start..end).unwrap_or(&[])
}
