use anyhow::Result;
use derive_more::Display;
use std::{
    collections::HashSet,
    fmt::Display,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use crate::{
    named::{Named, Namer},
    subid::{IdMapMatcher, IdRange},
};

trait ZaunOp<R: Display + Clone>: Display {
    fn execute(&self, namer: &mut Namer) -> Result<R>;
}

#[derive(Display)]
#[display("{op}\n  => {result}")]
struct Executed<'o> {
    op: &'o dyn Display,
    result: Box<dyn Display>,
}

#[derive(Default)]
struct ZaunExec<'o> {
    namer: Namer,
    ops: Vec<Executed<'o>>,
}

/// Executes the operations, giving prior executions as context if there is an error.
impl<'o> ZaunExec<'o> {
    fn execute<R: 'static + Display + Clone>(&mut self, op: &'o dyn ZaunOp<R>) -> Result<R> {
        let r = op.execute(&mut self.namer);
        match r {
            Ok(res) => {
                let result = Box::new(res.clone());
                self.ops.push(Executed { op, result });
                Ok(res)
            }
            Err(e) => Err(anyhow::anyhow!(
                "Error executing operation: {}\nAfter:\n{self}",
                e,
            )),
        }
    }
}

impl<'o> Display for ZaunExec<'o> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for executed in &self.ops {
            writeln!(f, "{executed}")?;
        }
        Ok(())
    }
}

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
pub(crate) struct GetUidRange<'m>(&'m Named<IdMapMatcher>, u32);

impl<'m> ZaunOp<IdRange> for GetUidRange<'m> {
    fn execute(&self, _namer: &mut Namer) -> Result<IdRange> {
        Ok(self.0.get_matching_uid_map(self.1)?)
    }
}

#[derive(Display, Clone)]
#[display("GetGidRange({_0})")]
pub(crate) struct GetGidRange<'m>(&'m Named<IdMapMatcher>, u32);

impl<'m> ZaunOp<IdRange> for GetGidRange<'m> {
    fn execute(&self, _namer: &mut Namer) -> Result<IdRange> {
        Ok(self.0.get_matching_gid_map(self.1)?)
    }
}

#[cfg(test)]
mod tests {
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
