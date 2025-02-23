---
title: Starlark
description: Starlark as build config language.
---

[Starlark](https://github.com/bazelbuild/starlark) is a programming
language that was purpose-built for built systems. The 
embeddable [rust starlark](https://github.com/facebook/starlark-rust)
implementation that is also used in [buck2](https://buck2.build/)
has a couple of very strong points:

- Nice API to lazily load/execute modules.
- Fast.
- Types.
- Clear impact on changed files. (no mutable global state)
- Language server integration.

Compared to a non-programmable config language, it allows us
more flexible runtime configuration with less boilerplate.
