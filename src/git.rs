use std::str;
use std::io::fs::PathExtensions;
use std::io::process::{Command, ProcessOutput};

/// Represents a git repository.
#[deriving(Clone)]
pub struct Repo {
    pub path: Path
}

#[deriving(Clone, Encodable, Decodable, Show)]
pub struct RemoteBranch {
    pub name: String,
    pub branch: String
}

/// Represents a SHA hash used by git.
#[deriving(Clone, PartialEq, Eq, Hash, Show)]
pub struct Sha {
    pub value: String
}

impl Repo {
    /// Pretend `p` is the path to a git repo. You better make sure it
    /// is.
    pub fn new(p: Path) -> Repo {
        Repo { path: p }
    }

    /// "Clone" this repo into `dir`.
    pub fn new_subrepo(&self, dir: Path) -> Repo {
        if dir.exists() {
            assert!(dir.is_dir(), "creating a subrepo at a nondirectory {}", dir.display());

            info!("{} already exists, reusing", dir.display());
        } else {
            // there's away to checkout into an external dir?
            let ProcessOutput { status, output, error } =
                Command::new("git")
                    .arg("clone")
                    .arg(&self.path)
                    .arg(&dir)
                    .output()
                    .unwrap();

            if !status.success() {
                fail!("Couldn't copy {} to {}: `{}` `{}`",
                      self.path.display(),
                      dir.display(),
                      str::from_utf8(output.as_slice()),
                      str::from_utf8(error.as_slice()))
            }
        }
        Repo::new(dir)
    }

    /// Convert a revision to a hash. `None` on failure.
    pub fn rev_parse(&self, rev: &str) -> Option<Sha> {
        let ProcessOutput { status, output, error } =
            self.exec("git", ["rev-parse".to_string(), rev.to_string()]);

        if status.success() {
            let s = str::from_utf8(output.as_slice()).expect("non-utf8 git output!");
            Some(Sha { value: s.trim().to_string() })
        } else {
            warn!("Repo.rev_parse failed with {}: {} {}",
                  status,
                  str::from_utf8(output.as_slice()),
                  str::from_utf8(error.as_slice()));
            None
        }
    }

    /// Retrieve the parent commit of `hash`.
    pub fn parent_commit(&self, hash: &Sha) -> Option<Sha> {
        self.rev_parse((format!("{}^", hash)).as_slice())
    }

    /// Checkout the given revision; anything that `git checkout` can
    /// understand. `false` on failure.
    #[allow(unused_variable)] // error handling should be better
    pub fn checkout(&self, rev: &str) -> bool {
        let ProcessOutput { status, output, error } =
            self.exec("git", ["checkout".to_string(), rev.to_string()]);
        if !status.success() {
            warn!("Repo.checkout failed with {}: {} {}",
                   status,
                   str::from_utf8(output.as_slice()),
                   str::from_utf8(error.as_slice()));
        }
        status.success()
    }

    /// Pull from a remote
    pub fn pull(&self, remote_branch: &RemoteBranch) -> bool {
        let ProcessOutput { status, output, error } =
            self.exec("git", ["pull".to_string(),
                              remote_branch.name.to_string(),
                              remote_branch.branch.to_string()]);
        if !status.success() {
            warn!("Repo.pull failed with {}: {} {}",
                   status,
                   str::from_utf8(output.as_slice()),
                   str::from_utf8(error.as_slice()));
        }
        status.success()
    }

    /// Run the given command with the given args in the root of this
    /// git repo.
    pub fn exec(&self, name: &str, args: &[String]) -> ProcessOutput {
        Command::new(name)
            .args(args)
            .cwd(&self.path)
            .output()
            .unwrap()
    }

    /// Get a UNIX timestamp of the commit date. `None` on failure.
    pub fn ctime(&self, hash: &Sha) -> Option<i64> {
        let time = self.exec("git", &["log".to_string(), hash.value.clone(),
                                      "-1".to_string(), "--format=%ct".to_string()]).output;
        let time = str::from_utf8(time.as_slice()).expect("non-utf8 git output!");
        from_str(time.trim())
    }
}
