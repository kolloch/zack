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

### Example Test Case

File write:

```bash
cargo run --bin hideo -- bash -c "touch x"
```

Output should include something like:

```
OPEN	<pid>	x
WRITE	<pid>	x
```

File read:

```bash
cargo run --bin hideo -- bash -c "cat Cargo.toml"
```

Output should include something like:

```
OPEN	<pid>	Cargo.toml
READ_CLOSE	<pid>	Cargo.toml
```
