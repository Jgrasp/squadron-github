use serde_json::{Value, json};
use squadron_extension_api::forge;
use squadron_extension_api::{Request, Response, host};

const API: &str = "https://api.github.com";
const PAGES: usize = 10;

fn request(method: &str, url: &str, account: &str, body: Option<String>) -> Request {
    let url = if url.starts_with("https://") {
        url.to_string()
    } else {
        format!("{API}{url}")
    };
    Request {
        method: method.into(),
        url,
        headers: vec![
            (
                "Authorization".into(),
                format!("Bearer {{{{{account}.token}}}}"),
            ),
            ("Accept".into(), "application/vnd.github+json".into()),
            ("X-GitHub-Api-Version".into(), "2022-11-28".into()),
            ("User-Agent".into(), "squadron-github".into()),
        ],
        body,
    }
}

pub fn get(account: &str, path: &str) -> Result<Response, String> {
    host::fetch(&request("GET", path, account, None))
}

pub fn accounts() -> Result<Vec<String>, String> {
    let found: Vec<String> = host::accounts().into_iter().map(|a| a.id).collect();
    if found.is_empty() {
        return Err(forge::no_account(json!({
            "en": "Add a GitHub account in Settings › Extensions › GitHub.",
            "fr": "Ajoutez un compte GitHub dans Réglages › Extensions › GitHub.",
        })));
    }
    Ok(found)
}

pub fn denied(error: &str) -> bool {
    [
        "\"code\":\"account\"",
        "answered 403",
        "answered 404",
        "Could not resolve to a Repository",
        "forbids access",
        "Resource not accessible",
    ]
    .iter()
    .any(|mark| error.contains(mark))
}

pub fn ordered(mut accounts: Vec<String>, known: Option<String>) -> Vec<String> {
    if let Some(i) = known.and_then(|k| accounts.iter().position(|a| *a == k)) {
        let first = accounts.remove(i);
        accounts.insert(0, first);
    }
    accounts
}

pub fn refused() -> String {
    forge::no_account(json!({
        "en": "GitHub refused the token: add a new one in Settings › Extensions › GitHub.",
        "fr": "GitHub a refusé le jeton : ajoutez-en un nouveau dans Réglages › Extensions › GitHub.",
    }))
}

pub fn answer(response: Response) -> Result<Value, String> {
    if response.status == 401 {
        return Err(refused());
    }
    let body: Value = serde_json::from_str(&response.body).unwrap_or(Value::Null);
    if !(200..300).contains(&response.status) {
        return Err(failure(response.status, &body, &response.body));
    }
    Ok(body)
}

pub fn failure(status: u16, body: &Value, raw: &str) -> String {
    let reason = body["message"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| raw.chars().take(200).collect());
    let details: Vec<&str> = body["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["message"].as_str().or(e.as_str()))
        .collect();
    if details.is_empty() {
        format!("GitHub answered {status}: {reason}")
    } else {
        format!("GitHub answered {status}: {reason}: {}", details.join("; "))
    }
}

pub fn rest(account: &str, path: &str) -> Result<Value, String> {
    answer(get(account, path)?)
}

pub fn send(account: &str, method: &str, path: &str, body: &Value) -> Result<Value, String> {
    answer(host::fetch(&request(
        method,
        path,
        account,
        Some(body.to_string()),
    ))?)
}

pub fn plain(error: String) -> String {
    match serde_json::from_str::<Value>(&error) {
        Ok(found) => found["title"]["en"]
            .as_str()
            .or(found["title"].as_str())
            .map(str::to_string)
            .unwrap_or(error),
        Err(_) => error,
    }
}

pub fn next_page(headers: &[(String, String)]) -> Option<String> {
    let link = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("link"))?;
    link.1.split(',').find_map(|part| {
        let (url, rel) = part.split_once(';')?;
        rel.contains("rel=\"next\"").then(|| {
            url.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string()
        })
    })
}

pub fn every(account: &str, path: &str) -> Result<Vec<Value>, String> {
    let sep = if path.contains('?') { '&' } else { '?' };
    let mut url = format!("{path}{sep}per_page=100");
    let mut found = Vec::new();
    for _ in 0..PAGES {
        let response = get(account, &url)?;
        let next = next_page(&response.headers);
        found.extend(answer(response)?.as_array().cloned().unwrap_or_default());
        match next {
            Some(next) if next.starts_with(API) => url = next,
            _ => break,
        }
    }
    Ok(found)
}

pub fn graphql(account: &str, query: &str, variables: Value) -> Result<Value, String> {
    let body = json!({ "query": query, "variables": variables }).to_string();
    let found = answer(host::fetch(&request(
        "POST",
        "/graphql",
        account,
        Some(body),
    ))?)?;
    graphql_data(found)
}

pub fn graphql_data(mut found: Value) -> Result<Value, String> {
    let data = &found["data"];
    let errors: Vec<&str> = found["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| match e["path"].get(0).and_then(Value::as_str) {
            Some(top) => data[top].is_null(),
            None => true,
        })
        .filter_map(|e| e["message"].as_str())
        .collect();
    if !errors.is_empty() || data.is_null() {
        return Err(format!("GitHub answered: {}", errors.join("; ")));
    }
    Ok(found["data"].take())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tries_the_account_that_worked_for_the_owner_first() {
        let all = vec!["a1".to_string(), "a2".to_string(), "a3".to_string()];
        assert_eq!(ordered(all.clone(), Some("a3".into())), ["a3", "a1", "a2"]);
        assert_eq!(
            ordered(all.clone(), Some("gone".into())),
            ["a1", "a2", "a3"]
        );
        assert_eq!(ordered(all, None), ["a1", "a2", "a3"]);
    }

    #[test]
    fn tells_a_refused_repository_from_another_failure() {
        assert!(denied(&refused()));
        assert!(denied(
            "GitHub answered 403: `Steel-Shed-Solutions` forbids access via a personal access token (classic)."
        ));
        assert!(denied(
            "GitHub answered: Could not resolve to a Repository with the name 'a/b'."
        ));
        assert!(denied("GitHub answered 404: Not Found"));
        assert!(!denied("GitHub answered 422: Unprocessable Entity"));
        assert!(!denied("acme/shop has no pull request #3."));
    }

    #[test]
    fn follows_the_next_page_of_a_link_header() {
        let headers = vec![(
            "Link".to_string(),
            r#"<https://api.github.com/repositories/1/pulls/2/comments?per_page=100&page=2>; rel="next", <https://api.github.com/repositories/1/pulls/2/comments?per_page=100&page=5>; rel="last""#.to_string(),
        )];
        assert_eq!(
            next_page(&headers).as_deref(),
            Some("https://api.github.com/repositories/1/pulls/2/comments?per_page=100&page=2")
        );
        assert_eq!(next_page(&[]), None);
    }

    #[test]
    fn explains_why_github_refused() {
        let body = json!({"message": "Unprocessable Entity", "errors": ["Pull request review thread line must be part of the diff"]});
        assert_eq!(
            failure(422, &body, ""),
            "GitHub answered 422: Unprocessable Entity: Pull request review thread line must be part of the diff"
        );
        assert_eq!(
            failure(404, &json!({"message": "Not Found"}), ""),
            "GitHub answered 404: Not Found"
        );
        let refused: Value = serde_json::from_str(&refused()).unwrap();
        assert_eq!(refused["code"], "account");
    }

    #[test]
    fn keeps_graphql_data_and_reports_errors_without_it() {
        assert_eq!(
            graphql_data(json!({"data": {"viewer": {"login": "octo"}}})).unwrap()["viewer"]["login"],
            "octo"
        );
        assert_eq!(
            graphql_data(
                json!({"data": null, "errors": [{"message": "Could not resolve to a Repository"}]})
            )
            .unwrap_err(),
            "GitHub answered: Could not resolve to a Repository"
        );
        let hidden: Value = serde_json::from_str(r#"{"data":{"viewer":{"login":"Jgrasp"},"repository":null},"errors":[{"type":"NOT_FOUND","path":["repository"],"locations":[{"line":3,"column":3}],"message":"Could not resolve to a Repository with the name 'Jgrasp/does-not-exist'."}]}"#).unwrap();
        assert_eq!(
            graphql_data(hidden).unwrap_err(),
            "GitHub answered: Could not resolve to a Repository with the name 'Jgrasp/does-not-exist'."
        );
        let partial = json!({"data": {"repository": {"pullRequest": {"number": 3}}}, "errors": [{"path": ["repository", "pullRequest", "reviewRequests"], "message": "Resource not accessible"}]});
        assert_eq!(
            graphql_data(partial).unwrap()["repository"]["pullRequest"]["number"],
            3
        );
    }
}
