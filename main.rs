extern mod extra;
use std::{io, str};
use std::hashmap::HashSet;
use std::rt::io::{Writer, Append, ReadWrite};
use std::rt::io::file;
use std::rt::io::extensions::ReaderUtil;
use std::rt::io::timer;

use extra::arc::Arc;
use extra::json;
use extra::serialize::Decodable;
use extra::getopts;
use extra::getopts::groups;

use git::{Repo, Sha};
use commit_walker::CommitWalker;

pub mod commit_walker;
pub mod git;
pub mod build;
pub mod task_worker;

#[deriving(Encodable, Decodable)]
struct Config {
    // the maximum number of builders to run in tasks in this
    // executable.
    num_local_builders: Option<uint>,
    // the directory in which a directory is created for each commit.
    build_parent_dir: ~str,
    // the repository to bench.
    main_repo: ~str,
}

impl Config {
    fn load(p: &Path) -> Config {
        // json needs old io :(
        match io::file_reader(p) {
            Err(s) => fail!(s),
            Ok(reader) => {
                let msg = format!("{} is invalid json", p.to_str());
                let json = json::from_reader(reader).expect(msg);
                Decodable::decode(&mut json::Decoder(json))
            }
        }
    }
}

fn main() {
    let args = std::os::args();

    let opts =
        ~[groups::optopt("c", "config", "configuration file (default config.json)", "PATH"),
          groups::optopt("a", "already-built",
                         "file of hashes already built (default already-built.txt)", "PATH"),
          groups::optflag("h", "help", "show this help message")];

    let (config_path, already_built_path) = match groups::getopts(args.tail(), opts) {
        Err(err) => fail!(getopts::fail_str(err)),
        Ok(matches) => {
            if getopts::opt_present(&matches, "h") ||
                getopts::opt_present(&matches, "help") {
                println(groups::usage(args[0], opts));
                return;
            }

            let cfg = if getopts::opt_present(&matches, "c") {
                Path(getopts::opt_str(&matches, "c"))
            } else if getopts::opt_present(&matches, "config") {
                Path(getopts::opt_str(&matches, "config"))
            } else {
                Path("config.json")
            };
            let built = if getopts::opt_present(&matches, "a") {
                Path(getopts::opt_str(&matches, "a"))
            } else if getopts::opt_present(&matches, "already-built") {
                Path(getopts::opt_str(&matches, "already-built"))
            } else {
                Path("already-built.txt")
            };

            (cfg, built)
        }
    };

    let config = Config::load(&config_path);

    let mut already_built = HashSet::new();
    let already_built_file = file::open(&already_built_path, Append, ReadWrite);

    let mut already_built_file =
        already_built_file.expect(format!("Error opening {}", already_built_path.to_str()));

    let text = str::from_utf8_owned(already_built_file.read_to_end());

    for hash in text.line_iter() {
        already_built.insert(git::Sha { value: hash.to_owned() });
    }

    println!("Found {} already built commits", already_built.len());

    let num_workers = config.num_local_builders.unwrap_or_zero();
    println!("Running with max {} workers", num_workers);

    let build_dir = Path(config.build_parent_dir);
    let main_repo = Arc::new(Repo::new(Path(config.main_repo)));

    let mut walker = CommitWalker::new(main_repo.get(), already_built, already_built_file);

    // start the workers a-working. This vec contains a worker iff
    // it's currently working (or just finished a job); they get
    // removed when we've finished (e.g. run out of commits).
    let mut workers = ~[];
    for i in range(0, num_workers) {
        match walker.find_unbuilt_commit() {
            None => break,
            Some(hash) => {
                let worker = task_worker::TaskWorker::new(build_dir.clone(), main_repo.clone());

                info2!("Sending {} to worker {}", hash.value, i);
                worker.stream.send(build::BuildHash(hash));
                workers.push(worker);
            }
        }
    }
    'outer: loop {
        if workers.is_empty() {
            break // all done.
        }

        // we need to remove items mid-iteration.
        // FIXME using select + a timeout would be nicer here?
        let mut found_a_message = false;
        'scanner: for i in range(0, workers.len()) {
            if workers[i].stream.peek() {
                found_a_message = true;

                // something's been sent back to us!
                match workers[i].stream.try_recv() {
                    // stream closed.
                    None => { workers.swap_remove(i); },
                    // it was the crushing disappointment of failure. :(
                    Some(build::Failure(hash)) => {
                        println!("{} failed.", hash.value);

                        walker.register_built(hash.clone());
                    }
                    // \o/ we won!
                    Some(build::Success(_loc, hash)) => {
                        println!("{} succeeded.", hash.value);
                        walker.register_built(hash.clone());
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
            timer.sleep(5000);
        }
    }
}
