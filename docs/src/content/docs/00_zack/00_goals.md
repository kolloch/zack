---
title: Zack's Goals
description: Zack's high-level goals.
---

## Zack's high-level goals.

:::note 
Zack is a collection of vague and still incoherent ideas at this point.
:::

- *Speedy Builds*: Achieve fast build times by caching and parallelizing build actions.
- *Monorepo Support*: Provide cross-language and toolchain compatibility for monorepo projects.
- *Build Tool Integration*: Integrate with popular language-specific build tools (like Cargo, pnpm, Go) while:
   - Reusing established configuration and lock files for dependencies wherever possible.
   - Ensuring existing IDE integration with these build tools continues to function for local development.
   - Make integrating a new existing build tool easy by smart instrumentation primitives.
   - [Allow shallow integration early on](/zack/00_zack/progressive_integration/).
- *Project Isolation*: Enable developers to focus solely on their subprojects without needing to consider the broader
  project context.
- *Reliability*: A build that works on a developer machine should also work in CI or on another machine.
- *Build Clarity*: Make builds easy to inspect and understand.
- *Simple CI*: Zack should be easy to integrate into existing CI systems.
