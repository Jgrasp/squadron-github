use crate::reads::Repo;
use crate::{api, pulls};
use serde_json::{Map, Value, json};
use squadron_extension_api::forge::{
    Draft, MergeArgs, PostArgs, Posted, ReplyArgs, ResolveArgs, UpdateArgs,
};

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
    api::send(&repo.via, "POST", &path, &json!({ "body": args.body })).map(|_| ())
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
    api::graphql(&repo.via, mutation, json!({ "id": thread.id })).map(|_| ())
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

pub fn post(repo: &Repo, args: &PostArgs) -> Result<Posted, String> {
    let Some(review) = review(args) else {
        return Ok(Posted {
            done: 0,
            said: true,
            error: None,
        });
    };
    let path = format!("/repos/{}/pulls/{}/reviews", repo.slug(), args.number);
    match api::send(&repo.via, "POST", &path, &review) {
        Ok(_) => Ok(Posted {
            done: args.drafts.len(),
            said: true,
            error: None,
        }),
        Err(error) if api::denied(&error) => Err(error),
        Err(error) => Ok(Posted {
            done: 0,
            said: false,
            error: Some(api::plain(error)),
        }),
    }
}

pub fn merge_method(default: &Value, allowed: &[String]) -> String {
    let default = match default.as_str() {
        Some("SQUASH") => "squash",
        Some("REBASE") => "rebase",
        _ => "merge",
    };
    match allowed.first() {
        Some(first) if !allowed.iter().any(|m| m == default) => first.clone(),
        _ => default.into(),
    }
}

pub fn merge(repo: &Repo, args: &MergeArgs) -> Result<(), String> {
    let method = match &args.method {
        Some(method) => method.clone(),
        None => {
            let data = api::graphql(
                &repo.via,
                "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { viewerDefaultMergeMethod pullRequest(number: $number) { repository { mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed } baseRef { rules(first: 50) { nodes { type parameters { ... on PullRequestParameters { allowedMergeMethods } } } } } } } }",
                json!({ "owner": repo.owner, "name": repo.name, "number": args.number }),
            )?;
            let repository = &data["repository"];
            merge_method(
                &repository["viewerDefaultMergeMethod"],
                &pulls::merge_methods(&repository["pullRequest"]),
            )
        }
    };
    let path = format!("/repos/{}/pulls/{}/merge", repo.slug(), args.number);
    api::send(&repo.via, "PUT", &path, &json!({ "merge_method": method })).map(|_| ())
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
    api::send(&repo.via, "PATCH", &path, &changes(args)).map(|_| ())
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
    fn merges_with_the_default_method_unless_it_is_not_allowed() {
        let allowed = |methods: &[&str]| methods.iter().map(|m| m.to_string()).collect::<Vec<_>>();
        let all = allowed(&["merge", "squash", "rebase"]);
        assert_eq!(merge_method(&json!("SQUASH"), &all), "squash");
        assert_eq!(merge_method(&json!("REBASE"), &all), "rebase");
        assert_eq!(merge_method(&json!("MERGE"), &all), "merge");
        assert_eq!(merge_method(&Value::Null, &all), "merge");
        assert_eq!(
            merge_method(&json!("REBASE"), &allowed(&["merge", "squash"])),
            "merge"
        );
        assert_eq!(
            merge_method(&json!("MERGE"), &allowed(&["squash", "rebase"])),
            "squash"
        );
        assert_eq!(merge_method(&json!("REBASE"), &[]), "rebase");
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
