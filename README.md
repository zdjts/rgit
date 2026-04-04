# rgit

A minimal Git implementation in Rust, built for learning purposes. Implements core Git concepts from scratch: object storage, index, refs, and working tree management.

## Build

```bash
cargo build --release
```

## Usage

All commands must be run from a directory containing `.rgit/`.

```bash
# Initialize a repository
rgit init

# Stage files
rgit add <file|dir> [<file|dir>...]
rgit rm <file> [--cached]        # --cached keeps the file on disk

# Inspect the index and working tree
rgit status

# Build objects
rgit write-tree                  # index → tree object, prints hash
rgit hash-object [-w] <file>     # hash a file; -w writes to object store

# Commit
rgit commit-tree <tree> -a "Name <email>" -m "message" [-p <parent>]

# Refs
rgit update-ref refs/heads/master <hash>
rgit set-head refs/heads/master  # symbolic ref, or <hash> for detached HEAD
rgit show-ref                    # show where HEAD points

# History
rgit log                         # from HEAD
rgit log <hash>                  # from a specific commit

# Restore
rgit checkout <tree|commit|ref>  # restore working tree + sync index

# Inspect objects
rgit cat-file -p <hash>
```

### Typical workflow

```bash
rgit init
rgit add src README.md
TREE=$(rgit write-tree)
COMMIT=$(rgit commit-tree "$TREE" -a "Alice <alice@example.com>" -m "Initial commit")
rgit update-ref refs/heads/master "$COMMIT"
rgit log
rgit status
```

## Architecture

```
src/
  main.rs       CLI (clap subcommands)
  commands.rs   High-level command logic: init, status, log, add, rm, checkout
  object.rs     Object construction and parsing: blob, tree, commit
  storage.rs    Zlib read/write; objects stored at .rgit/objects/<2>/<38>
  index.rs      Binary index file (.rgit/index): mtime, mode, sha1, path per entry
  refs.rs       Ref resolution and update: HEAD, refs/heads/*
  hash.rs       hex_to_bytes utility
```

Object format on disk: `"<type> <size>\0<content>"`, zlib-compressed.

Index format: `DIRC` header + fixed-width entries (mtime in nanoseconds, mode, sha1, null-terminated path) + SHA-1 checksum.

## What's not implemented

- `commit` (one-step shortcut; currently requires `write-tree` + `commit-tree` + `update-ref`)
- `branch` / `merge`
- `diff`
- `.rgitignore`
- Pack files / GC
- Remote operations
