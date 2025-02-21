use anyhow::Context;
use dupe::{Dupe, OptionDupedExt};
use starlark::environment::{FrozenModule, Globals, Module};
use starlark::eval::{Evaluator, FileLoader};
use starlark::syntax::{AstModule, Dialect, DialectTypes};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum LoadError {
    #[error("Module '{module_name}' not found.")]
    ModuleNotFound { module_name: String },
}

#[derive(Debug, Clone)]
pub struct Loader {
    globals: Globals,
    loaded: Arc<RwLock<HashMap<String, FrozenModule>>>,
}

impl Default for Loader {
    fn default() -> Self {
        Loader {
            globals: Globals::standard(),
            loaded: Default::default(),
        }
    }
}

const DIALECT: Dialect = Dialect {
    enable_types: DialectTypes::Enable,
    enable_load_reexport: false,
    enable_f_strings: true,
    enable_keyword_only_arguments: true,
    enable_top_level_stmt: true,
    ..Dialect::Standard
};

impl FileLoader for Loader {
    fn load(&self, module_name: &str) -> Result<FrozenModule, starlark::Error> {
        let loaded = self.loaded.read().expect("to get lock");
        if let Some(existing) = loaded.get(module_name).duped() {
            return Ok(existing);
        }
        drop(loaded);

        let file_name = store::rules_dir().join(module_name);
        let content = std::fs::read_to_string(file_name.as_str())
            .with_context(|| format!("while reading from {file_name:?}"))?;
        let parsed = AstModule::parse(file_name.as_str(), content, &DIALECT)?;
        let module = Module::new();
        let mut eval = Evaluator::new(&module);
        eval.eval_module(parsed, &self.globals)?;
        drop(eval);
        let frozen = module.freeze().map_err(starlark::Error::from)?;

        // This allows parallel loading of the same module which could be wasteful.
        let mut loaded = self.loaded.write().expect("to get lock");
        if let Some(existing) = loaded.get(module_name).duped() {
            return Ok(existing);
        }
        loaded.insert(module_name.to_string(), frozen.dupe());
        Ok(frozen)
    }
}
