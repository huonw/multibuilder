# Multibuilder

Point it at a git repository and it will build all the direct
ancestors of the current HEAD.

Example configuration:

    {
        "num_local_builders": 10,
        "build_parent_dir": "build",
        "main_repo": "test",
        "build_commands": [
            {"name": "./configure", "args": []},
            {"name": "make", "args": []}
        ]
    }

This will run (at most) 10 builder tasks that checkout `./test` into
`./build/<hash>` and run `./configure` then `make` in that directory.

Hashes that have already been built are stored in `already_built.txt`;
this file is updated progressively, and so it is safe to just kill the
builder mid-operation. It must exist.

The location of `config.json` and `already_built.txt` can be
controlled with `-c` and `-a` respectively.

## TODO

- git2-rs
- grease-bench to benchmark automatically
- handle pulling to get new commits
- manual rebuild/bench of specific commits.
- support distributed builds
