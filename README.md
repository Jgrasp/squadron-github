# GitHub

A [Squadron](https://github.com/Jgrasp/squadron) extension that brings the pull requests and GitHub Actions runs of your GitHub projects into Squadron.

## Use

In Settings › Extensions › GitHub, add a [personal access token](https://github.com/settings/tokens/new?scopes=repo,read:org,workflow&description=Squadron) with the `repo`, `read:org` and `workflow` scopes. **Test the connection** checks it before you save it.

Every project whose `origin` is on `github.com` then gets, in Squadron:

- the pull request of each agent's branch, and chips for the pull request links in conversations;
- the pull requests to review, yours and the others, with their checks, reviewers and conflicts;
- reviews with line comments, a summary and a verdict, replies to remarks, merges with the repository's default method, and title and description changes, from you or from an agent;
- the GitHub Actions runs, with a notification when one ends.

## What it asks Squadron for

- HTTPS requests to `api.github.com`, GitHub's API, and nothing else.
- A GitHub account: the token stays in the system keyring. Squadron adds it to the requests to GitHub, and the extension never reads it.
  - `repo` reads and writes the pull requests, their comments and reviews, and merges them.
  - `read:org` shows the teams asked to review.
  - `workflow` lets a merge go through when the pull request changes a workflow file.

## Build

```sh
rustup target add wasm32-wasip2
cargo build --release --target wasm32-wasip2
cargo test
```
