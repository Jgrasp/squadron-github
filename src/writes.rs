use crate::api;
use crate::reads::Repo;
use serde_json::{Map, Value, json};
use squadron_extension_api::forge::{Draft, PostArgs, Posted, ReplyArgs, ResolveArgs, UpdateArgs};

pub fn reply(repo: &Repo, args: &ReplyArgs) -> Result<(), String> {
    let path = if args.kind == "line" {
        format!(
            "/repos/{}/pulls/{}/comments/{}/replies",
            repo.slug(),
            args.number,
            args.to
        )
    } else {
        format!("/repos/{}/issues/{}/comments", repo.slug(), args.number)
    };
    api::send(&repo.account, "POST", &path, &json!({ "body": args.body })).map(|_| ())
}

pub fn resolve(repo: &Repo, args: &ResolveArgs) -> Result<(), String> {
    if args.kind != "line" {
        return Err("GitHub only resolves threads on a line.".into());
    }
    let thread = crate::reads::threads(repo, args.number)?
        .into_iter()
        .find(|t| t.first == args.to)
        .ok_or_else(|| {
            format!(
                "GitHub has no thread starting with the comment {}.",
                args.to
            )
        })?;
    let mutation = if args.resolved {
        "mutation($id: ID!) { resolveReviewThread(input: { threadId: $id }) { thread { isResolved } } }"
    } else {
        "mutation($id: ID!) { unresolveReviewThread(input: { threadId: $id }) { thread { isResolved } } }"
    };
    api::graphql(&repo.account, mutation, json!({ "id": thread.id })).map(|_| ())
}

pub fn review(args: &PostArgs) -> Option<Value> {
    let comments: Vec<Value> = args
        .drafts
        .iter()
        .filter(|d| !d.path.is_empty())
        .map(|d| json!({ "path": d.path, "line": d.line, "side": if d.removed { "LEFT" } else { "RIGHT" }, "body": d.body }))
        .collect();
    let general = args
        .drafts
        .iter()
        .filter(|d| d.path.is_empty())
        .map(|d: &Draft| d.body.as_str());
    let body = std::iter::once(args.summary.as_str())
        .chain(general)
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let event = match args.verdict.as_str() {
        "approve" => "APPROVE",
        "request_changes" => "REQUEST_CHANGES",
        _ => "COMMENT",
    };
    if event == "COMMENT" && comments.is_empty() && body.is_empty() {
        return None;
    }
    Some(json!({ "commit_id": args.head, "event": event, "body": body, "comments": comments }))
}

pub fn post(repo: &Repo, args: &PostArgs) -> Posted {
    let Some(review) = review(args) else {
        return Posted {
            done: 0,
            said: true,
            error: None,
        };
    };
    let path = format!("/repos/{}/pulls/{}/reviews", repo.slug(), args.number);
    match api::send(&repo.account, "POST", &path, &review) {
        Ok(_) => Posted {
            done: args.drafts.len(),
            said: true,
            error: None,
        },
        Err(error) => Posted {
            done: 0,
            said: false,
            error: Some(api::plain(error)),
        },
    }
}

pub fn merge_method(default: &str) -> &'static str {
    match default {
        "SQUASH" => "squash",
        "REBASE" => "rebase",
        _ => "merge",
    }
}

pub fn merge(repo: &Repo, number: u64) -> Result<(), String> {
    let data = api::graphql(
        &repo.account,
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { viewerDefaultMergeMethod } }",
        json!({ "owner": repo.owner, "name": repo.name }),
    )?;
    let method = merge_method(
        data["repository"]["viewerDefaultMergeMethod"]
            .as_str()
            .unwrap_or_default(),
    );
    let path = format!("/repos/{}/pulls/{number}/merge", repo.slug());
    api::send(
        &repo.account,
        "PUT",
        &path,
        &json!({ "merge_method": method }),
    )
    .map(|_| ())
}

pub fn changes(args: &UpdateArgs) -> Value {
    let mut changes = Map::new();
    if let Some(title) = &args.title {
        changes.insert("title".into(), json!(title));
    }
    if let Some(description) = &args.description {
        changes.insert("body".into(), json!(description));
    }
    Value::Object(changes)
}

pub fn update(repo: &Repo, args: &UpdateArgs) -> Result<(), String> {
    let path = format!("/repos/{}/pulls/{}", repo.slug(), args.number);
    api::send(&repo.account, "PATCH", &path, &changes(args)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(path: &str, line: u32, removed: bool, body: &str) -> Draft {
        Draft {
            path: path.into(),
            line,
            removed,
            body: body.into(),
        }
    }

    #[test]
    fn writes_one_review_with_line_comments_summary_and_verdict() {
        let args = PostArgs {
            number: 12,
            head: "abc123".into(),
            drafts: vec![
                draft("src/a.rs", 4, false, "Rename"),
                draft("src/a.rs", 9, true, "Why remove this?"),
                draft("", 0, false, "Add a test."),
            ],
            summary: "Close.".into(),
            verdict: "request_changes".into(),
        };
        assert_eq!(
            review(&args).unwrap(),
            json!({
                "commit_id": "abc123", "event": "REQUEST_CHANGES", "body": "Close.\n\nAdd a test.",
                "comments": [
                    { "path": "src/a.rs", "line": 4, "side": "RIGHT", "body": "Rename" },
                    { "path": "src/a.rs", "line": 9, "side": "LEFT", "body": "Why remove this?" }
                ]
            })
        );
        let approve = PostArgs {
            verdict: "approve".into(),
            head: "h".into(),
            ..PostArgs::default()
        };
        assert_eq!(review(&approve).unwrap()["event"], "APPROVE");
        for verdict in ["comment", ""] {
            let empty = PostArgs {
                verdict: verdict.into(),
                summary: "  ".into(),
                ..PostArgs::default()
            };
            assert_eq!(review(&empty), None);
        }
    }

    #[test]
    fn merges_with_the_repository_default_method() {
        assert_eq!(merge_method("SQUASH"), "squash");
        assert_eq!(merge_method("REBASE"), "rebase");
        assert_eq!(merge_method("MERGE"), "merge");
        assert_eq!(merge_method(""), "merge");
    }

    #[test]
    fn sends_the_description_as_the_body() {
        let both = UpdateArgs {
            number: 3,
            title: Some("Export".into()),
            description: Some("Adds export".into()),
        };
        assert_eq!(
            changes(&both),
            json!({ "title": "Export", "body": "Adds export" })
        );
        let title = UpdateArgs {
            number: 3,
            title: Some("Export".into()),
            description: None,
        };
        assert_eq!(changes(&title), json!({ "title": "Export" }));
    }
}
