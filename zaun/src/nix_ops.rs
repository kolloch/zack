use std::os::fd::RawFd;

use crate::{named::Namer, ops::ZaunOp};
use anyhow::Result;
use derive_more::Display;
use nix::{fcntl::OFlag, sys::stat::Mode};

#[derive(Debug, Clone, Display)]
#[display("NixOpen({path}, oflag: {oflag:?}, mode: {mode:?})")]
pub(crate) struct NixOpen {
    path: String,
    flags: OFlag,
    mode: Mode,
}

impl ZaunOp<RawFd> for NixOpen {
    fn execute(&self, _namer: &mut Namer) -> Result<RawFd> {
        Ok(nix::fcntl::open(self.path.as_str(), self.oflag, self.mode)?)
    }
}
