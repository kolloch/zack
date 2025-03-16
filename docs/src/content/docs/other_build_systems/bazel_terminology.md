---
title: Bazel Notes
description: About the Bazel Build System.
---

The [Bazel Build System](https://bazel.build), born as "Blaze" within Google, is a hugely
influential Build System that:

- uses hermetic, sandboxed executions, enabling
    - High Parallelization,
    - Distributed Caching,
    - Distributed Remote Building,
    - while still maintaining the illusion that each individual build action starts from a clean slate,
- uses [Starlark](/zack/components/starlark) as configuration and extension language, enabling
    - predictable invalidation of already generated/expanded build targets,
    - relatively familiar looking syntax (Python-based),
    - automatic large-scale refactoring of build rules with tools like [Buildozer](https://github.com/bazelbuild/buildtools/blob/main/buildozer/README.md),
- supports many language ecosystems (C++, JavaScript, TypeScript, Golang, Python, Java, Kotlin, ...),
- has a relatively rich ecosystem of competing vendors for consulting and managed solutions.

It is commonly perceived as powerful but complex:
"you need to have a team of build engineers to maintain it",

Bazel thrives on large, explicit build graphs, which need to be specified with build rules
in "BUILD.bazel" files. A well decomposed build graph can lead to exceptionally
fast and reliable build and test cycles.

Bazel provides extensive inspection abilities into this build graph but they often
rather complicated to use. (As opposed to, let's say, a simple TUI for build target
inspection.)

Obviously, this summary is very subjective and condensed. Feel free to point out misleading
statements. Below are outlines on more problem areas.

## Out of the box error reporting

Without external tools, developers often have trouble even localizing the error
that caused the build to fail. This is partially due to

- high-parallelism making good error reporting more difficult,
- the error reporting text output not being super clear and hard to parse
  for human beings.

Various Web UIs make use of Bazel's inspection capabilities to improve this
but that requires more setup and integration.

## Language-specific IDE/Developer Experience Support

IDE support, especially integrating with language specific test frameworks
and the like, is often sketchy. A team typically needs to specify

1. config and build files that make the IDE work well, and
2. in addition, Bazel build files that make Bazel work well.

Things that work seamlessly with the mainstream language-specific tools, like
selecting a test method and executing it, often do not work with Bazel integration
but bypass it completely.

Therefore, Continuous Integration (CI) is often handled by Bazel, whereas a huge chunk of the
local development workflow is Bazel-free.

Developers typically already know their language-specific tools well and
need to learn about Bazel to maintain the CI which is perceived as unwanted overhead.

There are several approaches that are tried to maintain the gap:

1. Use the flexibility and power of Bazel to read some of the build information from
   language-specific build files.
2. Generate Bazel build files automatically by analyzing the source code. Often
   using [Gazelle](https://github.com/bazel-contrib/bazel-gazelle).

The first one usually can never be complete due to purposeful restriction in Bazel.
So the best you can do is to detect inconsistencies and remind the user
to also add the new Cargo.toml file or "node_modules" directory to a configuration
list. Depending on how it is done, it might also make Bazel slower due to doing
work on startup (in `repo_rules`) which is repeated on every startup.

The second one requires running an extra tool before each build. Depending
on the repository size, language and tool, undesirably, this adds time to the 
[dev loop](/zack/concepts/dev_loop/). Also, what could speed up these tools,
looking at what source files have changed and then regenerating potentially
affected build tools, looks suspiciously like what a build system
already does. Making closer integration seem attractive.

## Special Continuous Integration Requirements

Bazel works best with some infrastructure in place for

- distributed caching,
- distributed execution.

Also Bazel caches important information about the build graph in memory,
making builds that start Bazel anew every time unnecessarily slow.

Preventing this in CI systems like Github Actions, Circle CI, Jenkins, ...
is not trivial because they usually want to start from a clean slate in a
docker image with some directories designated as caches.

Many things can be smoothed over by solutions like [BuildBuddy](https://www.buildbuddy.io/),
[BuildBarn](https://github.com/buildbarn), and others but you either have to throw
money or time at this, often both.
   
## Terminology

Bazel uses the following concepts and terminology. This can be useful when we come
up with our own terminology. Obviously, this is a small excerpt.

### Target

High-level declaration of a build goal such as a library, a binary, a docker image.

A target has a [Label](#Label) and is created within a `BUILD.bazel` file by
instantiating a [Rule](#Rule). 

[Target Bazel Docs](https://bazel.build/reference/glossary#target).

### Rule

A rule defines the attributes that are available for defining a target of a certain type 
(e.g. a C++ library or Golang executable).

It also specifies a rule implementation that is used to expand the declarative rule
into [Actions](#Action).

[Rule Bazel Docs](https://bazel.build/reference/glossary#rule)

### Action

An action defines which command to execute with which input files and what outputs to expect.

E.g. compiling an object file with the command `gcc -o hello.o -c hello.c`, the input file `hello.c`
and the expected output `hello.o`.

### Provider

A [rule](#Rule) implementation typically has to consume information from other rules, e.g.:

- the paths of required inputs which might have been produced by another rule,
- the names of libraries to link to the final binary,
- ...

Rules can access information from other targets they depend on by the "providers" they return.

Essentially, a provider is a struct of some fields that contain things like file paths. A
rule can return any number of providers which are then referenced by provider name from
dependent rules.

It is essentially the API of a target towards other rules.

### Toolchains

Rule implementations often allow customizability of the actions they execute
through config settings and toolchains.

Toolchains selection typically depends on platform constraints and settings
specified for a build [configuration](#Configuration).

### Configuration

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

### Label

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
