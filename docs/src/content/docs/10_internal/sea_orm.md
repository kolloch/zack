---
title: Sea ORM
description: How to install Sea ORM CLI.
---

## Sea ORM CLI install

(must use async-std, even though we use tokio)

```
cargo install --no-default-features --features runtime-async-std-rustls,codegen,cli,async-std sea-orm-cli
```

## Regenerate entities after schema changes

```bash
sea-orm-cli generate entity -o zack/src/entity
```