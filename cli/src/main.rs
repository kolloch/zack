use anyhow::Result;
use loader::Loader;
use starlark::environment::GlobalsBuilder;
use starlark::eval::FileLoader;
use starlark::starlark_module;

#[starlark_module]
fn starlark_quadratic(builder: &mut GlobalsBuilder) {
    fn quadratic(a: i32, b: i32, c: i32, x: i32) -> anyhow::Result<i32> {
        Ok(a * x * x + b * x + c)
    }
}

fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_line_number(true)
        .with_file(true)
        .init();

    rules::copy_built_in_rules()?;

    let loader = Loader::default();

    let core = loader
        .load("@core/other.star")
        .map_err(starlark::Error::into_anyhow)?;
    println!("{:?}", core.documentation().docs);

    Ok(())
}
