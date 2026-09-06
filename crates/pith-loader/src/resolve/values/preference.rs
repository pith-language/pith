//! The preference codec: a nominal over the written name.

use pith_core::{Type, Value};
use pith_diag::PithResult;

use super::super::model::{Preference, UnknownPreference};
use super::{decode_error, nominal_type, nominal_value, text_of};

/// The preference type: a nominal over its written name.
#[must_use]
pub fn preference_type() -> Type {
    nominal_type("Preference", Type::Text)
}

#[must_use]
pub fn preference_value(preference: Preference) -> Value {
    nominal_value("Preference", Value::Text(preference.name().into()))
}

/// Decodes a preference from `value`.
///
/// # Errors
/// Returns a diagnostic when `value` is not a written preference name.
pub fn preference_from_value(value: &Value) -> PithResult<Preference> {
    let Value::Nominal { representation, .. } = value else {
        return Err(decode_error("a modules.Preference", value));
    };
    let spelling = text_of(representation, "Preference")?;
    Preference::from_name(&spelling).map_err(|UnknownPreference { name }| {
        decode_error(&format!("a written preference, not `{name}`"), value)
    })
}
