# Multibuilder

[![Build Status](https://travis-ci.org/huonw/multibuilder.png)](https://travis-ci.org/huonw/multibuilder)

Point it at a git repository and it will build all the direct
ancestors of the current HEAD.

Example configuration:

    {
        "num_local_builders": 10,
        "build_parent_dir": "build",
        "output": {
            "parent_dir": "output_dir",
            "to_move": ["objects/final_binary", "objects/associated_file"],
        },
        "main_repo": "test",
        "build_commands": [
            {"name": "./configure", "args": []},
            {"name": "make", "args": []}
        ],
        "pull_from": {
            "name": "foo",
            "branch": "master"
        },
        "earliest_build": null,
        "when_finished": [
            {"name": "echo", "args": ["Finished!"]}
        ],
    }

This will run (at most) 10 builder tasks that checkout `./test` into
`./build/<hash>` and run `./configure` then `make` in that
directory. Since `output` is not `null`, the build being successful
will mean that the
`./build/<hash>/objects/{final_binary,associated_file` files are moved
to `./output_dir/<hash>/<filename>` and the `./build/<hash>` directory
deleted (the elements of `to_move` can be directories or files). If
`output` is `null`, nothing is moved or deleted.

The `pull_from` field gives the name and branch of a remote from which
to pull between each benchmark, to check for updates. Benching starts
from the new commits, if there are any. `pull_from` can be `null` to
disable auto-pulling. `earliest_build` (optionally) gives a timestamp
which represents the oldest age of commits to build; the builder will
not build any commits older than this (using the commit time, not the author
time).

When there are no more commits to build, the commands given in `when_finished`
will be run in the same manner as the `build_commands`, although in the
working directory, not a checked out repository. In the future, multibuilder
will poll the repository for changes and wake up when there are more to build,
making this option more useful.

Hashes that have already been built are stored in `already-built.txt`;
this file is updated progressively, and so it is safe to just kill the
builder mid-operation. It must exist.

The location of `config.json` and `already-built.txt` can be
controlled with `-c` and `-a` respectively.

## TODO

- git2-rs
- grease-bench to benchmark automatically
- manual rebuild/bench of specific commits.
- support distributed builds
- support running a command after a certain number of builds and
  provide the directories/hashes of the most recently built commits
  (e.g. it calls `bench-script 56da5f65..1314 12313..545 a0f9..123`
  with all the hashes it built in the most recent run). This would
  have to wait for all builds to finish before calling this script.
- spin up more threads after threads get killed and HEAD moves.
