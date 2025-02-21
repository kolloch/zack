use anyhow::Result;
use loader::Loader;
use starlark::eval::FileLoader;

fn main() -> Result<()> {
    rules::copy_built_in_rules()?;

    let loader = Loader::default();

    let core = loader
        .load("@core/core.star")
        .map_err(starlark::Error::into_anyhow)?;
    println!("{:?}", core.documentation().docs);

    Ok(())
}
