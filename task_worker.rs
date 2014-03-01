use std::{task, str};
use std::io::process::ProcessOutput;
use std::comm::{Empty, Disconnected, Data};
use sync::DuplexStream;
use sync::Arc;

use Command;
use git::Repo;
use build;
use build::{BuildInstruction, BuildResult};

pub struct TaskWorker {
    stream: DuplexStream<BuildInstruction, BuildResult>
}

impl TaskWorker {
    /// Create a new TaskWorker, which does builds in build_dir,
    /// cloning from `canonical_repo`.
    pub fn new(build_dir: Path,
               canonical_repo: Arc<Repo>,
               build_commands: Arc<~[Command]>) -> TaskWorker {
        let (outside, inside) = DuplexStream::new();
        let ret = TaskWorker {
            stream: outside
        };

        task::spawn(proc() {
            loop {
                let instr = match inside.try_recv() {
                    Empty | Disconnected => break, // finished
                    Data(instr) => instr
                };

                let result = match instr {
                    build::BuildHash(hash) => {
                        println!("Building {}", hash.value)

                        // foo/bar/0088119922aa33bb...77ff
                        let hash_dir = build_dir.join(hash.value.as_slice());
                        let subrepo = canonical_repo.get().new_subrepo(hash_dir);
                        subrepo.checkout(hash.value);

                        if run_build(&subrepo, build_commands.get().as_slice()) {
                            build::Success(build::Local(subrepo.path), hash)
                        } else {
                            build::Failure(hash)
                        }
                    }
                };

                // finished this build.
                inside.send(result)
            }
        });

        ret
    }
}

fn run_build(repo: &Repo, commands: &[Command]) -> bool {
    for command in commands.iter() {
        let ProcessOutput { status, output, error } =
            repo.exec(command.name, command.args);
        debug!("status success: {}", status.success());
        if !status.success() {
            warn!("run_build {} {:?} failed with {}: {} {}",
                   command.name,
                   command.args,
                   status,
                   str::from_utf8(output),
                   str::from_utf8(error));

            return false;
        }
    }
    true
}
