use crate::api;
use crate::pulls::{self, FIELDS, Found};
use serde_json::{Value, json};
use squadron_extension_api::forge::{Lists, Numbered, Person, Pull, Remark, Run};
use std::collections::BTreeMap;

pub struct Repo {
    pub via: api::Via,
    pub owner: String,
    pub name: String,
}

pub fn owner_and_name(
    repo: &str,
    groups: &BTreeMap<String, String>,
) -> Result<(String, String), String> {
    match (groups.get("owner"), groups.get("name")) {
        (Some(owner), Some(name)) => Ok((owner.clone(), name.clone())),
        _ => repo
            .split_once('/')
            .map(|(o, n)| (o.to_string(), n.to_string()))
            .ok_or_else(|| format!("{repo} is not a GitHub repository.")),
    }
}

impl Repo {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    fn graphql(&self, query: &str, mut variables: Value) -> Result<Value, String> {
        if query.contains("$owner") {
            variables["owner"] = json!(self.owner);
            variables["name"] = json!(self.name);
        }
        api::graphql(&self.via, &format!("{query}\n{FIELDS}"), variables)
    }

    fn pages(
        &self,
        query: &str,
        variables: Value,
        at: &[&str],
    ) -> Result<(Vec<Value>, Value), String> {
        let mut found = Vec::new();
        let mut first = Value::Null;
        let mut variables = variables;
        while found.len() < MOST {
            let data = self.graphql(query, variables.clone())?;
            let connection = at.iter().fold(&data, |v, key| &v[*key]);
            found.extend(nodes(connection));
            let next = connection["pageInfo"]["endCursor"]
                .as_str()
                .map(str::to_string);
            let more = connection["pageInfo"]["hasNextPage"] == true;
            if first.is_null() {
                first = data;
            }
            match next.filter(|_| more) {
                Some(cursor) => variables["after"] = json!(cursor),
                None => break,
            }
        }
        found.truncate(MOST);
        Ok((found, first))
    }
}

const MOST: usize = 50;

const SEARCH: &str = "query($q: String!, $after: String) {
  viewer { login }
  search(query: $q, type: ISSUE, first: 20, after: $after) { nodes { ...pr } pageInfo { hasNextPage endCursor } }
}";

const OPEN: &str = "query($owner: String!, $name: String!, $after: String) {
  repository(owner: $owner, name: $name) {
    pullRequests(states: OPEN, first: 20, after: $after, orderBy: { field: UPDATED_AT, direction: DESC }) { nodes { ...pr } pageInfo { hasNextPage endCursor } }
  }
}";

fn nodes(value: &Value) -> Vec<Value> {
    value["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|n| n.is_object() && !n.as_object().is_some_and(|o| o.is_empty()))
        .cloned()
        .collect()
}

pub fn pull(repo: &Repo, branch: &str) -> Result<Option<Pull>, String> {
    let data = repo.graphql(
        "query($owner: String!, $name: String!, $branch: String!) {
  viewer { login }
  repository(owner: $owner, name: $name) {
    pullRequests(headRefName: $branch, first: 1, states: [OPEN, CLOSED, MERGED], orderBy: { field: UPDATED_AT, direction: DESC }) { nodes { ...pr } }
  }
}",
        json!({ "branch": branch }),
    )?;
    let me = data["viewer"]["login"].as_str().unwrap_or_default();
    Ok(nodes(&data["repository"]["pullRequests"])
        .first()
        .map(|p| pulls::pull_of(p, me)))
}

pub fn numbered(repo: &Repo, number: u64) -> Result<Numbered, String> {
    let data = repo.graphql(
        "query($owner: String!, $name: String!, $number: Int!) {
  viewer { login }
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) { ...pr additions deletions changedFiles }
  }
}",
        json!({ "number": number }),
    )?;
    let found = &data["repository"]["pullRequest"];
    if found.is_null() {
        return Err(format!("{} has no pull request #{number}.", repo.slug()));
    }
    let me = data["viewer"]["login"].as_str().unwrap_or_default();
    Ok(Numbered {
        pull: pulls::pull_of(found, me),
        additions: found["additions"].as_u64(),
        deletions: found["deletions"].as_u64(),
        files: found["changedFiles"].as_u64(),
    })
}

pub fn lists(repo: &Repo) -> Result<Lists, String> {
    let scope = format!("repo:{} is:pr is:open", repo.slug());
    let search = |filter: &str| {
        repo.pages(
            SEARCH,
            json!({ "q": format!("{scope} {filter}") }),
            &["search"],
        )
    };
    let (requested, data) = search("review-requested:@me")?;
    let me = data["viewer"]["login"].as_str().unwrap_or_default();
    let (open, _) = repo.pages(OPEN, json!({}), &["repository", "pullRequests"])?;
    let (reviewed, _) = search("reviewed-by:@me -author:@me")?;
    let (authored, _) = search("author:@me")?;
    let mut lists = pulls::lists(Found {
        me,
        requested: &requested,
        reviewed: &reviewed,
        authored: &authored,
        open: &open,
    });
    for pull in lists.review.iter_mut().filter(|p| p.reviewed.is_some()) {
        let path = format!("/repos/{}/pulls/{}/reviews", repo.slug(), pull.number);
        pull.reviewed_head = api::every(&repo.via, &path)
            .ok()
            .and_then(|reviews| pulls::last_review_head(&reviews, me));
    }
    Ok(lists)
}

pub fn remark_of(found: &Value, kind: &str) -> Remark {
    let at = if kind == "reviews" {
        &found["submitted_at"]
    } else {
        &found["created_at"]
    };
    Remark {
        id: found["id"].as_u64().unwrap_or_default().to_string(),
        author: found["user"]["login"].as_str().unwrap_or_default().into(),
        avatar: found["user"]["avatar_url"]
            .as_str()
            .unwrap_or_default()
            .into(),
        body: found["body"].as_str().unwrap_or_default().into(),
        path: found["path"].as_str().map(str::to_string),
        line: found["line"].as_u64().map(|n| n as u32),
        removed: found["side"] == "LEFT",
        reply_to: found["in_reply_to_id"].as_u64().map(|n| n.to_string()),
        at: at.as_str().unwrap_or_default().into(),
        kind: if kind == "pulls" { "line" } else { "general" }.into(),
        resolved: None,
        commit: found["original_commit_id"].as_str().map(str::to_string),
    }
}

const THREADS: &str = "query($owner: String!, $name: String!, $number: Int!, $after: String) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { reviewThreads(first: 100, after: $after) { pageInfo { hasNextPage endCursor } nodes { id isResolved comments(first: 1) { nodes { databaseId } } } } } } }";

pub struct Thread {
    pub id: String,
    pub first: String,
    pub resolved: bool,
}

pub fn threads_of(page: &Value) -> Vec<Thread> {
    page["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| {
            Some(Thread {
                id: t["id"].as_str()?.to_string(),
                first: t["comments"]["nodes"][0]["databaseId"]
                    .as_u64()?
                    .to_string(),
                resolved: t["isResolved"] == true,
            })
        })
        .collect()
}

pub fn threads(repo: &Repo, number: u64) -> Result<Vec<Thread>, String> {
    let mut found = Vec::new();
    let mut after = Value::Null;
    for _ in 0..10 {
        let data = api::graphql(
            &repo.via,
            THREADS,
            json!({ "owner": repo.owner, "name": repo.name, "number": number, "after": after }),
        )?;
        let page = &data["repository"]["pullRequest"]["reviewThreads"];
        found.extend(threads_of(page));
        if page["pageInfo"]["hasNextPage"] != true {
            break;
        }
        after = page["pageInfo"]["endCursor"].clone();
    }
    Ok(found)
}

pub fn remarks(repo: &Repo, number: u64) -> Result<Vec<Remark>, String> {
    let slug = repo.slug();
    let mut found = Vec::new();
    for (kind, path) in [
        ("pulls", format!("/repos/{slug}/pulls/{number}/comments")),
        ("issues", format!("/repos/{slug}/issues/{number}/comments")),
        ("reviews", format!("/repos/{slug}/pulls/{number}/reviews")),
    ] {
        found.extend(
            api::every(&repo.via, &path)?
                .iter()
                .filter(|r| kind != "reviews" || r["body"].as_str().is_some_and(|b| !b.is_empty()))
                .map(|r| remark_of(r, kind)),
        );
    }
    if let Ok(threads) = threads(repo, number) {
        for remark in found.iter_mut().filter(|r| r.kind == "line") {
            remark.resolved = threads
                .iter()
                .find(|t| t.first == remark.id)
                .map(|t| t.resolved);
        }
    }
    found.sort_by(|a, b| a.at.cmp(&b.at));
    Ok(found)
}

pub fn me(repo: &Repo) -> Result<Person, String> {
    let user = api::rest(&repo.via, "/user")?;
    Ok(Person {
        name: user["login"].as_str().unwrap_or_default().into(),
        avatar: user["avatar_url"].as_str().unwrap_or_default().into(),
    })
}

pub fn run_of(value: &Value) -> Run {
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let status = match (
        text(&value["status"]).as_str(),
        text(&value["conclusion"]).as_str(),
    ) {
        ("completed", "success") => "success",
        ("completed", "cancelled") => "cancelled",
        ("completed", "skipped" | "neutral" | "stale") => "skipped",
        ("completed", _) => "failure",
        ("in_progress", _) => "running",
        _ => "queued",
    };
    let started = Some(text(&value["run_started_at"]))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| text(&value["created_at"]));
    Run {
        id: value["id"].as_u64().unwrap_or_default().to_string(),
        number: value["run_number"].as_u64().unwrap_or_default(),
        name: text(&value["name"]),
        title: text(&value["display_title"]),
        event: text(&value["event"]),
        branch: text(&value["head_branch"]),
        status: status.into(),
        url: text(&value["html_url"]),
        started,
        finished: (value["status"] == "completed").then(|| text(&value["updated_at"])),
        author: text(&value["actor"]["login"]),
        author_avatar: text(&value["actor"]["avatar_url"]),
    }
}

pub fn runs(repo: &Repo) -> Result<Vec<Run>, String> {
    let list = api::rest(
        &repo.via,
        &format!("/repos/{}/actions/runs?per_page=20", repo.slug()),
    )?;
    Ok(list["workflow_runs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(run_of)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_remarks_on_lines_and_on_the_whole_pull_request() {
        let line = json!({ "id": 1, "body": "Nit", "user": { "login": "octo", "avatar_url": "https://a/octo.png" }, "path": "a.ts", "line": 5, "side": "LEFT", "in_reply_to_id": null, "created_at": "t1" });
        let c = remark_of(&line, "pulls");
        assert_eq!(
            (
                c.id.as_str(),
                c.path.as_deref(),
                c.line,
                c.removed,
                c.author.as_str(),
                c.avatar.as_str(),
                c.reply_to,
                c.kind.as_str()
            ),
            (
                "1",
                Some("a.ts"),
                Some(5),
                true,
                "octo",
                "https://a/octo.png",
                None,
                "line"
            )
        );
        let answer = json!({ "id": 2, "body": "Done", "user": { "login": "ann" }, "path": "a.ts", "line": null, "side": "RIGHT", "in_reply_to_id": 1, "created_at": "t2" });
        let d = remark_of(&answer, "pulls");
        assert_eq!(
            (d.line, d.removed, d.reply_to.as_deref()),
            (None, false, Some("1"))
        );
        assert_eq!(c.commit, None);
        let review = json!({ "id": 9, "body": "Looks good", "user": { "login": "ann" }, "submitted_at": "t3", "commit_id": "abc" });
        let r = remark_of(&review, "reviews");
        assert_eq!(
            (r.kind.as_str(), r.at.as_str(), r.path),
            ("general", "t3", None)
        );
        let comment =
            json!({ "id": 7, "body": "Thanks", "user": { "login": "bob" }, "created_at": "t4" });
        assert_eq!(remark_of(&comment, "issues").kind, "general");
    }

    #[test]
    fn sums_up_runs() {
        let run = run_of(
            &json!({"id": 7, "run_number": 14, "name": "Release", "display_title": "Squadron 0.10.0", "event": "push", "head_branch": "main", "status": "completed", "conclusion": "timed_out", "html_url": "u", "created_at": "a", "run_started_at": "b", "updated_at": "c", "actor": {"login": "Jgrasp", "avatar_url": "https://avatars.githubusercontent.com/u/1"}}),
        );
        assert_eq!(
            (
                run.id.as_str(),
                run.number,
                run.status.as_str(),
                run.started.as_str(),
                run.finished.as_deref()
            ),
            ("7", 14, "failure", "b", Some("c"))
        );
        assert_eq!(
            (run.author.as_str(), run.author_avatar.as_str()),
            ("Jgrasp", "https://avatars.githubusercontent.com/u/1")
        );
        let run = run_of(&json!({"status": "waiting", "created_at": "a", "run_started_at": null}));
        assert_eq!(
            (run.status.as_str(), run.started.as_str(), run.finished),
            ("queued", "a", None)
        );
        let run = run_of(&json!({"status": "in_progress"}));
        assert_eq!(run.status, "running");
        let run =
            run_of(&json!({"status": "completed", "conclusion": "skipped", "updated_at": "z"}));
        assert_eq!(
            (run.status.as_str(), run.finished.as_deref()),
            ("skipped", Some("z"))
        );
    }

    #[test]
    fn drops_search_results_that_are_not_pull_requests() {
        let found = nodes(&json!({ "nodes": [{ "number": 1 }, {}, null] }));
        assert_eq!(found, [json!({ "number": 1 })]);
    }

    #[test]
    fn reads_which_threads_are_resolved() {
        let page = json!({ "nodes": [
            { "id": "PRRT_a", "isResolved": true, "comments": { "nodes": [{ "databaseId": 1 }] } },
            { "id": "PRRT_b", "isResolved": false, "comments": { "nodes": [{ "databaseId": 4 }] } },
            { "id": "PRRT_c", "isResolved": false, "comments": { "nodes": [] } }
        ] });
        let found: Vec<_> = threads_of(&page)
            .into_iter()
            .map(|t| (t.id, t.first, t.resolved))
            .collect();
        assert_eq!(
            found,
            [
                ("PRRT_a".to_string(), "1".to_string(), true),
                ("PRRT_b".to_string(), "4".to_string(), false)
            ]
        );
        let line = json!({ "id": 1, "body": "Nit", "user": { "login": "octo" }, "path": "a.ts", "line": 5, "original_commit_id": "a1b2c3d", "created_at": "t1" });
        assert_eq!(remark_of(&line, "pulls").commit.as_deref(), Some("a1b2c3d"));
    }
}
