---
title: Process Suspend/Resume
description: Process snapshotting and resuming is a useful tool.
---

Being able to suspend a process and later resume it at the same point
is a powerful tool for processes that keep a lot of state in memory:

- all JITted processes typically perform best after a warmup period,
- various tools (e.g. Bazel) keep significant caches in-memory only
  and restarts are expensive.

Thus, if the ideas in [/zack/components/infra_dependencies/] don't
work, having this ability can speed things up.

## Snapshotting

The typical workflow would be to snapshot an already running process,
write the snapshot to a file.

Then, to resume the process somewhere else, we need to transfer the
snapshot file to that host and restore the process.

Typical challenges are dependencies on the environment, such as file
descriptors, memory addresses, host names, ...

## Tools

- [Firecracker MicroVM Snapshot Support](https://github.com/firecracker-microvm/firecracker/blob/main/docs/snapshotting/snapshot-support.md):
  not all solutions for docker/VMs support nested virtualization, though, e.g. [not
  on OrbStack](https://github.com/orbstack/orbstack/issues/248)
- [nested virtual machine support on Mac OS](https://github.com/orbstack/orbstack/issues/1504)
- [UTM](https://github.com/utmapp/UTM) for Virtual Machines on MacOS
- [CRIU](https://criu.org/Main_Page)
- [CRIU with Docker](https://www.redhat.com/en/blog/container-live-migration-using-runc-and-criu)
