#[feature(managed_boxes)]; // this can be removed once extra::term doesn't use @mut Writer
#[feature(macro_rules)];

#[deny(warnings)];

extern mod extra;
use std::{str, run};
use std::hashmap::HashSet;
use std::rt::io::{Reader, Writer, Append, ReadWrite, stdout, File, timer};

use extra::arc::Arc;
use extra::{glob, json};
use extra::serialize::Decodable;
use extra::getopts::groups;

use git::{Repo, Sha};
use commit_walker::CommitWalker;

pub mod commit_walker;
pub mod git;
pub mod build;
pub mod task_worker;

fn is_dir(p: Path) -> Path {
    assert!(p.is_dir(), "`{}` is not a directory", p.display());
    p
}

#[deriving(Encodable, Decodable)]
struct Config {
    /// the maximum number of builders to run in tasks in this
    /// executable.
    num_local_builders: Option<uint>,
    /// the directory in which a directory is created for each commit.
    build_parent_dir: ~str,

    /// information about where to move (a subset of) the build
    /// artifacts. `None` for no movement.
    output: Option<OutputMovement>,

    /// the repository to bench.
    main_repo: ~str,
    /// the commands to run when building.
    build_commands: ~[Command],
    /// the branch to pull from when updating the repo
    pull_from: Option<git::RemoteBranch>,
    /// a unix timestamp. if a commit is older than this, it won't be built.
    earliest_build: Option<i64>,
    when_finished: ~[Command],
}

#[deriving(Encodable, Decodable)]
struct OutputMovement {
    /// The directory to place the <hash> directory which the
    /// `to_move` files get placed in.
    parent_dir: ~str,
    /// The files/directories to move from the build dir to
    /// `parent_dir/<hash>/`.
    to_move: ~[~str]
}

#[deriving(Encodable, Decodable, Clone)]
pub struct Command {
    name: ~str,
    args: ~[~str]
}

impl Config {
    fn load(p: &Path) -> Config {
        match File::open(p) {
            None => fail!("couldn't open {}", p.display()),
            Some(ref mut reader) => {
                let msg = format!("{} is invalid json", p.display());
                let json = json::from_reader(reader as &mut Reader).expect(msg);
                Decodable::decode(&mut json::Decoder(json))
            },
        }
    }
}

fn main() {
    let args = std::os::args();

    let opts =
        ~[groups::optopt("c", "config", "configuration file (default ./config.json)", "PATH"),
          groups::optopt("a", "already-built",
                         "file of hashes already built (default ./already-built.txt)", "PATH"),
          groups::optflag("h", "help", "show this help message")];

    let (config_path, already_built_path) = match groups::getopts(args.tail(), opts) {
        Err(err) => fail!(err.to_err_msg()),
        Ok(matches) => {
            if matches.opt_present("h") || matches.opt_present("help") {
                println(groups::usage(args[0], opts));
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
        already_built_file.expect(format!("Error opening {}", already_built_path.display()));

    let text = str::from_utf8_owned(already_built_file.read_to_end());

    for hash in text.line_iter() {
        already_built.insert(git::Sha { value: hash.split_iter(':').next().unwrap().to_owned() });
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

    let mut walker = CommitWalker::new(main_repo.get(),
                                       already_built,
                                       already_built_file,
                                       config.pull_from.as_ref(),
                                       config.earliest_build);

    // start the workers a-working. This vec contains a worker iff
    // it's currently working (or just finished a job); they get
    // removed when we've finished (e.g. run out of commits).
    let mut workers = ~[];
    for i in range(0, num_workers) {
        match walker.find_unbuilt_commit() {
            None => { info!("No more commits to build"); break },
            Some(hash) => {
                let worker = task_worker::TaskWorker::new(build_dir.clone(),
                                                          main_repo.clone(),
                                                          build_commands.clone());

                info!("Sending {} to worker {}", hash.value, i);
                worker.stream.send(build::BuildHash(hash));
                workers.push(worker);
            }
        }
    }
    'outer: loop {
        if workers.is_empty() {
            info!("No more builds, running when_finished");
            for cmd in config.when_finished.iter() {
                use std::run::ProcessOptions;
                debug!("Running {:?}", cmd);
                let mut result = run::Process::new(cmd.name, cmd.args, ProcessOptions::new());
                let result = result.finish_with_output();

                if result.status != 0 {
                    error!("{} failed", cmd.name);
                }
            }
            return;
        }

        // we need to remove items mid-iteration.
        // FIXME using select + a timeout would be nicer here?
        let mut found_a_message = false;
        let term = extra::term::Terminal::new(@mut stdout() as @mut Writer).unwrap();
        'scanner: for i in range(0, workers.len()) {
            if workers[i].stream.peek() {
                found_a_message = true;

                // something's been sent back to us!
                match workers[i].stream.try_recv() {
                    // stream closed.
                    None => { workers.swap_remove(i); },
                    // it was the crushing disappointment of failure. :(
                    Some(build::Failure(hash)) => {
                        term.fg(extra::term::color::RED);
                        println!("{} failed.", hash.value);
                        term.reset();

                        walker.register_built(hash.clone(), false);
                    }
                    // \o/ we won!
                    Some(build::Success(loc, hash)) => {
                        term.fg(extra::term::color::GREEN);
                        println!("{} succeeded.", hash.value);
                        term.reset();

                        // FIXME: break this out.
                        match config.output {
                            None => {}
                            Some(ref output) => {
                                let suboutput_dir = Path::new(output.parent_dir.as_slice())
                                    .join(hash.value.as_slice());

                                // create the final output directory.
                                let mkdir = run::process_output("mkdir",
                                                                [~"-p",
                                                                 format!("{}",
                                                                         suboutput_dir.display())]);
                                if mkdir.status != 0 {
                                    fail!("mkdir failed on {} with {}",
                                          suboutput_dir.display(),
                                          std::str::from_utf8(mkdir.error));
                                }

                                match loc {
                                    build::Local(p) => {
                                        // move some subdirectory of the final
                                        // output (in `p`) to the appropriate
                                        // place.
                                        let mut move_args: ~[~str] = do output.to_move.map |s| {
                                            let glob_path = p.join(s.as_slice());
                                            let glob_str = format!("{}", glob_path.display());
                                            let glob = glob::glob(glob_str);

                                            // XXX shouldn't be using strings here :(
                                            glob.map(|x| format!("{}", x.display()))
                                                .to_owned_vec()
                                        }.concat_vec();
                                        move_args.push(~"-vt");
                                        // XXX strings
                                        move_args.push(format!("{}", suboutput_dir.display()));

                                        // move what we want.
                                        let mv = run::process_output("mv", move_args);
                                        if mv.status != 0 {
                                            println!("mv: {}", str::from_utf8(mv.output));
                                            fail!("mv failed with {}", str::from_utf8(mv.error))
                                        }

                                        // delete the build dir.
                                        let rm = run::process_output("rm",
                                                                     [~"-rf",
                                                                      // XXX strings
                                                                      format!("{}", p.display())]);
                                        if rm.status != 0 {
                                            println!("rm: {}", str::from_utf8(rm.output));
                                            fail!("rm failed on {} with {}", p.display(),
                                                  std::str::from_utf8(rm.error));
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
                        workers.swap_remove(i);
                        break 'scanner;
                    }
                    Some(hash) => workers[i].stream.send(build::BuildHash(hash)),
                }
            }
        }

        // FIXME: handle fetching new commits

        // only pause if we didn't do anything in the last run.
        if !found_a_message {
            // no need to busy wait
            let mut timer = timer::Timer::new().expect("No timer??");
            timer.sleep(500);
        }
    }
}
