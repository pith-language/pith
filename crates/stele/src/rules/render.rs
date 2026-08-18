//! The three text projections: a unit file, a passwd table, and a boot
//! loader entry. Rendering is pure and deterministic, the written lock's
//! discipline (0041): the projection is a function of its declared inputs,
//! and writing any of them to a machine is a caller effect that belongs to
//! the activation half (M-5b).

use pith_core::{BodyRevision, Pure, Rule, Value};
use pith_diag::{PithResult, Span};
use pith_engine::{PureRule, PureRuleFrame};

use crate::rules::{
    Leaf, diag, field_of, representation_of, text_of, unit_parts_of, user_entries_of,
};
use crate::types::{self, MODULE};

/// Render a merged unit as the text systemd's main unit file format spells.
pub struct RenderUnit;

impl RenderUnit {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::declared(
            MODULE,
            types::RENDER_UNIT,
            BodyRevision(1),
            types::render_unit_interface(),
            Span::none(),
        )
    }

    fn compute(inputs: &[Value]) -> PithResult<Value> {
        let Some(unit) = inputs.first() else {
            return Err(diag(&format!(
                "a render-unit request supplies its unit; this one supplied {}",
                inputs.len()
            )));
        };
        let parts = unit_parts_of(unit)?;
        let mut text = String::new();
        text.push_str("[Unit]\n");
        text.push_str(&format!("Description={}\n", parts.description));
        if !parts.after.is_empty() {
            text.push_str(&format!("After={}\n", parts.after.join(" ")));
        }
        if !parts.wants.is_empty() {
            text.push_str(&format!("Wants={}\n", parts.wants.join(" ")));
        }
        text.push_str("\n[Service]\n");
        text.push_str(&format!("ExecStart={}\n", parts.exec));
        Ok(types::unit_text().value(Value::Text(text.into())))
    }
}

/// Render a merged user table as passwd lines, in the name order the value
/// already carries.
pub struct RenderPasswd;

impl RenderPasswd {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::declared(
            MODULE,
            types::RENDER_PASSWD,
            BodyRevision(1),
            types::render_passwd_interface(),
            Span::none(),
        )
    }

    fn compute(inputs: &[Value]) -> PithResult<Value> {
        let Some(table) = inputs.first() else {
            return Err(diag(&format!(
                "a render-passwd request supplies its table; this one supplied {}",
                inputs.len()
            )));
        };
        let entries = user_entries_of(table)?;
        let mut text = String::new();
        for (name, record) in entries {
            let uid = id_of(&record, types::UID)?;
            let gid = id_of(&record, types::GID)?;
            let home = field_of(&record, types::HOME);
            let shell = field_of(&record, types::SHELL);
            let (Some(home), Some(shell)) = (home, shell) else {
                return Err(diag(&format!(
                    "the account `{name}` is missing its home or its shell"
                )));
            };
            text.push_str(&format!(
                "{name}:x:{uid}:{gid}::{home}:{shell}\n",
                home = text_of(home)?,
                shell = text_of(shell)?,
            ));
        }
        Ok(types::passwd_text().value(Value::Text(text.into())))
    }
}

/// Render a boot description as a loader entry line set, the Boot Loader
/// Specification's key-value shape.
pub struct RenderBoot;

impl RenderBoot {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::declared(
            MODULE,
            types::RENDER_BOOT,
            BodyRevision(1),
            types::render_boot_interface(),
            Span::none(),
        )
    }

    fn compute(inputs: &[Value]) -> PithResult<Value> {
        let Some(boot) = inputs.first() else {
            return Err(diag(&format!(
                "a render-boot request supplies its boot description; this one supplied {}",
                inputs.len()
            )));
        };
        let record = representation_of(boot, types::boot())?;
        let machine = field_of(record, types::MACHINE);
        let kernel = field_of(record, types::KERNEL);
        let initrd = field_of(record, types::INITRD);
        let (Some(machine), Some(kernel), Some(initrd)) = (machine, kernel, initrd) else {
            return Err(diag(
                "a boot description was missing its machine, kernel, or initrd",
            ));
        };
        let text = format!(
            "title {machine}\nlinux {kernel}\ninitrd {initrd}\n",
            machine = text_of(machine)?,
            kernel = text_of(kernel)?,
            initrd = text_of(initrd)?,
        );
        Ok(types::boot_text().value(Value::Text(text.into())))
    }
}

/// The machine-width id a user field carries, refusing a uid the host cannot
/// spell.
fn id_of(record: &Value, field: &str) -> PithResult<i64> {
    let Some(Value::Int(value)) = field_of(record, field) else {
        return Err(diag("a user account's ids were missing or not integers"));
    };
    value.to_i64().ok_or_else(|| {
        diag(&format!(
            "the `{field}` of a user account is outside the machine range"
        ))
    })
}

impl PureRule for RenderUnit {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(Leaf::new(inputs, Self::compute))
    }
}

impl PureRule for RenderPasswd {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(Leaf::new(inputs, Self::compute))
    }
}

impl PureRule for RenderBoot {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(Leaf::new(inputs, Self::compute))
    }
}
