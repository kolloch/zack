---
title: Sandbox
description: Allow actions access to exactly the right files.
---

For enforcing hermetic execution of build steps, Zack needs sandboxing technology.
The immediate goal is to prevent accidental sources of non-determinism such as inputs unknown
to the build system.

The implementation will most likely overlap with
[Execution VFS](/zack/components/execution_vfs/) and 
[Execution Instrumentation](/zack/components/execution_instrumentation/).

## Existing Tools

- The Bazel Sandbox (not well documented)
- Docker: Very common. Startup of individual containers relatively slow, so probably not suited
  for many small isolated actions.
- [buildah](https://github.com/containers/buildah) during building
- [common-rs](https://github.com/containers/conmon-rs) A pod level OCI container runtime monitor.
- [youki](https://github.com/youki-dev/youki) A container runtime in Rust, awesome pointers here: https://youki-dev.github.io/youki/developer/libcontainer.html
- [syd](https://lib.rs/crates/syd)

https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.pre_exec

### Not triaged yet

- [jailtime](https://github.com/cblichmann/jailtime) Linux / Mac OS
- [jailkit](https://olivier.sessink.nl/jailkit/) Linux / Mac OS

### Probably too restrictive

- [goal](https://github.com/servo/gaol)
- [rusty-sandbox](https://crates.io/crates/rusty-sandbox): We'd need to allow more IO.

### How do others do it?

The sandbox in [Dune](https://dune.readthedocs.io/en/stable/concepts/sandboxing.html) sounds very
pragmatic.

## Technology

https://github.com/soh0ro0t/kernel-namespace/blob/master/user-namespace.md
