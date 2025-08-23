use std::cell::RefCell;

use playground::starlark::{extended_globals, rule::Rule};
use starlark::{
    StarlarkResultExt,
    any::ProvidesStaticType,
    environment::{GlobalsBuilder, Module},
    eval::Evaluator,
    starlark_module,
    values::{
        Value, ValueLike,
        none::NoneType,
        typing::{StarlarkCallable, StarlarkCallableParamAny, StarlarkCallableParamSpec},
    },
};

use playground::ast;

#[derive(Debug, ProvidesStaticType, Default)]
struct Store(RefCell<Vec<Rule>>);

impl Store {
    fn add(&self, x: Rule) {
        self.0.borrow_mut().push(x)
    }
}

#[starlark_module]
fn starlark_rule(builder: &mut GlobalsBuilder) {
    // fn rule(target: &str, implementation: Value, eval: &mut Evaluator) -> anyhow::Result<NoneType> {
    //     let func = implementation
    //         .downcast_ref::<StarlarkCallable<'static>>()
    //         .ok_or_else(|| anyhow::anyhow!("Expected a callable Starlark function"))?;

    //     let r = Rule {
    //         target: target.to_string(),
    //         implementation: implementation.to_string(),
    //     };

    //     eval.extra.unwrap().downcast_ref::<Store>().unwrap().add(r);

    //     Ok(NoneType)
    // }
}

fn main() -> anyhow::Result<()> {
    let globals = extended_globals().with(starlark_rule).build();
    let ast = ast!(
        "rule.star",
        r#"
        def impl():
            print("Hello, World!")

        rule("foo", impl)
        rule("bar", "echo bar")
        "#
    )?;

    let module = Module::new();
    let store = Store::default();
    {
        let mut eval = Evaluator::new(&module);
        eval.extra = Some(&store);

        eval.eval_module(ast, &globals).into_anyhow_result()?;
    }

    println!("Store {:?}", store.0.borrow());

    Ok(())
}
