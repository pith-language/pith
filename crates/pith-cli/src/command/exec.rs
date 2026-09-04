use std::os::unix::process::CommandExt;
use std::process::Command;

use super::Context;
use super::entry::EntryTarget;
use crate::exit::Failure;

#[derive(clap::Args)]
pub struct Exec {
    #[command(flatten)]
    target: EntryTarget,
}

impl Exec {
    /// Replace this process with the entry's derived program. Success never
    /// returns; the value is the failure to report when it cannot.
    pub fn replace(self, context: &mut Context) -> Failure {
        let (module, entry) = self.target.parts();
        let invocation = match context.query_writable(|session| session.prepare_exec(module, entry))
        {
            Ok(invocation) => invocation,
            Err(failure) => return failure,
        };
        let mut command = Command::new(invocation.program.as_ref());
        command.args(invocation.arguments.iter().map(AsRef::as_ref));
        let error = command.exec();
        Failure::user(format!("cannot exec the derived program: {error}"))
    }
}
