use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum LoadError {
    #[error("Module '{module_name}' not found.")]
    ModuleNotFound { module_name: String }
}
