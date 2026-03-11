use std::rc::Rc;

use anyhow::Result;
use derive_more::Display;

use crate::{
    named::{Named, Namer},
    ops::{Void, ZaunOp},
    subid::{IdMapMatcher, IdRange},
};

#[derive(Display, Clone)]
#[display("GetIdMapMatcher")]
pub(crate) struct GetIdMapMatcher;

impl ZaunOp<Rc<Named<IdMapMatcher>>> for GetIdMapMatcher {
    fn execute(&self, namer: &mut Namer) -> Result<Rc<Named<IdMapMatcher>>> {
        Ok(Rc::new(namer.named(
            "$id_matcher",
            IdMapMatcher::new_for_current_user()?,
        )))
    }
}

#[derive(Display, Clone)]
#[display("GetUidRange({_0}, count={_1})")]
pub(crate) struct GetUidRange<'m>(pub &'m Named<IdMapMatcher>, pub u32);

impl<'m> ZaunOp<IdRange> for GetUidRange<'m> {
    fn execute(&self, _namer: &mut Namer) -> Result<IdRange> {
        Ok(self.0.get_matching_uid_map(self.1)?)
    }
}

#[derive(Display, Clone)]
#[display("GetGidRange({_0})")]
pub(crate) struct GetGidRange<'m>(pub &'m Named<IdMapMatcher>, pub u32);

impl<'m> ZaunOp<IdRange> for GetGidRange<'m> {
    fn execute(&self, _namer: &mut Namer) -> Result<IdRange> {
        Ok(self.0.get_matching_gid_map(self.1)?)
    }
}

#[derive(Display, Clone)]
pub(crate) enum IdMapCommand {
    NewUidMap,
    NewGidMap,
}

impl From<&IdMapCommand> for &'static str {
    fn from(cmd: &IdMapCommand) -> Self {
        match cmd {
            IdMapCommand::NewUidMap => "newuidmap",
            IdMapCommand::NewGidMap => "newgidmap",
        }
    }
}

#[derive(Display, Clone)]
#[display("NewIdMapCommand({command}, {range}, {pid})")]
pub(crate) struct NewIdMapCommand<'a> {
    pub command: IdMapCommand,
    pub range: &'a IdRange,
    pub pid: u32,
}

impl<'a> ZaunOp<Void> for NewIdMapCommand<'a> {
    fn execute(&self, _namer: &mut Namer) -> Result<Void> {
        self.range.call_newidmap((&self.command).into(), self.pid)?;
        Ok(Void)
    }
}
#[cfg(test)]
mod tests {
    use crate::{
        identity::ops::{GetGidRange, GetIdMapMatcher, GetUidRange},
        ops::ZaunExec,
    };

    use super::*;
    #[test]
    fn test_zaun_exec() {
        let mut exec = ZaunExec::default();
        let get_id_map_matcher = GetIdMapMatcher;
        let id_map_matcher = exec.execute(&get_id_map_matcher).unwrap();
        let get_uid_range = GetUidRange(&id_map_matcher, 100);
        let uid_range = exec.execute(&get_uid_range).unwrap();
        let get_gid_range = GetGidRange(&id_map_matcher, 100);
        let gid_range = exec.execute(&get_gid_range).unwrap();
        println!("Uid Range: {:?}", uid_range);
        println!("Gid Range: {:?}", gid_range);
    }
}
