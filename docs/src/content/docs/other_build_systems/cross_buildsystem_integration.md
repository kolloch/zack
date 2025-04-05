---
title: Cross Buildsystem Integration
description: Integrating/Nesting Build Systems.
---

Integrating/Nesting Build Systems is tricky -- especially if the build systems in
question have a rather narrow view on how build steps should be performed.

Here are some resources on integration patterns and concrete implementations.

## Ninja Integration

The [ninja build system](https://ninja-build.org/) is an attractive integration target:

1. It is rather simple because it aims to be a generation target for other build systems, 
   and therefore does not have to be user-friendly.
2. Build systems like CMake, Meson use it as an execution target, so integrating with
   Ninja also allows integrating with these systems.

### Resources

[nix-ninja](https://github.com/pdtpartners/nix-ninja):

> Parses ninja.build files and generates a derivation per compilation unit.

It is written in rust and contains a ninja-parser!

[rules_rust](https://github.com/bazelbuild/rules_rust) provides Bazel rules for `rust`/`cargo`
and works really well. Rust would be an obvious early choice to support
in Zack to allow self-building.

[crate2nix](https://github.com/nix-community/crate2nix) generates [nix](https://nixos.org)
files to compile `rust`/`cargo` programs.
