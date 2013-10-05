use std::{run, str, os};

/// Represents a git repository.
#[deriving(Clone)]
pub struct Repo {
    path: Path
}

#[deriving(Clone, Encodable, Decodable)]
pub struct RemoteBranch {
    name: ~str,
    branch: ~str
}

/// Represents a SHA hash used by git.
#[deriving(Clone, Eq, IterBytes)]
pub struct Sha {
    value: ~str
}

impl Repo {
    /// Pretend `p` is the path to a git repo. You better make sure it
    /// is.
    pub fn new(p: Path) -> Repo {
        Repo { path: p }
    }

    /// "Clone" this repo into `dir`.
    pub fn new_subrepo(&self, dir: Path) -> Repo {
        if os::path_exists(&dir) {
            assert!(os::path_is_dir(&dir),
                    "creating a subrepo at a nondirectory %s", dir.to_str());

            info2!("{} already exists, reusing", dir.to_str());
        } else {
            // there's away to checkout into an external dir?
            let run::ProcessOutput { status, output, error } =
                run::process_output("git",
                                    [~"clone",
                                     self.path.to_str(),
                                     dir.to_str()]);

            if status != 0 {
                fail2!("Couldn't copy {} to {}: `{}` `{}`",
                       self.path.to_str(),
                       dir.to_str(),
                       str::from_utf8_slice(output),
                       str::from_utf8_slice(error))
            }
        }
        Repo::new(dir)
    }

    /// Convert a revision to a hash. `None` on failure.
    pub fn rev_parse(&self, rev: &str) -> Option<Sha> {
        let run::ProcessOutput { status, output, error } =
            self.exec("git", [~"rev-parse", rev.to_owned()]);

        if status == 0 {
            let s = str::from_utf8_owned(output);
            Some(Sha { value: s.trim().to_owned() })
        } else {
            warn2!("Repo.rev_parse failed with {}: {} {}",
                   status,
                   str::from_utf8(output),
                   str::from_utf8(error));
            None
        }
    }

    /// Retrieve the parent commit of `hash`.
    pub fn parent_commit(&self, hash: &Sha) -> Option<Sha> {
        self.rev_parse(hash.value + "^")
    }

    /// Checkout the given revision; anything that `git checkout` can
    /// understand. `false` on failure.
    #[allow(unused_variable)] // error handling should be better
    pub fn checkout(&self, rev: &str) -> bool {
        let run::ProcessOutput { status, output, error } =
            self.exec("git", [~"checkout", rev.to_owned()]);
        if status != 0 {
            warn2!("Repo.checkout failed with {}: {} {}",
                   status,
                   str::from_utf8(output),
                   str::from_utf8(error));
        }
        status == 0
    }

    /// Pull from a remote
    pub fn pull(&self, remote_branch: &RemoteBranch) -> bool {
        let run::ProcessOutput { status, output, error } =
            self.exec("git", [~"pull",
                              remote_branch.name.to_owned(),
                              remote_branch.branch.to_owned()]);
        if status != 0 {
            warn2!("Repo.pull failed with {}: {} {}",
                   status,
                   str::from_utf8(output),
                   str::from_utf8(error));
        }
        status == 0
    }

    /// Run the given command with the given args in the root of this
    /// git repo.
    pub fn exec(&self, name: &str, args: &[~str]) -> run::ProcessOutput {
        let opts = run::ProcessOptions {
            dir: Some(&self.path),
            .. run::ProcessOptions::new()
        };

        let mut proc = run::Process::new(name, args, opts);
        proc.finish_with_output()
    }
}
