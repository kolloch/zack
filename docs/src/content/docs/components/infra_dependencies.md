---
title: Infrastructure Dependencies
description: Zack should support dependencies on Infrastructure or Build Support daemons.
---

Infrastructure dependencies are poorly supported in build systems. What
kind of infrastructure dependencies do I mean here?

Examples:

1. A database running in a docker container for integration testing.
2. A database provisioned in a cloud environment.
3. A compiler worker e.g. for compilers for Java / TypeScript using JITing VMS like JVM/NodeJS.
4. Throttled access to a cloud provider for integration testing support.

## Integration Points

*Network Endpoints*: Often, providing one or more network endpoints to the already provisioned resource
would be enough. These could be provided by environment variables, config files, or
arguments to the build action.

*Files*: Compilers typically assume local file access to the source files and their dependencies.

## Sharing

While best isolation between tests is achieved by using a pristine, new database instance for
every test, this is seldom the best trade off. If e.g. each test uses a different logical database
served by the same database service instance, isolation is typically good enough.

Thus, the same same database service instance can be used for multiple tests sequentially or even
in parallel.

It is typical of infrastructure dependencies to support concurrent use and good enough isolation.

For compiler workers, avoiding the costly restarts is even the point of it all.

## Pooling

Infrastructure resources can be provisioned independently before executing tasks that depend
on them, e.g. in parallel to compiling the integration tests or their dependencies.
That shaves some startup time from infrastructure tests. 

Also, they don't have to be torn down immediately after they are used but returned to a pool.

It might be reasonable, to cap the total number
of instances, though.

## Cleanup

When the infrastructure dependency is retired, some cleanup might need to run.

E.g. deleting the cloud database instance, killing a docker container and cleaning state on disk.

For failing tests, it is also good to have means to prevent or defer that cleanup so that
one can inspect the database state of the failing test.

## Dependencies

Infra dependencies themselves might depend on other build targets.

## Test Containers

[Test Containers](https://testcontainers.com/) are a popular approach for using infrastructure
dependencies in integration tests. Typically, they require a docker instance and provide
a library to start the infrastructure dependencies as part of the tests.

That is not ideal and we can do better. But it would be great if we somehow integrated these
libraries with our infrastructure dependencies more deeply, so infra dependencies
can be pooled and shared.

## Related

[ninja pool](https://ninja-build.org/manual.html#ref_pool) allows to limit parallelism for
certain resources but providing these resources is outside of the scope of Ninja.
