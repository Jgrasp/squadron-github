use serde_json::{Value, json};
use squadron_extension_api::{Account, Extension, squadron_extension};

mod api;

struct GitHub;

impl Extension for GitHub {
    fn check_account(account: Account) -> Result<String, String> {
        let me = api::get(&account.id, "/user")?;
        match me.status {
            200 => {
                let user: Value = serde_json::from_str(&me.body).unwrap_or_default();
                let login = user["login"].as_str().unwrap_or_default();
                Ok(json!({
                    "text": { "en": format!("Connected as {login}"), "fr": format!("Connecté en tant que {login}") }
                })
                .to_string())
            }
            401 => Err(json!({
                "title": { "en": "GitHub refused this token", "fr": "GitHub a refusé ce jeton" },
                "detail": { "en": "The token may be wrong, expired or revoked.", "fr": "Le jeton est peut-être faux, expiré ou révoqué." },
                "field": "token"
            })
            .to_string()),
            status => Err(format!("GitHub answered {status}.")),
        }
    }
}

squadron_extension!(GitHub);
