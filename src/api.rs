use squadron_extension_api::{Request, Response, host};

const API: &str = "https://api.github.com";

fn request(method: &str, path: &str, account: &str, body: Option<String>) -> Request {
    Request {
        method: method.into(),
        url: format!("{API}{path}"),
        headers: vec![
            ("Authorization".into(), format!("Bearer {{{{{account}.token}}}}")),
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
