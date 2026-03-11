use std::os::fd::RawFd;

use crate::{named::Namer, ops::ZaunOp};
use anyhow::Result;
use derive_more::Display;
use nix::{fcntl::OFlag, sys::stat::Mode};

#[derive(Debug, Clone, Display)]
#[display("NixOpen({path}, flags: {flags:?}, mode: {mode:?})")]
pub(crate) struct NixOpen {
    pub(crate) path: String,
    pub(crate) flags: OFlag,
    pub(crate) mode: Mode,
}

impl ZaunOp<RawFd> for NixOpen {
    fn execute(&self, _namer: &mut Namer) -> Result<RawFd> {
        Ok(nix::fcntl::open(self.path.as_str(), self.flags, self.mode)?)
    }
}
