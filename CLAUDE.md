# squadron-github

The `github` extension of Squadron: GitHub pull requests and Actions runs, out of Squadron's core. `docs/PLAN.md` tells how it left the core.

This session works on this repo only. A problem in the Squadron app (`../squadron`) or in `squadron-extensions` (API, guide, registry) goes to an agent on that project through `start_agent`, as the devkit rules say: never fix it, run it or publish to it from here.

Load the `squadron-devkit:extension` skill before touching `extension.json` or the Rust code.

```bash
cargo build --release --target wasm32-wasip2
cargo test
```
