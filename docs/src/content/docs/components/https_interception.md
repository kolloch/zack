---
title: HTTPS Interception
description: Detect downloads by an HTTPS man-in-the-middle proxy.
---

Most language-specific package managers retrieve both metadata as the
actual package payload by HTTPS GET requests. By intercepting
these requests and potentially replaying them later, we could make 
them work without external network requests. If wanted, we can 
write a lock file.


## Resources

- Rust crate [http_mitm_proxy](https://docs.rs/http-mitm-proxy/0.14.0/http_mitm_proxy/index.html)
- https://mitmproxy.org/
