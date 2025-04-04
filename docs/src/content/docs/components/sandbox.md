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

### Not triaged yet

- [jailtime](https://github.com/cblichmann/jailtime) Linux / Mac OS
- [jailkit](https://olivier.sessink.nl/jailkit/) Linux / Mac OS

### Probably too restrictive

- [goal](https://github.com/servo/gaol)
- [rusty-sandbox](https://crates.io/crates/rusty-sandbox): We'd need to allow more IO.

### How do others do it?

The sandbox in [Dune](https://dune.readthedocs.io/en/stable/concepts/sandboxing.html) sounds very
pragmatic.
