# Hideo – FUSE Passthrough Plan

## Architecture

Two mounts are stacked at the cwd inside a private mount namespace:

```
command's cwd  →  FUSE passthrough        (records reads + writes, forwards ops)
                       ↓  O_PATH backing fd
               overlayfs                  (isolates writes from the real workspace)
                   lower: cwd / virtiofs  (read-only view of real workspace)
                   upper: tmpfs           (all writes land here)
```

## Mount sequence

1. `unshare(CLONE_NEWUSER)` — gain `CAP_SYS_ADMIN` without sudo.
2. `unshare(CLONE_NEWNS)` — private mount namespace.
3. Create a tmpfs and make `upper/` and `work/` subdirectories inside it.
4. Mount overlayfs **at the cwd**: `lowerdir=cwd, upperdir=tmpfs/upper,
   workdir=tmpfs/work`.  The cwd now presents a writable, copy-on-write view
   of the workspace; the underlying virtiofs is untouched.
5. Open an `O_PATH` file descriptor to the cwd.  This fd points at the
   overlayfs root and is captured **before** the FUSE mount goes on top, so
   the FUSE daemon can use `openat(backing_fd, relpath, …)` to reach backing
   files without recursing through its own mount.
6. Mount FUSE **at the cwd** (on top of the overlayfs).  The cwd path now
   resolves through FUSE.
7. Spawn the command with `cwd` unchanged.  It sees its normal working
   directory; FUSE + overlayfs are invisible to it.

## What the FUSE daemon does

For every `open` request it receives from the kernel:

- Record `(pid, path, read|write)` from the FUSE request header.
- Open the same path via `openat(backing_fd, relpath, flags)` to get a real fd.
- Return the real fd to the kernel as a FUSE file handle (passthrough).

All other operations (`read`, `write`, `readdir`, `stat`, …) are forwarded
directly using the stored file handle.  The daemon itself stays small.

## Output

```
READ   <pid>   src/main.rs
WRITE  <pid>   target/debug/hideo
```

## Crate

[`fuser`](https://crates.io/crates/fuser) — Rust FUSE bindings, protocol
versions 7.8–7.34.

## Limitations

- **One context-switch per syscall.**  Acceptable for a build step.  Linux 5.17
  `FUSE_PASSTHROUGH` can bypass userspace for bulk data after the open is
  logged; deferred for now.
- **`allow_other` not needed (for now).**  The child inherits the same user
  namespace (uid 0 inside it) so it can access the FUSE mount without it.
  If we later drop privileges in the child (e.g. `setuid` to a non-root uid),
  we would need `allow_other` so the new uid can still access the mount.
  Downsides: requires `user_allow_other` in `/etc/fuse.conf` on the host, and
  broadens the access surface (mitigated by our private mount namespace).
  Deferring until privilege-dropping is implemented.
- **Symlinks escaping the mount root** need care (cap to the backing fd or deny).