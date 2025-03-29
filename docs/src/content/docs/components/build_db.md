---
title: Build Database
description: Storing the Build Graph, and other Build metadata in a DB.
---

Using a build database at the core of the build system, 

1. speeds up cold starts,
2. allows simpler concurrent access.

## Constructing/Updating the Build Graph

Build systems typically read the source file tree, in particular build files,
and construct a build graph from there. They typically start from scratch when restarted.

This leads to long startup times e.g. of Bazel in a typical CI setup where 
each build starts in a "fresh" Docker container. Workarounds are using advanced snapshot
restore techniques.

Using database technology could solve these issue. If we store the build graph and other
meta information in a build database, we can narrowly only access the data in it
that is affected by the detected changes.

## Concurrent Access

Build systems that hold their build graph in-memory have a hard time allowing concurrent
access to it. E.g. while Bazel is running a build or query, no other build or query can be
executed on the same Bazel instance. That is a serious limitation. While waiting for
a bigger build, a developer cannot e.g. use IDE assist features that utilize the query
feature underneath. Or kick of a unit test run while the integration test is executing.

With a typical RDBMS and transactions, it should be possible to access the build graph
without interfering with the build. It should also make scheduling additional targets
easier -- while certainly not trivial: Some additional coordination is most likely
needed.
