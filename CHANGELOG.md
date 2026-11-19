# Changelog

## 0.2.0 · 2026-11-19

- GitHub Actions runs name the pull request they check, so Squadron shows its number and title on the run and files it under PR checks

## 0.1.1 · 2026-11-19

- GitHub Actions runs show again for projects that go through the GitHub CLI
- Merging a pull request uses a method the repository and its base branch rules allow, when your default one is not allowed

## 0.1.0 · 2026-11-18

- Pull requests of your GitHub projects in Squadron: the pull request of each agent's branch, the ones to review, yours and the others, with their checks, reviewers and conflicts
- Reviews with line comments, a summary and a verdict, replies to remarks, resolved threads shown and resolved from Squadron, merges with the repository's default method, and title and description changes
- GitHub Actions runs of each project, with a notification when one ends
- Chips for GitHub pull request links in conversations
- One GitHub account per owner, with a fine-grained token checked before it is saved; without one, the GitHub CLI is used
