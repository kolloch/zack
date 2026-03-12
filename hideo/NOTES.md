# Hideo – Engineering Notes

## Environment

| Property | Value |
|---|---|
| Host OS | macOS (OrbStack VM) |
| Kernel | `6.17.8-orbstack-00308-g8f9c941121b1` |
| User | `ubuntu` (uid 1000, no effective capabilities by default) |
| Workspace filesystem | **virtiofs** – OrbStack mounts the macOS project directory into the Linux VM via VirtIO-fs |

Check with:

```sh
findmnt -T /workspaces/zack
# TARGET           SOURCE                                 FSTYPE   OPTIONS
# /workspaces/zack mac[/Users/.../projects/zack] virtiofs rw,relatime
```

---

## Finding 1 – fanotify does NOT work on virtiofs

### Symptoms

`fanotify_init` and `fanotify_mark(FAN_MARK_MOUNT)` both **succeed** against a virtiofs path with no error.
However, **no events are ever delivered**:

```
[1] ✓  filesystem type  —  virtiofs (path: /workspaces/zack)
[2] ✓  fanotify_init    —  fd opened
[3] ✓  fanotify_mark    —  FAN_MARK_MOUNT on /workspaces/zack
[4] ✓  write probe file —  /workspaces/zack/fanotify_probe_tmp.txt
  read_events error: ENOENT: No such file or directory
[5] ✓  events received  —  OPEN=0  CLOSE_WRITE=0  CLOSE_NOWRITE=0
[6] ✗  verdict          —  FAIL — missing events: OPEN, CLOSE_WRITE
```

The spurious `ENOENT` from `read_events` is also notable: `poll(2)` reported `POLLIN` on the fanotify
fd (meaning the kernel thought something was readable), but the subsequent `read(2)` returned
`ENOENT`. This suggests the kernel internally created a fanotify event record but could not attach a
valid file descriptor to it – likely because the virtiofs layer does not expose inodes in a way the
fanotify notification subsystem can reference.

### Root cause

virtiofs is a FUSE-based filesystem. The Linux fanotify subsystem operates at the VFS layer and
requires that the underlying filesystem supports the `fsnotify` hooks. FUSE filesystems historically
either do not implement these hooks or implement them incompletely. The result is that `fanotify_mark`
does not fail (the VFS mount record is still valid), but file-operation events are never fired.

This is a **kernel limitation**, not a capability or permission problem.

---

## Finding 2 – fanotify works perfectly on tmpfs

Running the same probe against a freshly mounted `tmpfs` produces all expected events:

```
[INFO]  Mounting tmpfs at /tmp/.tmpGMVa1f
[INFO]  tmpfs mounted successfully
[INFO]  fanotify initialised
[INFO]  fanotify watching mount at /tmp/.tmpGMVa1f
[INFO]  Writing test file: /tmp/.tmpGMVa1f/probe.txt
[INFO]  EVENT  CLOSE_WRITE   pid=…  path=probe.txt
[INFO]  EVENT  OPEN          pid=…  path=probe.txt
[INFO]  EVENT  CLOSE_NOWRITE pid=…  path=probe.txt
[INFO]  Results: OPEN=1  CLOSE_WRITE=1  CLOSE_NOWRITE=1
✓ PASS – fanotify detected writes and opens on tmpfs
```

This confirms the fanotify API itself, the kernel configuration, and the runtime permissions are all
fine. The failure is specific to virtiofs.

---

## Finding 3 – `mount(2)` requires elevated privileges

Without `sudo` the process has **zero effective capabilities** (`CapEff: 0000000000000000`), so any
call to `mount(2)` – whether for tmpfs or overlayfs – returns `EPERM` immediately.

```
[INFO]  Mounting tmpfs at /tmp/.tmp8u842f
[ERROR] mounting tmpfs: Operation not permitted (os error 1)
```

`SYS_ADMIN` is present in the **bounding set** (`CapBnd: 000001ffffffffff`), so `sudo` grants full
root and resolves this. The longer-term fix is the user-namespace approach already sketched in
`main.rs` (`unshare(CLONE_NEWUSER)` + uid/gid map writes), which grants `CAP_SYS_ADMIN` inside the
new namespace without requiring `sudo`.

---

## Implication for Milestone 1

The original design mounts an **overlayfs** over the virtiofs workspace and then attaches fanotify to
the overlay mount. Based on the findings above:

- Attaching fanotify to the **virtiofs lower layer** would be silent (Finding 1).
- Attaching fanotify to the **overlay mount itself** (which is a kernel-native filesystem) should
  work, because the overlay mount is a real in-kernel VFS mount, not a FUSE mount. The overlay
  filesystem does implement `fsnotify` hooks.
- The critical question therefore becomes: can overlayfs be mounted successfully in this environment?
  That is gated on the user-namespace / `SYS_ADMIN` question (Finding 3), not on virtiofs.

### Recommended path forward

1. Confirm `unshare(CLONE_NEWUSER)` succeeds (or use `sudo` for experiments).
2. Mount overlayfs with `lowerdir` = virtiofs workspace, `upper`/`work` on a **tmpfs** (not on
   virtiofs, which does not support `xattr` operations overlayfs needs).
3. Attach fanotify to the **overlay mount point** – this is a kernel-native mount and should
   generate events normally.
4. All write events will hit the tmpfs upper layer; fanotify watching the overlay mount will see them.

---

## Finding 4 – `CLONE_NEWUSER` silently breaks `fanotify_init` and `fanotify_mark`

### Symptom

With the original code flow (user namespace → mount namespace → overlay → fanotify), `fanotify_init`
returned `EPERM` even when the binary was run via `sudo`:

```
unshare(CLONE_NEWUSER)  ← succeeds, creates nested user namespace
write uid_map / gid_map ← succeeds
unshare(CLONE_NEWNS)    ← succeeds
mount overlayfs         ← succeeds
fanotify_init           ← EPERM  ✗
```

### Root cause

`fanotify_init(2)` and `fanotify_mark(2)` with `FAN_MARK_MOUNT` both call `capable(CAP_SYS_ADMIN)`
internally. `capable()` checks against the **initial user namespace**, not the caller's current user
namespace. Once a process has called `unshare(CLONE_NEWUSER)`, it moves into a new sub-namespace.
Its capabilities are valid *inside* that sub-namespace but `capable()` no longer sees them, so both
calls return `EPERM` regardless of whether the process was originally root.

This is distinct from `ns_capable()`, which checks against a specific namespace and would work.
fanotify does not use `ns_capable` for these checks.

### Fix

Remove `CLONE_NEWUSER` entirely. Use only `CLONE_NEWNS` (a private mount namespace) to keep the
overlay mounts local to the process. This requires real `CAP_SYS_ADMIN` in the initial namespace
(i.e. `sudo`), but that is acceptable for Milestone 1.

The `userxattr` overlayfs option was also removed – it was only needed for unprivileged overlay
mounts inside a user namespace. As root with a standard mount namespace the default
`trusted.overlay.*` xattrs work fine.

---

## Finding 5 – fanotify on an overlayfs mount works correctly

With the user-namespace detour removed, the full pipeline succeeds:

```
[INFO]  Workspace root: /workspaces/zack
[INFO]  Mount namespace ready
[INFO]  Overlay mounted at /tmp/.tmpXXXXXX/merged
[INFO]  Fanotify watching mount at /tmp/.tmpXXXXXX/merged

# sudo hideo bash -c "touch x"
WRITE   <pid>   x

# sudo hideo bash -c "cat Cargo.toml"
OPEN        <pid>   Cargo.toml
READ_CLOSE  <pid>   Cargo.toml
```

Key observations:

- The lower layer is virtiofs (no events there, see Finding 1), but the **overlay mount itself** is
  a kernel-native VFS mount and does support `fsnotify` hooks. Events are generated at the overlay
  layer before they are dispatched to the lower/upper layers.
- The `upper` and `work` directories live in `/tmp`, which is already a `tmpfs` mount on this
  system, so they are on a kernel-native filesystem as recommended in Finding 1.
- Path resolution via `/proc/self/fd/<n>` correctly returns paths under the overlay merged directory,
  which are then stripped to workspace-relative paths.

---

## Binaries

| Binary | Purpose |
|---|---|
| `hideo` | Main Milestone 1 binary (overlayfs + fanotify + command execution) |
| `tmpfs_experiment` | Proves fanotify works on tmpfs end-to-end |
| `fanotify_probe [PATH]` | Probes whether fanotify delivers events on the filesystem at PATH |

Run probes with `sudo`:

```sh
sudo ./target/debug/fanotify_probe /workspaces/zack   # virtiofs – expect FAIL
sudo ./target/debug/fanotify_probe /tmp               # tmpfs    – expect PASS
sudo ./target/debug/tmpfs_experiment                  # full tmpfs round-trip
sudo ./target/debug/hideo bash -c "touch x"           # overlay  – expect WRITE x
sudo ./target/debug/hideo bash -c "cat Cargo.toml"    # overlay  – expect OPEN + READ_CLOSE
```
