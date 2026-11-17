# squadron-github

The `github` extension of Squadron: GitHub pull requests and Actions runs, moved out of Squadron's core. Read `docs/PLAN.md` before anything: the work spans this repo, `../squadron` and `../squadron-extensions`.

Load the `squadron-devkit:extension` skill before touching `extension.json` or the Rust code.

```bash
cargo build --release --target wasm32-wasip2
cargo test
```
