---
title: Bazel Build System Terminology
description: Explanation of common terminology.
---

Bazel uses the following concepts and terminology. This can be useful when we come
up with our own terminology.

## Target

High-level declaration of a build goal such as a library, a binary, a docker image.

A target has a [Label](#Label) and is created within a `BUILD.bazel` file by
instantiating a [Rule](#Rule).

## Rule

A rule defines the attributes that are available for defining a target of a certain type.

It also specifies a rule implementation that is used to expand the declarative rule
into [Actions](#Action).

## Action

An action defines which command to execute with which input files and what outputs to expect.

E.g. compiling an object file with the command `gcc -o hello.o -c hello.c`, the input file `hello.c`
and the expected output `hello.o`.

## Provider

A [rule](#Rule) implementation typically has to consume information from other rules, e.g.:

- the paths of required inputs which might have been produced by another rule,
- the names of libraries to link to the final binary,
- ...

Rules can access information from other targets they depend on by the "providers" they return.

Essentially, a provider is a struct of some fields that contain things like file paths. A
rule can return any number of providers which are then referenced by provider name from
dependent rules.

It is essentially the API of a target towards other rules. 

## Toolchains

Rule implementations often allow customizability of the actions they execute
through config settings and toolchains.

Toolchains selection typically depends on platform constraints and settings
specified for a build [configuration](#Configuration).

## Configuration

A configuration is a bunch of settings that affect what exact actions are generated.
For example, you might have configurations differing in:

- compiler optimization flags,
- target CPU architecture,
- ...

One target can be built in multiple configurations -- also commonly in the same build.

The initial configuration is specified when the build is invoked but changes
along certain dependency actions. E.g. if you build your compiler as part of your build, then
the dependency edge to you compiler would need to be built to be compatible
with the "execution" environment that might be different from the "target" environment
of your final build artifact.

## Label

The unique name of a (unconfigured) target. A full label looks like this:

```
@this_is_a_module//this/is/a/package:this_a_target_name
```

Targets in the current MODULE are referenced without the module specification:

```
//this/is/a/package:this_a_target_name
```

Targets in the same package can be referenced without mentioning the package path:

```
:this_is_a_target_name
```
