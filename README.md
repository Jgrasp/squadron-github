# GitHub

A [Squadron](https://github.com/Jgrasp/squadron) extension that brings the pull requests and GitHub Actions runs of your GitHub projects into Squadron.

## Use

In Settings › Extensions › GitHub, add a [personal access token](https://github.com/settings/tokens/new?scopes=repo,read:org,workflow&description=Squadron) with the `repo`, `read:org` and `workflow` scopes. **Test the connection** checks it before you save it.

## What it asks Squadron for

- HTTPS requests to `api.github.com`, GitHub's API.

The token stays in the system keyring. Squadron adds it to the requests to GitHub, and the extension never reads it.

## Build

```sh
rustup target add wasm32-wasip2
cargo build --release --target wasm32-wasip2
cargo test
```
