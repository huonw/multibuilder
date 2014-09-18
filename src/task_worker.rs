use std::io::process::ProcessOutput;
use std::{task, str, comm};

use std::sync::Arc;

use Command;
use build::{BuildInstruction, BuildResult};
use build;
use git::Repo;

pub struct TaskWorker {
    pub stream: (Sender<BuildInstruction>, Receiver<BuildResult>)
}

impl Drop for TaskWorker {
    fn drop(&mut self) {
        error!("TaskWorker dropped!")
    }
}

impl TaskWorker {
    pub fn send(&self, bi: BuildInstruction) {
        self.stream.ref0().send(bi)
    }

    pub fn recv_opt(&self) -> Result<BuildResult, ()> {
        self.stream.ref1().recv_opt()
    }

    /// Create a new TaskWorker, which does builds in build_dir,
    /// cloning from `canonical_repo`.
    pub fn new(build_dir: Path,
               canonical_repo: Arc<Repo>,
               build_commands: Arc<Vec<Command>>) -> TaskWorker {
        let (outer_tx, rx) = comm::channel();
        let (tx, outer_rx) = comm::channel();
        let ret = TaskWorker {
            stream: (outer_tx, outer_rx)
        };

        task::spawn(proc() {
            loop {
                let instr = match rx.recv_opt() {
                    Ok(instr) => instr,
                    Err(()) => {
                        debug!("main task hung up? bailing");
                        break
                    }
                };

                let result = match instr {
                    build::BuildHash(hash) => {
                        println!("Building {}", hash.value)

                        // foo/bar/0088119922aa33bb...77ff
                        let hash_dir = build_dir.join(hash.value.as_slice());
                        let subrepo = canonical_repo.new_subrepo(hash_dir);
                        subrepo.checkout(hash.value.as_slice());

                        if run_build(&subrepo, build_commands.as_slice()) {
                            build::Success(build::Local(subrepo.path), hash)
                        } else {
                            build::Failure(hash)
                        }
                    }
                };

                debug!("Finished a built with {}", result);
                // finished this build.
                tx.send(result)
            }
        });

        ret
    }
}

fn run_build(repo: &Repo, commands: &[Command]) -> bool {
    for command in commands.iter() {
        let ProcessOutput { status, output, error } =
            repo.exec(command.name.as_slice(), command.args.as_slice());
        debug!("status success: {}", status.success());
        if !status.success() {
            warn!("run_build {} {} failed with {}: {} {}",
                   command.name,
                   command.args,
                   status,
                   str::from_utf8(output.as_slice()),
                   str::from_utf8(error.as_slice()));

            return false;
        }
    }
    true
}
