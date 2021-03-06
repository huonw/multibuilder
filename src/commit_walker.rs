use git::{Repo, Sha, RemoteBranch};
use std;
use std::io::fs::File;
use std::collections::HashSet;

pub struct CommitWalker<'a> {
    repo: &'a Repo,
    next_candidate: Option<Sha>,
    in_progress: HashSet<Sha>,
    already_built: HashSet<Sha>,
    already_built_file: File,
    pull_remote: Option<&'a RemoteBranch>,
    earliest_build: i64,
}

impl<'r> CommitWalker<'r> {
    pub fn new<'a>(repo: &'a Repo,
                   already_built: HashSet<Sha>, already_built_file: File,
                   remote: Option<&'a RemoteBranch>, earliest_build: Option<i64>)
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

        let status = match success {
            true => "success",
            false => "failure",
        };

        (writeln!(&mut self.already_built_file, "{}:{}", hash.value, status)).unwrap();

        self.already_built.insert(hash);
    }

    pub fn find_unbuilt_commit(&mut self) -> Option<Sha> {
        let CommitWalker {
            ref repo,
            ref mut next_candidate,
            ref mut in_progress,
            ref mut already_built,
            ..
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
                    info!("Next candidate is too old, not building");
                    *next_candidate = None;
                    return None;
                }

                loop {
                    let parent = repo.parent_commit(&hash);
                    if self.earliest_build > repo.ctime(&hash).unwrap() {
                        info!("Parent too old, not building");
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
