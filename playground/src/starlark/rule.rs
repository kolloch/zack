use allocative::Allocative;
use derive_more::Display;
use serde::Serialize;
use starlark::{
    any::ProvidesStaticType,
    starlark_simple_value,
    values::{StarlarkAttrs, StarlarkValue, starlark_attrs, starlark_value},
};

#[derive(
    Debug,
    Clone,
    StarlarkAttrs,
    PartialEq,
    Eq,
    Hash,
    Display,
    ProvidesStaticType,
    Serialize,
    Allocative,
)]
#[display("{:?}", self)]
pub struct Rule {
    pub target: String,
    pub implementation: String,
}
starlark_simple_value!(Rule);

#[starlark_value(type = "rule")]
impl<'v> StarlarkValue<'v> for Rule {
    starlark_attrs!();
}
