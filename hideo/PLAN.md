# Hideo

Simple advanced build step executor.

## Milestone 1: Detect reads and writes

- [x] Detect workspace root.
- [x] Remount the workspace with an overlayfs:
  - [x] Mount the workspace as read-only.
  - [x] Create a writable overlay on top of the read-only workspace.
  - [x] Register fanotify on the overlay.
- [x] Execute command from cli
  - [x] and output fanotify events.

### Running

`hideo` requires `CAP_SYS_ADMIN` in the **initial user namespace** because both
`fanotify_init(2)` and `fanotify_mark(2)` call `capable(CAP_SYS_ADMIN)` against
the initial namespace.  Run with `sudo`:

```bash
sudo cargo run --bin hideo -- bash -c "touch x"
# or against the pre-built binary:
sudo ./target/debug/hideo bash -c "touch x"
```

> **Why not user namespaces?**  The workspace is on a **virtiofs** mount (OrbStack /
> Docker Desktop).  Entering `CLONE_NEWUSER` creates a nested user namespace; all
> subsequent `capable()` checks then fail with `EPERM` even when the outer process
> was root.  We therefore use only `CLONE_NEWNS` (a private mount namespace) and
> require real root instead.

### Example Test Cases

File write:

```bash
sudo ./target/debug/hideo bash -c "touch x"
```

Expected output:

```
WRITE	<pid>	x
```

File read:

```bash
sudo ./target/debug/hideo bash -c "cat Cargo.toml"
```

Expected output:

```
OPEN	<pid>	Cargo.toml
READ_CLOSE	<pid>	Cargo.toml
```
