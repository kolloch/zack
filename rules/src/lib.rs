use ahash::AHashMap;
use anyhow::Context;
use camino::Utf8PathBuf;
use dupe::Dupe;
use include_dir::{Dir, include_dir};
use starlark::environment::{FrozenModule, Globals, Module};
use starlark::eval::{Evaluator, FileLoader};
use starlark::syntax::{AstModule, Dialect};
use std::collections::btree_map::Entry;
use std::str::FromStr;
use std::sync::RwLock;

static STARLARK: Dir = include_dir!("$CARGO_MANIFEST_DIR/starlark");

struct BuiltinLoader {
    modules: RwLock<AHashMap<Utf8PathBuf, FrozenModule>>,
}

impl FileLoader for BuiltinLoader {
    fn load(&self, path: &str) -> starlark::Result<FrozenModule> {
        let modules = self.modules.read()
            .context("while trying to get a lock on built-in modules")?;
        if let Some(module) = modules.get(&path) {
            return Ok(module.dupe());
        }
        drop(modules);

        let mut modules =
            self.modules.write().context("while getting cache lock")?;
        let mut entry = modules.entry(Utf8PathBuf::from_str(path).unwrap());
        if let Entry::Occupied(module) = entry {
            let module: &FrozenModule = module.get();
            return Ok(module.dupe());
        }

        let Some(code) = STARLARK.get_file(path) else {
            return Err(loader::LoadError::ModuleNotFound {
                module_name: path.to_owned(),
            }
            .into());
        };

        let globals = Globals::standard();
        let ast = AstModule::parse(
            path,
            code.contents_utf8()
                .expect("starlark code to be utf8")
                .to_owned(),
            &Dialect::Standard,
        )?;

        let module: Module = Module::new();
        let mut eval: Evaluator = Evaluator::new(&module);

        let _ = eval.eval_module(ast, &globals)?;
        Ok(module.freeze()?)
    }
}
