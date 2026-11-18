use serde_json::{Value, json};
use squadron_extension_api::forge::{
    self, BranchArgs, Input, NoArgs, NumberArgs, PostArgs, ReplyArgs, UpdateArgs,
};
use squadron_extension_api::{Account, Extension, squadron_extension};

mod api;
mod pulls;
mod reads;
mod writes;

use reads::Repo;

struct GitHub;

fn repo<A: Default + serde::de::DeserializeOwned>(input: &str) -> Result<(Repo, A), String> {
    let asked: Input<A> = forge::input(input)?;
    Ok((Repo::new(&asked.repo, &asked.groups)?, asked.args))
}

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

    fn forge(call: String, input: String) -> Result<String, String> {
        match call.as_str() {
            "pull" => {
                let (repo, args) = repo::<BranchArgs>(&input)?;
                forge::answer(&reads::pull(&repo, &args.branch)?)
            }
            "numbered" => {
                let (repo, args) = repo::<NumberArgs>(&input)?;
                forge::answer(&reads::numbered(&repo, args.number)?)
            }
            "lists" => {
                let (repo, _) = repo::<NoArgs>(&input)?;
                forge::answer(&reads::lists(&repo)?)
            }
            "remarks" => {
                let (repo, args) = repo::<NumberArgs>(&input)?;
                forge::answer(&reads::remarks(&repo, args.number)?)
            }
            "me" => forge::answer(&reads::me(&api::account()?)?),
            "runs" => {
                let (repo, _) = repo::<NoArgs>(&input)?;
                forge::answer(&reads::runs(&repo)?)
            }
            "reply" => {
                let (repo, args) = repo::<ReplyArgs>(&input)?;
                writes::reply(&repo, &args)?;
                forge::answer(&Value::Null)
            }
            "post" => {
                let (repo, args) = repo::<PostArgs>(&input)?;
                forge::answer(&writes::post(&repo, &args))
            }
            "merge" => {
                let (repo, args) = repo::<NumberArgs>(&input)?;
                writes::merge(&repo, args.number)?;
                forge::answer(&Value::Null)
            }
            "update" => {
                let (repo, args) = repo::<UpdateArgs>(&input)?;
                writes::update(&repo, &args)?;
                forge::answer(&Value::Null)
            }
            _ => Err(forge::unsupported(&call)),
        }
    }
}

squadron_extension!(GitHub);
