---
title: Execution Instrumentation
description: Instrumenting the execution of e.g. a compiler allows a number of features.
---

Instrumenting e.g. a compiler invocation can reveal interesting things in a general way:

- what dependencies were truly accessed.
- how much resources where used over which time (memory, CPU, IO, ...).
- ...

Partially, this might be part of the [Execution VFS](/zack/components/execution_vfs/) or implemented by other means.

Accessed files could be used to provide something similar to Buck2s [Dep Files](https://buck2.build/docs/rule_authors/dep_files/).

Recording historical resource usage of tasks allows smarter scheduling. 

## Prior art

[shournal](https://github.com/tycho-kirchner/shournal) looks very interesting!
