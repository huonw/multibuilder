#![feature(macro_rules, phase)]

//#![deny(warnings)]

extern crate serialize;
extern crate getopts;
extern crate glob;
extern crate term;
#[phase(plugin, link)]
extern crate log;

use std::io::fs::PathExtensions;
use std::io::Command as IoCommand;
use std::io::{Append, ReadWrite, stdout, File, timer};
use std::str;
use std::vec::Vec;

use std::collections::HashSet;
use serialize::Decodable;
use serialize::json;
use std::sync::Arc;

use commit_walker::CommitWalker;
use git::{Repo, Sha};

pub mod commit_walker;
pub mod git;
pub mod build;
pub mod task_worker;

fn is_dir(p: Path) -> Path {
    assert!(p.is_dir(), "`{}` is not a directory", p.display());
    p
}

#[deriving(Encodable, Decodable, Show)]
struct Config {
    /// the maximum number of builders to run in tasks in this
    /// executable.
    num_local_builders: Option<uint>,
    /// the directory in which a directory is created for each commit.
    build_parent_dir: String,

    /// information about where to move (a subset of) the build
    /// artifacts. `None` for no movement.
    output: Option<OutputMovement>,

    /// the repository to bench.
    main_repo: String,
    /// the commands to run when building.
    build_commands: Vec<Command>,
    /// the branch to pull from when updating the repo
    pull_from: Option<git::RemoteBranch>,
    /// a unix timestamp. if a commit is older than this, it won't be built.
    earliest_build: Option<i64>,
    when_finished: Vec<Command>,
}

#[deriving(Encodable, Decodable, Show)]
struct OutputMovement {
    /// The directory to place the <hash> directory which the
    /// `to_move` files get placed in.
    parent_dir: String,
    /// The files/directories to move from the build dir to
    /// `parent_dir/<hash>/`.
    to_move: Vec<String>
}

#[deriving(Encodable, Decodable, Clone, Show)]
pub struct Command {
    name: String,
    args: Vec<String>
}

impl Config {
    fn load(p: &Path) -> Config {
        match File::open(p) {
            Err(e) => fail!("couldn't open {} ({})", p.display(), e),
            Ok(ref mut reader) => {
                let msg = format!("{} is invalid json", p.display());
                let json = json::from_reader(reader as &mut Reader).ok().expect(msg.as_slice());
                Decodable::decode(&mut json::Decoder::new(json)).unwrap()
            },
        }
    }
}

fn main() {
    let args = std::os::args();

    let opts =
        vec![getopts::optopt("c", "config", "configuration file (default ./config.json)", "PATH"),
          getopts::optopt("a", "already-built",
                         "file of hashes already built (default ./already-built.txt)", "PATH"),
          getopts::optflag("h", "help", "show this help message")];

    let (config_path, already_built_path) = match getopts::getopts(args.tail(), opts.as_slice()) {
        Err(err) => fail!("{}", err),
        Ok(matches) => {
            if matches.opt_present("h") || matches.opt_present("help") {
                println!("{}", getopts::usage(args[0].as_slice(), opts.as_slice()));
                return;
            }

            let cfg = if matches.opt_present("c") {
                Path::new(matches.opt_str("c").unwrap())
            } else if matches.opt_present("config") {
                Path::new(matches.opt_str("config").unwrap())
            } else {
                Path::new("config.json")
            };
            let built = if matches.opt_present("a") {
                Path::new(matches.opt_str("a").unwrap())
            } else if matches.opt_present("already-built") {
                Path::new(matches.opt_str("already-built").unwrap())
            } else {
                Path::new("already-built.txt")
            };

            (cfg, built)
        }
    };

    let config = Config::load(&config_path);

    let mut already_built = HashSet::new();

    // FIXME: allow this to be created automagically.
    let already_built_file = File::open_mode(&already_built_path, Append, ReadWrite);

    let mut already_built_file =
        already_built_file.ok().expect(format!("Error opening {}", already_built_path.display()).as_slice());

    let text = String::from_utf8(already_built_file.read_to_end().unwrap())
        .ok().expect("already-built non-utf8!");

    for hash in text.as_slice().split('\n') {
        already_built.insert(git::Sha { value: hash.split(':').next().unwrap().to_string() });
    }

    println!("Found {} already built commits", already_built.len());

    let num_workers = config.num_local_builders.unwrap_or_default();
    println!("Running with max {} workers", num_workers);

    let build_dir = is_dir(Path::new(config.build_parent_dir.as_slice()));

    let main_repo_dir = is_dir(Path::new(config.main_repo.as_slice()));

    let main_repo = Arc::new(Repo::new(main_repo_dir));

    // check the dir exists
    match config.output {
        None => {}
        Some(ref output) => {
            is_dir(Path::new(output.parent_dir.as_slice()));
        }
    }

    let build_commands = Arc::new(config.build_commands.clone());

    let mut walker = CommitWalker::new(&*main_repo,
                                       already_built,
                                       already_built_file,
                                       config.pull_from.as_ref(),
                                       config.earliest_build);

    // start the workers a-working. This vec contains a worker iff
    // it's currently working (or just finished a job); they get
    // removed when we've finished (e.g. run out of commits).
    let mut workers = Vec::with_capacity(num_workers);
    for i in range(0, num_workers) {
        match walker.find_unbuilt_commit() {
            None => { info!("No more commits to build"); break },
            Some(hash) => {
                let worker = task_worker::TaskWorker::new(build_dir.clone(),
                                                          main_repo.clone(),
                                                          build_commands.clone());

                info!("Sending {} to worker {}", hash.value, i);
                worker.send(build::BuildHash(hash));
                workers.push(worker);
            }
        }
    }
    'outer: loop {
        if workers.is_empty() {
            info!("No more builds, running when_finished");
            for cmd in config.when_finished.iter() {
                debug!("Running {}", cmd);
                let mut result = IoCommand::new(cmd.name.as_slice());
                result.args(cmd.args.as_slice());
                let result = result.output().unwrap();

                if !result.status.success() {
                    error!("{} failed", cmd.name);
                }
            }
            return;
        }

        // we need to remove items mid-iteration.
        // FIXME using select + a timeout would be nicer here?
        let mut found_a_message = false;
        let mut term = term::stdout().unwrap();
        'scanner: for i in range(0, workers.len()) {
            match workers[i].recv_opt() {
                // stream closed.
                Err(()) => {
                    debug!("removing a worker, other end hung up");
                    workers.swap_remove(i);
                },
                // it was the crushing disappointment of failure. :(
                Ok(build::Failure(hash)) => {
                    found_a_message = true;
                    term.fg(term::color::RED).unwrap();
                    println!("{} failed.", hash.value);
                    term.reset().unwrap();

                    walker.register_built(hash.clone(), false);
                }
                // \o/ we won!
                Ok(build::Success(loc, hash)) => {
                    found_a_message = true;
                    term.fg(term::color::GREEN).unwrap();
                    println!("{} succeeded.", hash.value);
                    term.reset().unwrap();

                    // FIXME: break this out.
                    match config.output {
                        None => {}
                        Some(ref output) => {
                            let suboutput_dir = Path::new(output.parent_dir.as_slice())
                                .join(hash.value.as_slice());

                            // create the final output directory.
                            let mkdir = IoCommand::new("mkdir")
                                .arg("-p")
                                .arg(&suboutput_dir)
                                .output().unwrap();

                            if !mkdir.status.success() {
                                fail!("mkdir failed on {} with {}",
                                      suboutput_dir.display(),
                                      std::str::from_utf8(mkdir.error.as_slice()));
                            }

                            match loc {
                                build::Local(p) => {
                                    // move some subdirectory of the final
                                    // output (in `p`) to the appropriate
                                    // place.
                                    let mut move_args: Vec<String> = output.to_move.iter().map(|s| {
                                        let glob_path = p.join(s.as_slice());
                                        let glob_str = format!("{}", glob_path.display());
                                        let glob = glob::glob(glob_str.as_slice());

                                        // XXX shouldn't be using strings here :(
                                        glob.map(|x| format!("{}", x.display())).collect::<Vec<String>>()
                                    }).collect::<Vec<Vec<String>>>().as_slice().concat_vec();
                                    move_args.push("-vt".to_string());
                                    // XXX strings
                                    move_args.push(format!("{}", suboutput_dir.display()));

                                    // move what we want.
                                    let mv = IoCommand::new("mv").args(move_args.as_slice()).output().unwrap();
                                    if !mv.status.success() {
                                        println!("mv: {}", str::from_utf8(mv.output.as_slice()));
                                        fail!("mv failed with {}", str::from_utf8(mv.error.as_slice()))
                                    }

                                    // delete the build dir.
                                    let rm = IoCommand::new("rm")
                                        .arg("-rf")
                                        .arg(&p)
                                        .output().unwrap();

                                    if !rm.status.success() {
                                        println!("rm: {}", str::from_utf8(rm.output.as_slice()));
                                        fail!("rm failed on {} with {}", p.display(),
                                              std::str::from_utf8(rm.error.as_slice()));
                                    }
                                }
                            }
                        }
                    }

                    walker.register_built(hash.clone(), true);
                }
            }
            // get back to work!
            match walker.find_unbuilt_commit() {
                None => {
                    // no more commits so remove this (now useless) worker.
                    debug!("Removing worker, it's useless now");
                    workers.swap_remove(i);
                    break 'scanner;
                }
                Some(hash) => workers[i].send(build::BuildHash(hash)),
            }
        }

        // FIXME: handle fetching new commits

        // only pause if we didn't do anything in the last run.
        if !found_a_message {
            // no need to busy wait
            let mut timer = timer::Timer::new().ok().expect("No timer??");
            timer.sleep(std::time::Duration::seconds(500));
        }
    }
}
