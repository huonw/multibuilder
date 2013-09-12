use std::{task, run, str};
use extra::comm::DuplexStream;
use extra::arc::Arc;

use git::Repo;
use build;
use build::{BuildInstruction, BuildResult};

pub struct TaskWorker {
    stream: DuplexStream<BuildInstruction, BuildResult>
}

impl TaskWorker {
    /// Create a new TaskWorker, which does builds in build_dir,
    /// cloning from `canonical_repo`.
    pub fn new(build_dir: Path, canonical_repo: Arc<Repo>) -> TaskWorker {
        let (outside, inside) = DuplexStream();
        let ret = TaskWorker {
            stream: outside
        };

        do task::spawn_sched(task::SingleThreaded) {
            loop {
                let instr = match inside.try_recv() {
                    None => break, // finished
                    Some(instr) => instr
                };

                let result = match instr {
                    build::BuildHash(hash) => {
                        println!("Building {}", hash.value)

                        // foo/bar/0088119922aa33bb...77ff
                        let hash_dir = build_dir.push(hash.value);
                        let subrepo = canonical_repo.get().new_subrepo(hash_dir);
                        subrepo.checkout(hash.value);

                        if run_build(&subrepo) {
                            build::Success(build::Local(subrepo.path), hash)
                        } else {
                            build::Failure(hash)
                        }
                    }
                };

                // finished this build.
                inside.send(result)
            }
        }

        ret
    }
}

fn run_build(repo: &Repo) -> bool {
    let run::ProcessOutput { status, output, error } = repo.exec("./configure", []);
    if status != 0 {
        warn2!("run_build configure failed with {}: {} {}",
               status,
               str::from_utf8(output),
               str::from_utf8(error));

        return false;
    }
    let run::ProcessOutput { status, output, error } = repo.exec("make", []);
    if status != 0 {
        warn2!("run_build configure failed with {}: {} {}",
               status,
               str::from_utf8(output),
               str::from_utf8(error));
    }

    status == 0
}
