---
title: Early Direction
description: Zack is ambitious, we make things easier for us at first.
---

Zack is ambitious, we make things easier for us at first by making some
simplifications.

## Target Linux first

Zack will integrate with the OS to offer all its features. Doing this
in a cross platform way, is difficult.

As bandaid, we might offer something like easy execution via docker
and/or just use a [devcontainer](https://containers.dev/).

## Sandboxing does not have to be strict

Zack is not primarly security focussed, so a pragmatic approach
to sandboxing is fine.

## Restricted set of programming languages

An obvious intermediate milestone would be to self-build Zack.
On that way, we could create more simple examples with Rust.

## Targeting reasonably sized monorepos

Zack should be fast for typical monorepos, that is the point.

But rather than aiming at Google scale, we contend ourselves if
Zack works well for monorepos for communities / companies with
<= 100 regular contributors. Another way to think about it is
that

- the source should still fit comfortably on a developer laptop
- a full rebuild is possible on a developer laptop in a few hours
