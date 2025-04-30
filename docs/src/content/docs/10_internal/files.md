---
title: Source/Build files
description: How to store source / build files.
---

## Goals

- Unique paths, also when building the same artifacts for multiple platforms.

Nice to have:

- Paths the same as in IDE to make debugging easier.

## Options

### Bazel Style

For file/tree artifacts directly referenced by //$module_path:$file_path,
Bazel distinguishes between:

- Source: path unchanged $module_path/$file_path
- Generated file / binary: bazel-out/${config_name}/$module_path/$file_path

When referencing generated files/binary files as dependencies, the
config is the same as the one of the referring target -- unless
there is a transition. In both cases, the label is config-less.

That works, because dependency attributes either have one transition or none
and all the dependencies in that attribute are treated the same.

Within the action/sandbox, the paths need the config path prefix, though.
So through different attributes/transitions, one target can reference
artifacts of different configurations. Referring exec/target configurations
in one target is common, e.g.:

- compile/runtime from exec config
- library dependencies from target config

To translate between labels and the paths, Bazel provides "macros" that
lookup the kind (source/bin) and config automatically.

That means that the actual paths in the action change if the dependency
changes from being a source file to a generated file. That also potentially
breaks relative paths between generated and source files and/or makes
them really long (since they need to first need to ../ their way back to the root).

Pros:

- source paths remain unchanged and errors reported in them with relative paths
  will work in the IDE

Cons:

- relative paths between generated files and source files might not work or be
  weird. If the language eco-system allows to specify multiple source
  paths as search paths, that might work, otherwise annoying.

### Merged style (Brainstorm)

To unify source files/binary files one can do at least two things:

1. One could "merge" source and binary files into the config-specific build directory:
   - sources used in multiple configs would be copied/hard-linked (not a problem).
   - if there is an error then the error would be highlighted in the copy, not the actual source,
     potentially breaking IDE integration.

2. One could "merge" source and binary files into the source directory. This is only
   possible with one build config -- e.g. one could chose the "default" build config.
   That means that one would need to change the source directory to at least add symlinks
   to the generated files. As long as the other tooling supports following symlinks,
   that might be usable to "fake" the default setup.

3. Allow "manual" merging. One could supply an explicit option to symlink back an output
   in the source tree. One could explicitly chose the config here, defaulting to target/exec/host.

   In that case, the same symlink would exist in the sandbox so that relative paths work.