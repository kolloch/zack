use anyhow::{Context, Result};
use directories::workspace_dir;
use loader::{Executor, Loader};
use starlark::environment::GlobalsBuilder;
use starlark::starlark_module;
use std::fs::read_to_string;

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
    let executor = Executor::default();

    let file_path = workspace_dir().join("ZACK.star");
    executor
        .execute(
            &loader,
            &file_path,
            read_to_string(&file_path).with_context(|| format!("while reading {file_path:?}"))?,
        )
        .map_err(|e| e.into_anyhow())
        .with_context(|| format!("while executing {file_path:?}"))?;

    Ok(())
}
