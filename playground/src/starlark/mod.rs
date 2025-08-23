pub mod rule;

use anyhow::Result;
pub use indoc::indoc;
use starlark::{
    StarlarkResultExt,
    environment::{GlobalsBuilder, LibraryExtension},
    syntax::{AstModule, Dialect},
};

pub fn extended_globals() -> GlobalsBuilder {
    GlobalsBuilder::extended_by(&[
        // LibraryExtension::Breakpoint,
        LibraryExtension::Debug,
        LibraryExtension::CallStack,
        LibraryExtension::EnumType,
        LibraryExtension::Filter,
        LibraryExtension::Json,
        LibraryExtension::Map,
        LibraryExtension::NamespaceType,
        LibraryExtension::Partial,
        LibraryExtension::Pprint,
        LibraryExtension::Prepr,
        LibraryExtension::Print,
        LibraryExtension::Pstr,
        LibraryExtension::RecordType,
        LibraryExtension::SetType,
        LibraryExtension::StructType,
        LibraryExtension::Typing,
    ])
}

pub fn ast_module(file_name: &str, code: &str) -> Result<AstModule> {
    AstModule::parse(file_name, code.to_owned(), &Dialect::Extended).into_anyhow_result()
}

#[macro_export]
macro_rules! ast {
    ($file_name:literal, $code:literal) => {
        $crate::starlark::ast_module($file_name, $crate::starlark::indoc! {$code});
    };
}
