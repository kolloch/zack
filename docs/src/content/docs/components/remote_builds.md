---
title: Remote Builds
description: Remote builds allow massive parallelization and executing builds on heterogeneous executors.
---

Remote builds allow 
- massive parallelization, and 
- executing builds on heterogeneous executors, e.g. 
    - special GPU hardware, 
    - extreme resource demands on CPU/mem,
    - connected hardware, e.g. test hardware for embedded systems.

## Integration with Existing Providers

There are already providers for build executors and we should be able to use them, e.g.:

- providers for the Bazel Remote Execution protocol, or
- [nix](https://nixos.org).

## Minimal requirements

It would be nice if executing an action remotely had minimal requirements, e.g.
like anything that can execute the zack-agent binary.
