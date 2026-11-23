use serde_json::Value;
use squadron_extension_api::forge::{Lists, Pull, Reviewer};

pub const FIELDS: &str = "fragment pr on PullRequest {
  number title body url state isDraft reviewDecision
  latestReviews(first: 50) { nodes { state submittedAt author { login } } }
  reviewRequests(first: 50) { nodes { requestedReviewer { __typename ... on User { login name } ... on Mannequin { login } ... on Team { name } } } }
  comments { totalCount }
  commits(last: 1) { nodes { commit { statusCheckRollup { contexts(first: 100) { nodes { __typename ... on CheckRun { status conclusion } ... on StatusContext { state } } } } } } }
  baseRefName headRefName headRefOid
  author { login }
  updatedAt mergeable mergeStateStatus
  repository { mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed }
  headRepository { deleteBranchOnMerge viewerPermission }
  baseRef { rules(first: 50) { nodes { type parameters { ... on PullRequestParameters { allowedMergeMethods } } } } }
  headRef { refUpdateRule { allowsDeletions } rules(first: 50) { nodes { type } } }
}";

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}

fn nodes(value: &Value) -> &[Value] {
    value["nodes"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

pub fn avatar(login: &str) -> String {
    format!("https://avatars.githubusercontent.com/{login}?s=64")
}

fn checks(found: &Value) -> Option<String> {
    let contexts = found["commits"]["nodes"]
        .get(0)
        .map(|n| nodes(&n["commit"]["statusCheckRollup"]["contexts"]))
        .unwrap_or_default();
    let states: Vec<&str> = contexts
        .iter()
        .map(|c| {
            let state = [&c["conclusion"], &c["state"], &c["status"]]
                .into_iter()
                .filter_map(Value::as_str)
                .find(|s| !s.is_empty());
            match state {
                Some("SUCCESS" | "NEUTRAL" | "SKIPPED") => "pass",
                Some(
                    "FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED"
                    | "STARTUP_FAILURE",
                ) => "fail",
                _ => "pending",
            }
        })
        .collect();
    if states.is_empty() {
        None
    } else if states.contains(&"fail") {
        Some("failing".into())
    } else if states.contains(&"pending") {
        Some("pending".into())
    } else {
        Some("passing".into())
    }
}

fn verdict(state: &Value) -> Option<&'static str> {
    match state.as_str() {
        Some("APPROVED") => Some("approved"),
        Some("CHANGES_REQUESTED") => Some("changes_requested"),
        Some("COMMENTED") => Some("commented"),
        _ => None,
    }
}

fn reviewers(found: &Value) -> Vec<Reviewer> {
    let mut list: Vec<Reviewer> = Vec::new();
    for review in nodes(&found["latestReviews"]) {
        let Some(verdict) = verdict(&review["state"]) else {
            continue;
        };
        let login = text(&review["author"]["login"]);
        list.push(Reviewer {
            avatar: avatar(&login),
            name: login,
            verdict: verdict.into(),
        });
    }
    for asked in nodes(&found["reviewRequests"]) {
        let who = &asked["requestedReviewer"];
        let login = who["login"].as_str().unwrap_or_default();
        let name = who["name"]
            .as_str()
            .filter(|n| !n.is_empty())
            .unwrap_or(login);
        if name.is_empty() {
            continue;
        }
        list.retain(|r| r.name != login);
        list.push(Reviewer {
            name: name.into(),
            avatar: if login.is_empty() {
                String::new()
            } else {
                avatar(login)
            },
            verdict: "waiting".into(),
        });
    }
    list
}

pub fn requested(found: &Value, me: &str) -> bool {
    nodes(&found["reviewRequests"])
        .iter()
        .any(|r| r["requestedReviewer"]["login"] == me)
}

pub fn merge_methods(found: &Value) -> Vec<String> {
    let rules = nodes(&found["baseRef"]["rules"]);
    [
        ("merge", "mergeCommitAllowed", "MERGE"),
        ("squash", "squashMergeAllowed", "SQUASH"),
        ("rebase", "rebaseMergeAllowed", "REBASE"),
    ]
    .into_iter()
    .filter(|(_, flag, name)| {
        found["repository"][*flag] == true
            && rules
                .iter()
                .filter(|rule| rule["type"] == "PULL_REQUEST")
                .all(|rule| {
                    rule["parameters"]["allowedMergeMethods"]
                        .as_array()
                        .is_none_or(|methods| methods.iter().any(|m| m == name))
                })
    })
    .map(|(method, ..)| method.to_string())
    .collect()
}

fn branch_protected(found: &Value) -> bool {
    let head = &found["headRef"];
    !head["refUpdateRule"].is_null()
        || nodes(&head["rules"])
            .iter()
            .any(|r| r["type"] == "DELETION")
}

fn delete_branch(found: &Value) -> Option<bool> {
    let head = &found["headRepository"];
    let writer = matches!(
        head["viewerPermission"].as_str(),
        Some("ADMIN" | "MAINTAIN" | "WRITE")
    );
    head["deleteBranchOnMerge"]
        .as_bool()
        .map(|on| on && writer && !branch_protected(found))
}

pub fn pull_of(found: &Value, me: &str) -> Pull {
    let reviews = nodes(&found["latestReviews"]);
    let mine = reviews
        .iter()
        .find(|r| !me.is_empty() && r["author"]["login"] == me);
    let login = found["author"]["login"].as_str();
    Pull {
        number: found["number"].as_u64().unwrap_or_default(),
        title: text(&found["title"]),
        description: text(&found["body"]),
        url: text(&found["url"]),
        state: text(&found["state"]).to_lowercase(),
        draft: found["isDraft"] == true,
        approvals: reviews.iter().filter(|r| r["state"] == "APPROVED").count() as u32,
        changes_requested: found["reviewDecision"] == "CHANGES_REQUESTED",
        comments: found["comments"]["totalCount"].as_u64().unwrap_or_default() as u32,
        checks: checks(found),
        branch: text(&found["headRefName"]),
        base: text(&found["baseRefName"]),
        head: text(&found["headRefOid"]),
        author: login.unwrap_or_default().to_string(),
        author_avatar: login.map(avatar).unwrap_or_default(),
        updated: text(&found["updatedAt"]),
        reviewed: mine.and_then(|r| verdict(&r["state"])).map(str::to_string),
        reviewed_head: None,
        reviewed_at: mine
            .and_then(|r| r["submittedAt"].as_str())
            .map(str::to_string),
        head_at: None,
        reviewers: reviewers(found),
        conflicts: found["mergeable"] == "CONFLICTING",
        mergeable: found["state"] == "OPEN"
            && found["isDraft"] != true
            && matches!(
                found["mergeStateStatus"].as_str(),
                Some("CLEAN" | "HAS_HOOKS" | "UNSTABLE")
            ),
        merge_methods: merge_methods(found),
        delete_branch: delete_branch(found),
        delete_branch_settable: false,
        branch_protected: branch_protected(found),
    }
}

pub struct Found<'a> {
    pub me: &'a str,
    pub requested: &'a [Value],
    pub reviewed: &'a [Value],
    pub authored: &'a [Value],
    pub open: &'a [Value],
}

pub fn lists(found: Found) -> Lists {
    let me = found.me;
    let has = |list: &[Pull], node: &Value| {
        list.iter()
            .any(|p| Some(p.number) == node["number"].as_u64())
    };
    let asked = |node: &Value| {
        let mut pull = pull_of(node, me);
        if requested(node, me) {
            pull.reviewed = None;
        }
        pull
    };
    let mut review: Vec<Pull> = found.requested.iter().map(asked).collect();
    for node in found.open.iter().filter(|p| requested(p, me)) {
        if !has(&review, node) {
            review.push(asked(node));
        }
    }
    for node in found.reviewed {
        if !has(&review, node) {
            review.push(pull_of(node, me));
        }
    }
    let others = found
        .open
        .iter()
        .filter(|p| p["author"]["login"] != me && !has(&review, p))
        .map(|p| pull_of(p, me))
        .collect();
    let mut mine: Vec<Pull> = found.authored.iter().map(|p| pull_of(p, me)).collect();
    for node in found.open.iter().filter(|p| p["author"]["login"] == me) {
        if !has(&mine, node) {
            mine.push(pull_of(node, me));
        }
    }
    Lists {
        review,
        mine,
        others,
    }
}

pub fn last_review_head(reviews: &[Value], me: &str) -> Option<String> {
    reviews
        .iter()
        .rev()
        .find(|r| r["user"]["login"] == me)
        .and_then(|r| r["commit_id"].as_str())
        .filter(|sha| !sha.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn exported() -> Value {
        json!({
            "number": 12, "title": "Export", "body": "Adds export", "url": "https://github.com/a/b/pull/12", "state": "MERGED", "isDraft": true,
            "reviewDecision": "APPROVED",
            "latestReviews": { "nodes": [
                { "state": "APPROVED", "submittedAt": "2026-10-22T10:00:00Z", "author": { "login": "ann" } },
                { "state": "COMMENTED", "submittedAt": "2026-10-23T09:00:00Z", "author": { "login": "octo" } }
            ] },
            "comments": { "totalCount": 2 },
            "commits": { "nodes": [{ "commit": { "statusCheckRollup": { "contexts": { "nodes": [
                { "__typename": "CheckRun", "status": "COMPLETED", "conclusion": "SUCCESS" },
                { "__typename": "StatusContext", "state": "FAILURE" }
            ] } } } }] },
            "baseRefName": "main", "headRefName": "feat/export", "headRefOid": "abc123", "author": { "login": "octo" }, "updatedAt": "2026-10-23T10:00:00Z"
        })
    }

    #[test]
    fn sums_up_a_pull_request() {
        let found = exported();
        let pull = pull_of(&found, "octo");
        assert_eq!(
            (
                pull.branch.as_str(),
                pull.author.as_str(),
                pull.reviewed.as_deref(),
                pull.reviewed_at.as_deref()
            ),
            (
                "feat/export",
                "octo",
                Some("commented"),
                Some("2026-10-23T09:00:00Z")
            )
        );
        assert_eq!(
            pull.author_avatar,
            "https://avatars.githubusercontent.com/octo?s=64"
        );
        assert_eq!(pull_of(&found, "ann").reviewed.as_deref(), Some("approved"));
        assert_eq!(pull_of(&found, "").reviewed, None);
        assert_eq!(
            (
                pull.state.as_str(),
                pull.draft,
                pull.approvals,
                pull.changes_requested,
                pull.comments
            ),
            ("merged", true, 1, false, 2)
        );
        assert_eq!(
            (
                pull.checks.as_deref(),
                pull.base.as_str(),
                pull.head.as_str(),
                pull.description.as_str()
            ),
            (Some("failing"), "main", "abc123", "Adds export")
        );
        assert!(!pull.mergeable);

        let running = json!({ "commits": { "nodes": [{ "commit": { "statusCheckRollup": { "contexts": { "nodes": [{ "status": "IN_PROGRESS", "conclusion": null }] } } } }] } });
        assert_eq!(pull_of(&running, "x").checks.as_deref(), Some("pending"));
        let unchecked =
            json!({ "commits": { "nodes": [{ "commit": { "statusCheckRollup": null } }] } });
        assert_eq!(pull_of(&unchecked, "x").checks, None);
    }

    #[test]
    fn lists_reviewers_and_who_is_still_asked() {
        let asked = json!({
            "latestReviews": { "nodes": [{ "state": "APPROVED", "author": { "login": "ann" } }, { "state": "CHANGES_REQUESTED", "author": { "login": "bob" } }, { "state": "PENDING", "author": { "login": "eve" } }] },
            "reviewRequests": { "nodes": [
                { "requestedReviewer": { "__typename": "User", "login": "bob", "name": "Bob B." } },
                { "requestedReviewer": { "__typename": "Team", "name": "core" } }
            ] },
            "reviewDecision": "CHANGES_REQUESTED",
            "mergeable": "CONFLICTING"
        });
        let pull = pull_of(&asked, "");
        let seen: Vec<_> = pull
            .reviewers
            .iter()
            .map(|r| (r.name.as_str(), r.verdict.as_str(), r.avatar.is_empty()))
            .collect();
        assert_eq!(
            seen,
            [
                ("ann", "approved", false),
                ("Bob B.", "waiting", false),
                ("core", "waiting", true)
            ]
        );
        assert!(pull.conflicts && pull.changes_requested);
        assert!(requested(&asked, "bob"));
        assert!(!requested(&asked, "ann"));
    }

    #[test]
    fn reads_a_pull_request_as_github_sends_it() {
        let found: Value = serde_json::from_str(r#"{"number": 15094, "title": "docs: note that events may be delivered more than once", "body": "<!--\nThank you for contributing to GitHu", "url": "https://github.com/cli/cli/pull/15094", "state": "OPEN", "isDraft": false, "reviewDecision": "REVIEW_REQUIRED", "latestReviews": {"nodes": [{"state": "COMMENTED", "submittedAt": "2026-11-18T12:12:35Z", "author": {"login": "copilot-pull-request-reviewer"}}]}, "reviewRequests": {"nodes": [{"requestedReviewer": null}, {"requestedReviewer": {"__typename": "User", "login": "niik", "name": "Markus Olsson"}}]}, "comments": {"totalCount": 0}, "commits": {"nodes": [{"commit": {"statusCheckRollup": {"contexts": {"nodes": [{"__typename": "CheckRun", "status": "COMPLETED", "conclusion": "SUCCESS"}, {"__typename": "CheckRun", "status": "COMPLETED", "conclusion": "SUCCESS"}]}}}}]}, "baseRefName": "trunk", "headRefName": "babakks/event-listen-dup-note", "headRefOid": "b11a0fb4cf95369b4460010a9cf4bbcdf432ffc1", "author": {"login": "babakks"}, "updatedAt": "2026-11-18T12:12:35Z", "mergeable": "MERGEABLE", "mergeStateStatus": "BLOCKED"}"#).unwrap();
        let pull = pull_of(&found, "niik");
        assert_eq!(
            (
                pull.number,
                pull.state.as_str(),
                pull.checks.as_deref(),
                pull.mergeable,
                pull.conflicts,
                pull.reviewed
            ),
            (15094, "open", Some("passing"), false, false, None)
        );
        let seen: Vec<_> = pull
            .reviewers
            .iter()
            .map(|r| (r.name.as_str(), r.verdict.as_str()))
            .collect();
        assert_eq!(
            seen,
            [
                ("copilot-pull-request-reviewer", "commented"),
                ("Markus Olsson", "waiting")
            ]
        );
        assert!(requested(&found, "niik"));
    }

    #[test]
    fn merges_only_a_clean_open_pull_request() {
        let clean = json!({ "state": "OPEN", "isDraft": false, "mergeStateStatus": "CLEAN" });
        assert!(pull_of(&clean, "").mergeable);
        let blocked = json!({ "state": "OPEN", "isDraft": false, "mergeStateStatus": "BLOCKED" });
        assert!(!pull_of(&blocked, "").mergeable);
        let draft = json!({ "state": "OPEN", "isDraft": true, "mergeStateStatus": "CLEAN" });
        assert!(!pull_of(&draft, "").mergeable);
    }

    #[test]
    fn tells_the_merge_methods_and_whether_the_branch_goes() {
        let found = json!({
            "repository": { "mergeCommitAllowed": true, "squashMergeAllowed": true, "rebaseMergeAllowed": false },
            "headRepository": { "deleteBranchOnMerge": true, "viewerPermission": "WRITE" },
            "baseRef": { "rules": { "nodes": [
                { "type": "DELETION", "parameters": null },
                { "type": "PULL_REQUEST", "parameters": { "allowedMergeMethods": ["SQUASH", "REBASE"] } }
            ] } },
            "headRef": { "refUpdateRule": null, "rules": { "nodes": [] } }
        });
        let pull = pull_of(&found, "");
        assert_eq!(pull.merge_methods, ["squash"]);
        assert_eq!(
            (
                pull.delete_branch,
                pull.delete_branch_settable,
                pull.branch_protected
            ),
            (Some(true), false, false)
        );

        let mut kept = found.clone();
        kept["headRef"]["rules"]["nodes"] = json!([{ "type": "DELETION" }]);
        assert_eq!(
            (
                pull_of(&kept, "").delete_branch,
                pull_of(&kept, "").branch_protected
            ),
            (Some(false), true)
        );
        kept["headRef"] =
            json!({ "refUpdateRule": { "allowsDeletions": false }, "rules": { "nodes": [] } });
        assert!(pull_of(&kept, "").branch_protected);

        let mut fork = found.clone();
        fork["headRepository"]["viewerPermission"] = json!("READ");
        assert_eq!(pull_of(&fork, "").delete_branch, Some(false));

        let unknown = pull_of(&json!({ "number": 3 }), "");
        assert!(
            unknown.merge_methods.is_empty()
                && unknown.delete_branch.is_none()
                && !unknown.branch_protected
        );
    }

    fn open(number: u64, author: &str, asked: &[&str]) -> Value {
        let asked: Vec<Value> = asked
            .iter()
            .map(|login| json!({ "requestedReviewer": { "__typename": "User", "login": login } }))
            .collect();
        json!({
            "number": number, "state": "OPEN", "author": { "login": author },
            "reviewRequests": { "nodes": asked },
            "latestReviews": { "nodes": [{ "state": "COMMENTED", "author": { "login": "me" } }] }
        })
    }

    #[test]
    fn sorts_pull_requests_into_the_three_lists_despite_a_lagging_search() {
        let found = [
            open(1, "ann", &["me"]),
            open(2, "ann", &["me"]),
            open(3, "bob", &[]),
            open(4, "me", &[]),
            open(5, "me", &[]),
            open(6, "eve", &[]),
        ];
        let lists = lists(Found {
            me: "me",
            requested: &[found[0].clone()],
            reviewed: &[found[2].clone(), found[0].clone()],
            authored: &[found[3].clone()],
            open: &found,
        });
        let numbers = |list: &[Pull]| list.iter().map(|p| p.number).collect::<Vec<_>>();
        assert_eq!(numbers(&lists.review), [1, 2, 3]);
        assert_eq!(numbers(&lists.mine), [4, 5]);
        assert_eq!(numbers(&lists.others), [6]);
        assert_eq!(
            lists
                .review
                .iter()
                .map(|p| p.reviewed.as_deref())
                .collect::<Vec<_>>(),
            [None, None, Some("commented")]
        );
    }

    #[test]
    fn keeps_my_verdict_on_a_pull_request_the_search_still_lists_as_requested() {
        let mut approved = open(7, "ann", &[]);
        approved["latestReviews"]["nodes"][0]["state"] = json!("APPROVED");
        let lists = lists(Found {
            me: "me",
            requested: &[approved.clone()],
            reviewed: &[],
            authored: &[],
            open: &[approved],
        });
        assert_eq!(
            lists
                .review
                .iter()
                .map(|p| (p.number, p.reviewed.as_deref()))
                .collect::<Vec<_>>(),
            [(7, Some("approved"))]
        );
        assert!(lists.others.is_empty());
    }

    #[test]
    fn finds_the_commit_of_the_last_review() {
        let reviews = [
            json!({ "user": { "login": "me" }, "commit_id": "aaa" }),
            json!({ "user": { "login": "ann" }, "commit_id": "bbb" }),
            json!({ "user": { "login": "me" }, "commit_id": "ccc" }),
            json!({ "user": { "login": "ann" }, "commit_id": "ddd" }),
        ];
        assert_eq!(last_review_head(&reviews, "me").as_deref(), Some("ccc"));
        assert_eq!(last_review_head(&reviews, "eve"), None);
    }
}
