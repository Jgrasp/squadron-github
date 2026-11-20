use crate::reads::Repo;
use crate::{api, pulls};
use serde_json::{Map, Value, json};
use squadron_extension_api::forge::{
    self, CreateArgs, Draft, MergeArgs, PostArgs, Posted, Pull, ReplyArgs, ResolveArgs, UpdateArgs,
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

pub fn merge_method(picked: Option<&str>, default: &Value, allowed: &[String]) -> String {
    if let Some(picked) = picked {
        return picked.into();
    }
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
    let data = match args.method {
        Some(_) => Value::Null,
        None => api::graphql(
            &repo.via,
            "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { viewerDefaultMergeMethod pullRequest(number: $number) { repository { mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed } baseRef { rules(first: 50) { nodes { type parameters { ... on PullRequestParameters { allowedMergeMethods } } } } } } } }",
            json!({ "owner": repo.owner, "name": repo.name, "number": args.number }),
        )?,
    };
    let repository = &data["repository"];
    let method = merge_method(
        args.method.as_deref(),
        &repository["viewerDefaultMergeMethod"],
        &pulls::merge_methods(&repository["pullRequest"]),
    );
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

fn known_refusal(error: &str) -> bool {
    error.contains("A pull request already exists") || error.contains("No commits between")
}

pub fn draft_refused(error: &str) -> bool {
    error.contains("422") && error.to_lowercase().contains("draft") && !known_refusal(error)
}

pub fn creation_failure(error: String, args: &CreateArgs) -> String {
    let (branch, base) = (&args.branch, &args.base);
    if error.contains("A pull request already exists") {
        return forge::failure(
            None,
            json!({
                "en": format!("A pull request is already open for {branch}"),
                "fr": format!("Une pull request est déjà ouverte pour {branch}"),
            }),
            None,
        );
    }
    if error.contains("No commits between") {
        return forge::failure(
            None,
            json!({
                "en": format!("{branch} has no commit that {base} lacks"),
                "fr": format!("{branch} n’a aucun commit absent de {base}"),
            }),
            Some(json!({
                "en": "Push commits on the branch, or pick another base.",
                "fr": "Poussez des commits sur la branche, ou choisissez une autre base.",
            })),
        );
    }
    error
}

fn created(found: &Value) -> Pull {
    Pull {
        number: found["number"].as_u64().unwrap_or_default(),
        title: found["title"].as_str().unwrap_or_default().into(),
        description: found["body"].as_str().unwrap_or_default().into(),
        url: found["html_url"].as_str().unwrap_or_default().into(),
        state: "open".into(),
        draft: found["draft"] == true,
        branch: found["head"]["ref"].as_str().unwrap_or_default().into(),
        base: found["base"]["ref"].as_str().unwrap_or_default().into(),
        ..Pull::default()
    }
}

pub fn create(repo: &Repo, args: &CreateArgs) -> Result<Pull, String> {
    let path = format!("/repos/{}/pulls", repo.slug());
    let mut body = json!({ "head": args.branch, "base": args.base, "title": args.title, "body": args.description, "draft": args.draft });
    let found = match api::send(&repo.via, "POST", &path, &body) {
        Err(error) if args.draft && draft_refused(&error) => {
            body["draft"] = json!(false);
            api::send(&repo.via, "POST", &path, &body)
        }
        sent => sent,
    }
    .map_err(|error| creation_failure(error, args))?;
    let Some(number) = found["number"].as_u64() else {
        return Err(format!(
            "GitHub opened no pull request for {}.",
            args.branch
        ));
    };
    Ok(crate::reads::numbered(repo, number).map_or_else(|_| created(&found), |n| n.pull))
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
        assert_eq!(merge_method(None, &json!("SQUASH"), &all), "squash");
        assert_eq!(merge_method(None, &json!("REBASE"), &all), "rebase");
        assert_eq!(merge_method(None, &json!("MERGE"), &all), "merge");
        assert_eq!(merge_method(None, &Value::Null, &all), "merge");
        assert_eq!(
            merge_method(None, &json!("REBASE"), &allowed(&["merge", "squash"])),
            "merge"
        );
        assert_eq!(
            merge_method(None, &json!("MERGE"), &allowed(&["squash", "rebase"])),
            "squash"
        );
        assert_eq!(merge_method(None, &json!("REBASE"), &[]), "rebase");
        let picked: squadron_extension_api::forge::Input<MergeArgs> =
            squadron_extension_api::forge::input(r#"{"args":{"number":7,"method":"squash"}}"#)
                .unwrap();
        assert_eq!(
            merge_method(picked.args.method.as_deref(), &json!("MERGE"), &all),
            "squash"
        );
    }

    #[test]
    fn tells_why_github_opened_no_pull_request() {
        let args = CreateArgs {
            branch: "feat/draft-mode".into(),
            base: "main".into(),
            draft: true,
            ..CreateArgs::default()
        };
        let exists = "GitHub answered 422: Validation Failed: A pull request already exists for Jgrasp:feat/draft-mode.";
        let empty =
            "GitHub answered 422: Validation Failed: No commits between main and feat/draft-mode";
        let draft = "GitHub answered 422: Validation Failed: Draft pull requests are not supported in this repository.";
        assert!(!draft_refused(exists) && !draft_refused(empty) && draft_refused(draft));
        assert!(!draft_refused(
            "GitHub answered 403: Resource not accessible: draft"
        ));
        let failure: Value = serde_json::from_str(&creation_failure(exists.into(), &args)).unwrap();
        assert_eq!(
            failure["title"]["en"],
            "A pull request is already open for feat/draft-mode"
        );
        assert_eq!(
            failure["title"]["fr"],
            "Une pull request est déjà ouverte pour feat/draft-mode"
        );
        let failure: Value = serde_json::from_str(&creation_failure(empty.into(), &args)).unwrap();
        assert_eq!(
            failure["title"]["en"],
            "feat/draft-mode has no commit that main lacks"
        );
        assert!(failure["detail"]["fr"].is_string());
        let other = "GitHub answered 404: Not Found";
        assert_eq!(creation_failure(other.into(), &args), other);
    }

    #[test]
    fn answers_the_created_pull_from_the_rest_answer() {
        let found = json!({ "number": 9, "title": "Export", "body": null, "html_url": "https://github.com/a/b/pull/9", "draft": true, "head": { "ref": "feat/x" }, "base": { "ref": "main" } });
        let pull = created(&found);
        assert_eq!(
            (pull.number, pull.state.as_str(), pull.draft),
            (9, "open", true)
        );
        assert_eq!(
            (
                pull.branch.as_str(),
                pull.base.as_str(),
                pull.description.as_str()
            ),
            ("feat/x", "main", "")
        );
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
