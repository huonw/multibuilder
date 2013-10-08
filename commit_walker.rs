use git::{Repo, Sha, RemoteBranch};
use std;
use std::rt::io::Writer;
use std::rt::io::file::FileStream;
use std::hashmap::HashSet;

pub struct CommitWalker<'self> {
    repo: &'self Repo,
    next_candidate: Option<Sha>,
    in_progress: HashSet<Sha>,
    already_built: HashSet<Sha>,
    already_built_file: FileStream,
    pull_remote: Option<&'self RemoteBranch>,
    earliest_build: int,
}

impl<'self> CommitWalker<'self> {
    pub fn new<'a>(repo: &'a Repo,
                   already_built: HashSet<Sha>, already_built_file: FileStream,
                   remote: Option<&'a RemoteBranch>, earliest_build: Option<int>)
        -> CommitWalker<'a> {
        CommitWalker {
            repo: repo,
            next_candidate: Some(repo.rev_parse("HEAD").expect("Missing HEAD")),
            in_progress: HashSet::new(),
            already_built: already_built,
            already_built_file: already_built_file,
            pull_remote: remote,
            earliest_build: earliest_build.unwrap_or(std::num::Bounded::min_value()),
        }
    }

    pub fn register_built(&mut self, hash: Sha, success: bool) {
        self.in_progress.remove(&hash);

        self.already_built_file.write(hash.value.as_bytes());
        self.already_built_file.write(bytes!(":"));
        self.already_built_file.write(match success {
                                      true  => bytes!("success"),
                                      false => bytes!("failure") });
        self.already_built_file.write(bytes!("\n"));

        self.already_built.insert(hash);
    }

    pub fn find_unbuilt_commit(&mut self) -> Option<Sha> {
        let CommitWalker { repo: ref repo,
            next_candidate: ref mut next_candidate,
            in_progress: ref mut in_progress,
            already_built: ref already_built,
            _
        } = *self;


        match self.pull_remote {
            Some(r_b) => {
                let old_head = repo.rev_parse("HEAD").expect("Missing current HEAD");
                repo.pull(r_b);
                let new_head = repo.rev_parse("HEAD").expect("Missing new HEAD");
                if new_head != old_head {
                    *next_candidate = Some(new_head);
                }
            }
            None => {}
        }

        match next_candidate.take() {
            None => None,
            Some(hash) => {
                let mut hash = hash;

                if self.earliest_build > repo.ctime(&hash).unwrap() {
                    info2!("Next candidate is too old, not building");
                    *next_candidate = None;
                    return None;
                }

                loop {
                    let parent = repo.parent_commit(&hash);
                    if self.earliest_build > repo.ctime(&hash).unwrap() {
                        info2!("Parent too old, not building");
                        *next_candidate = None;
                        return None;
                    }

                    // not built, and not in progress.
                    if !already_built.contains(&hash) && !in_progress.contains(&hash) {
                        *next_candidate = parent;
                        in_progress.insert(hash.clone());
                        return Some(hash);
                    }

                    match parent {
                        None => {
                            *next_candidate = None;
                            return None;
                        }
                        Some(h) => hash = h
                    }
                }
            }
        }
    }
}
