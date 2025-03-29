---
title: Output VFS
description: A virtual file system for build artifacts can facilitate on-demand fetching.
---

Bazel introduced an [Output Protocol](https://blog.bazel.build/2024/07/23/remote-output-service.html#:~:text=The%20Bazel%20Output%20Service%20protocol%20also%20provides%20methods%20for%20computing,operations%20Bazel%20needs%20to%20perform.) in 7.2 -- mostly to download output artifacts only on
demand. That is very useful for setups where developers use a central distributed cache.

Another use case might be to generate certain artifacts only when needed. E.g. providing the files
that an IDE needs for understanding the project structure on demand on access.
