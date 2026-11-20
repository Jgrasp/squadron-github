# GitHub

A [Squadron](https://github.com/Jgrasp/squadron) extension that brings the pull requests and GitHub Actions runs of your GitHub projects into Squadron.

## Use

In Settings › Extensions › GitHub, add one account per owner of your repositories, you or an organization, each with a [fine-grained personal access token](https://github.com/settings/personal-access-tokens/new) for that owner:

- Repository permissions: Pull requests and Contents in read and write, Workflows in read and write, Actions and Commit statuses in read.
- Organization permissions: Members in read, to show the teams asked to review.

A [classic token](https://github.com/settings/tokens/new?scopes=repo,read:org,workflow&description=Squadron) with the `repo`, `read:org` and `workflow` scopes works too, where the organization allows it. **Test the connection** checks a token before you save it.

Without an account, or when none reaches a repository, the extension goes through the [GitHub CLI](https://cli.github.com) once you ran `gh auth login`. Organizations that refuse classic tokens often accept it. **Use the GitHub CLI**, in the extension's options, turns this off.

For each repository, the extension remembers the account, or the CLI, that reached it.

Every project whose `origin` is on `github.com` then gets, in Squadron:

- the pull request of each agent's branch, and chips for the pull request links in conversations;
- the pull requests to review, yours and the others, with their checks, reviewers and conflicts;
- reviews with line comments, a summary and a verdict, replies to remarks, merges with the method you pick among those the repository and its branch rules allow, or your default one, and the head branch deleted when the repository's setting says so, and title and description changes, from you or from an agent;
- the GitHub Actions runs, with a notification when one ends.

## What it asks Squadron for

- HTTPS requests to `api.github.com`, GitHub's API.
- Running `gh`, the GitHub CLI: the extension only runs `gh api`, to reach GitHub with the CLI's sign in when no account can.
- GitHub accounts: each token stays in the system keyring. Squadron adds it to the requests to GitHub, and the extension never reads it.
  - Pull requests and Contents read and write the pull requests, their comments and reviews, and merge them.
  - Workflows lets a merge go through when the pull request changes a workflow file.
  - Actions and Commit statuses show the runs and the checks.
  - Members shows the teams asked to review.

## Build

```sh
rustup target add wasm32-wasip2
cargo build --release --target wasm32-wasip2
cargo test
```

CI checks the formatting of every push and pull request with `cargo fmt --check`.
