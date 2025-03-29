---
title: Meta-build
description: Allow generating build rules based on source file analysis.
---

Build systems like e.g. Bazel require a mostly static build graph to work. That makes their implementation easier.

That means, developers need to reproduce target relationships from golang or other languages in BUILD.bazel files.

## Build File Generation

To avoid that churn, a build file generator such as [Gazelle](https://github.com/bazel-contrib/bazel-gazelle)
is used. That avoids the churn of writing the rules manually but adds a different hassle:

Calling the code generator at the right time, when dependencies between different go modules have changes.

Common solutions are:

1. Letting the developer call it manually when they hit problems.
2. Call it before every Bazel build.

Both are not ideal. The rules when the generation should be triggered, sounds awfully much like what a
build system does: Each go file needs to be reanalyzed for its dependencies on a change, potentially
resulting in a build file change.

This should be facilitated by the build system, i.e. it should have capabilities to generate build rules
based on source file analysis.

## Things to preserve

One advantage of build file generation is, that the generated targets are actually output in a fashion
that is readable to a build engineer. If there are problems, they can be quite easily inspected or manipulated.
