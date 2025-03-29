---
title: Source Filesystem Change detection
description: A simple source passthrough VFS can be used for fast change detection or very large source repositories.
---

Meta/Google both famously have large monorepos that wouldn't fit on the disk of their developers anymore.
By accessing the code through a custom-made VFS, the developers only need to transfer the source that
they interact with to their workstations.

Zack is not aimed at repositories of that size. But it should handle large source repositories that
can be stored on a typical Laptop and already reach a size where sequential file tree traversal is slow.

To speed up change detection, we should use OS dependent techniques to detect changes fast. E.g. on Linux, this is
not easy. `inotify` does not work well with larger directory structures since one needs to watch all sub directories.
`fanotify` on the other end cannot be constrained to a single directory.

Possible techniques include:

- Layering a passthrough VFS on top the source directory and constraining `fanotify` to the whole directory.
- Implementing a FUSE passthrough VFS that instruments all updates.
