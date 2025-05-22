use std::cell::RefCell;

use allocative::Allocative;
use serde::{Deserialize, Serialize};
use starlark::{any::ProvidesStaticType};


#[derive(Debug, ProvidesStaticType, Allocative)]
pub struct BuildContext {
    pub commands: Vec<Command>,
}

#[derive(Debug, Allocative)]
pub struct Command {
    pub args: Vec<String>,
}