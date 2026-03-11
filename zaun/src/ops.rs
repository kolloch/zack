use anyhow::Result;
use derive_more::Display;
use std::fmt::{Display, Formatter};

use crate::named::Namer;

pub(crate) trait ZaunOp<R: OpResult + Clone>: Display {
    fn execute(&self, namer: &mut Namer) -> Result<R>;
}

struct Executed<'o> {
    op: &'o dyn Display,
    result: Box<dyn OpResult>,
}

impl Display for Executed<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{} => ", self.op)?;
        self.result.fmt(f)
    }
}

#[derive(Default)]
pub(crate) struct ZaunExec<'o> {
    namer: Namer,
    ops: Vec<Executed<'o>>,
}

trait OpResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error>;
}

impl<R: Display + Clone> OpResult for R {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        Display::fmt(self, f)
    }
}

#[derive(Clone)]
pub(crate) struct Void;

impl OpResult for Void {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "()")
    }
}

/// Executes the operations, giving prior executions as context if there is an error.
impl<'o> ZaunExec<'o> {
    pub(crate) fn execute<R: 'static + OpResult + Clone>(
        &mut self,
        op: &'o dyn ZaunOp<R>,
    ) -> Result<R> {
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
