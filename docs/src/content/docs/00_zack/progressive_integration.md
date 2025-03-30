---
title: Progressive Integration
description: Allow Zack to integrate with other build systems shallowly.
---

We want to allow Zack to integrate build steps with other build systems
early on. 

## Deep integration required?

For an optimal experience, Zack probably needs a certain
level of custom integration with supported wrapped build systems
and/or language ecosystems. We want to make that easy but it will
still require custom code.

## Shallow integration 

But Zack should provide a "good" escape hatch early on: A way
to integrate a different build system to perform certain
build steps. That requires running tasks that don't have
any knowledge about Zack and require "full control" over
the build environment.

An [Execution VFS](/zack/components/execution_vfs/)
could provide that build system with a familiar view of the
project, without making its execution unhermetic.

[HTTPS Interception](/zack/components/https_interception/)
could even allow these build systems to fetch external
dependencies while still retaining knowledge and control
over these dependencies.

## Automatic deeper integration

If the build system, e.g. already spawns sub processes like
compiler invocations, we might intercept those and provide
caching for them, transparently, without the build system
noticing.

## Smart integration primitives

But a lot of build systems avoid this because restarting
JIT-compiled (Just-In-Time compiled, as performed by nodejs/Java)
compilers can be expensive.

Maybe there is a way to make Zack aware of these logically
independent compiler invocations nevertheless with minimal
custom code on the Zack side, or minimal integration points
or code changes in the sub build system?
