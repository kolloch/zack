---
title: Execution VFS
description: Using a virtual file system to execute build actions in, unlocks many interesting features.
---

One struggle with hermetic build systems such as Nix or Bazel often is the different file system layout
they require. They break certain assumptions that build systems make, e.g.:

- input directories are read-only,
- for performance reasons, a lot of symlinks are used to avoid file/directory duplication,
- relative files between source files and generated files break, because those file are stored in 
  different root directories,
- hard-coded paths don't work anymore,
- "global" dependencies are not at their typical locations, ...

If we faked a typical build environment where it is required, we could reuse more existing tooling
unchanged. Executing commands in a `chroot` or even stronger sandboxed environment with
and underlying VFS that provided input files at expected paths and tracked all writes, would be
useful.

## Getting changes back to the source

Where desired, getting the changes on the source files back to the source would be nice. E.g.
for code formatting tools. If those run in parallel with the build, changing the source files
adhoc is not desirable -- at least not without further coordination. But e.g. allowing
the user to copy all "proposed" changes to the source files easily, would make fast, incremental
code formatting/fixes facilitated by the build system easy.

## Implementation Ideas

- Using FUSE, e.g. with [fuser](https://github.com/cberner/fuser)
- [Overlay filesystem](https://wiki.archlinux.org/title/Overlay_filesystem) basic
- [mergerfs](https://github.com/trapexit/mergerfs) FUSE unionfs with many features
  - Merges different branches, e.g. root paths together
  - has a follow symlink feature
- [wrapfs](https://wrapfs.filesystems.org/) simple pass through as basis
- [cow-shell](https://manpages.ubuntu.com/manpages/noble/man1/cow-shell.1.html)
- [bindfs](https://github.com/mpartel/bindfs)
- [bind mounts](https://unix.stackexchange.com/questions/507420/mounting-is-slow-after-4000-mounts) might get
  slow if used excessively?
- [fuse passthrough](https://docs.rs/fuse-backend-rs/latest/fuse_backend_rs/passthrough/index.html)
- [crosvm](https://crosvm.dev/book/)
- [filter files](https://github.com/gburca/rofs-filtered?tab=readme-ov-file)
