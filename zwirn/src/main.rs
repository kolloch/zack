extern crate starlark;
use std::cell::RefCell;

use allocative::Allocative;
use anyhow::anyhow;
use derive_more::Display;
use indoc::indoc;
use serde::Serialize;
use starlark::any::ProvidesStaticType;
use starlark::environment::{Globals, GlobalsBuilder, LibraryExtension, Module};
use starlark::eval::Evaluator;
use starlark::syntax::{AstModule, Dialect};
use starlark::values::none::NoneType;
use starlark::values::type_repr::StarlarkTypeRepr;
use starlark::values::{
    Demand, NoSerialize, StarlarkAttrs, StarlarkValue, UnpackValue, Value, starlark_attrs,
    starlark_value,
};
use starlark::{StarlarkResultExt, starlark_module, starlark_simple_value};
use thiserror::Error;

// This defines the function that is visible to Starlark
#[starlark_module]
fn starlark_quadratic(builder: &mut GlobalsBuilder) {
    fn quadratic(a: i32, b: i32, c: i32, x: i32) -> anyhow::Result<i32> {
        Ok(a * x * x + b * x + c)
    }
}

// #[derive(Debug, PartialEq, Eq, Allocative)]
// struct Complex {
//     real: i32,
//     imaginary: i32,
// }
// starlark_simple_value!(Complex);

trait SomeTrait {
    fn payload(&self) -> u32;
}

#[derive(ProvidesStaticType, derive_more::Display, Debug, NoSerialize, Allocative)]
#[display("SomeType")]
struct MyValue {
    payload: u32,
}

unsafe impl<'v> ProvidesStaticType<'v> for &'v dyn SomeTrait {
    type StaticType = &'static dyn SomeTrait;
}

impl SomeTrait for MyValue {
    fn payload(&self) -> u32 {
        self.payload
    }
}

starlark_simple_value!(MyValue);

#[starlark_value(type = "MyValue")]
impl<'v> StarlarkValue<'v> for MyValue {
    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value::<&dyn SomeTrait>(self);
    }
}

#[derive(
    Debug,
    StarlarkAttrs,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Display,
    ProvidesStaticType,
    NoSerialize,
    Allocative,
)]
#[display("{:?}", self)]
struct Example {
    hello: String,
    #[starlark(skip)]
    answer: i64,
    #[starlark(clone)]
    nested: Nested,
    r#type: i64,
    r#escaped: String,
}
starlark_simple_value!(Example);

#[starlark_value(type = "example")]
impl<'v> StarlarkValue<'v> for Example {
    starlark_attrs!();
}

#[derive(
    Debug,
    Clone,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    Hash,
    StarlarkAttrs,
    Display,
    ProvidesStaticType,
    NoSerialize,
    Allocative,
)]
#[display("{}", foo)]
struct Nested {
    foo: String,
}
starlark_simple_value!(Nested);

#[starlark_value(type = "nested")]
impl<'v> StarlarkValue<'v> for Nested {
    starlark_attrs!();
}

#[derive(
    Debug, StarlarkAttrs, PartialEq, Eq, Hash, Display, ProvidesStaticType, Serialize, Allocative,
)]
#[display("{:?}", self)]
struct Smaller {
    hello: String,
    r#escaped: String,
}
starlark_simple_value!(Smaller);

#[starlark_value(type = "smaller")]
impl<'v> StarlarkValue<'v> for Smaller {
    starlark_attrs!();
}

#[derive(StarlarkTypeRepr, UnpackValue, Eq, PartialEq, Debug)]
enum IntOrStr {
    // Int(i32),
    Str(String),
    Example(Example),
}

impl UnpackValue<'_> for Example {
    type Error = anyhow::Error;

    fn unpack_value_impl(value: Value<'_>) -> Result<Option<Self>, Self::Error> {
        Ok(value.request_value::<Example>())
    }
}

// #[derive(Debug, ProvidesStaticType, Allocative)]
// struct SomeStruct {
//     field1: String,
//     field2: i32,
// }
// starlark_simple_value!(SomeStruct);

#[derive(Debug, ProvidesStaticType, Default)]
struct Store(RefCell<Vec<IntOrStr>>);

impl Store {
    fn add(&self, x: IntOrStr) {
        self.0.borrow_mut().push(x)
    }
}

#[starlark_module]
fn starlark_emit(builder: &mut GlobalsBuilder) {
    fn emit(x: IntOrStr, eval: &mut Evaluator) -> anyhow::Result<NoneType> {
        // We modify extra (which we know is a Store) and add the JSON of the
        // value the user gave.

        eval.extra.unwrap().downcast_ref::<Store>().unwrap().add(x);
        Ok(NoneType)
    }
}

fn main() -> anyhow::Result<()> {
    // We build our globals to make the function available in Starlark
    let mut globals =
        GlobalsBuilder::extended_by(&[LibraryExtension::Print, LibraryExtension::Json])
            .with(starlark_quadratic)
            .with(starlark_emit);

    globals.set(
        "smaller",
        Smaller {
            hello: "world".to_string(),
            escaped: "escaped".to_string(),
        },
    );

    let globals = globals.build();

    // Let's test calling the function from Starlark code
    let starlark_code = indoc! {r#"
        print(smaller);
        print(json.encode(smaller))
        emit("asd")
        emit("asd")
        quadratic(4, 2, 1, x = 8)
    "#};

    let ast = AstModule::parse(
        "quadratic.star",
        starlark_code.to_owned(),
        &Dialect::Extended,
    )
    .into_anyhow_result()?;

    let module = Module::new();
    let store = Store::default();
    {
        let mut eval = Evaluator::new(&module);

        eval.extra = Some(&store);
        let res = eval.eval_module(ast, &globals).into_anyhow_result()?;
        assert_eq!(res.unpack_i32(), Some(273)); // Verify that we got an `int` return value of 4 * 8^2 + 2 * 8 + 1 = 273
    }

    println!("Store {:?}", store.0.borrow());

    Ok(())
}
