---
title: Incremental Builds
description: Incremental Builds are an enabler for the inner dev loop.
---

Many compiler/tools allow incremental builds. Sometimes, they just smartly 
cache intermediate results (such as Zack attempts to) but, often, they
also take shortcuts that result in different code so that they are faster.

In the [Inner Dev Loop](/zack/concepts/dev_loop.md), speed is often
more important than determinism or correctness.

To support the incremental mode, we need to capture "temporary" or
at least non-primary build products and provision the next
build of the same artifact with the same files. E.g. an incremental
builds kinda creates a dependency on the last run of that action.

Some tools might also profit from a shared cache, keeping the tool running,
or [suspending/resuming](/zack/components/snapshot_resume) it. 

An example would be build systems such as Bazel. So this overlaps
with shallowly integrating other build systems.
