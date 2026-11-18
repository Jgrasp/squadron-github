use serde_json::{Value, json};
use squadron_extension_api::forge::{
    self, BranchArgs, Input, NoArgs, NumberArgs, PostArgs, ReplyArgs, ResolveArgs, UpdateArgs,
};
use squadron_extension_api::{Account, Extension, host, squadron_extension};

mod api;
mod pulls;
mod reads;
mod writes;

use reads::Repo;

struct GitHub;

fn on_repo<A, T>(
    input: &str,
    work: impl Fn(&Repo, &A) -> Result<T, String>,
) -> Result<String, String>
where
    A: Default + serde::de::DeserializeOwned,
    T: serde::Serialize,
{
    let asked: Input<A> = forge::input(input)?;
    let (owner, name) = reads::owner_and_name(&asked.repo, &asked.groups)?;
    let key = format!("owner:{}", owner.to_lowercase());
    let known = host::state_get(&key);
    let mut last = String::new();
    for account in api::ordered(api::accounts()?, known.clone()) {
        let repo = Repo {
            account: account.clone(),
            owner: owner.clone(),
            name: name.clone(),
        };
        match work(&repo, &asked.args) {
            Ok(found) => {
                if known.as_deref() != Some(account.as_str()) {
                    host::state_set(&key, Some(&account));
                }
                return forge::answer(&found);
            }
            Err(error) if api::denied(&error) => last = error,
            Err(error) => return Err(error),
        }
    }
    Err(last)
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
            "pull" => on_repo(&input, |repo, args: &BranchArgs| {
                reads::pull(repo, &args.branch)
            }),
            "numbered" => on_repo(&input, |repo, args: &NumberArgs| {
                reads::numbered(repo, args.number)
            }),
            "lists" => on_repo(&input, |repo, _: &NoArgs| reads::lists(repo)),
            "remarks" => on_repo(&input, |repo, args: &NumberArgs| {
                reads::remarks(repo, args.number)
            }),
            "me" => on_repo(&input, |repo, _: &NoArgs| reads::me(repo)),
            "runs" => on_repo(&input, |repo, _: &NoArgs| reads::runs(repo)),
            "reply" => on_repo(&input, |repo, args: &ReplyArgs| writes::reply(repo, args)),
            "resolve" => on_repo(&input, |repo, args: &ResolveArgs| {
                writes::resolve(repo, args)
            }),
            "post" => on_repo(&input, |repo, args: &PostArgs| writes::post(repo, args)),
            "merge" => on_repo(&input, |repo, args: &NumberArgs| {
                writes::merge(repo, args.number)
            }),
            "update" => on_repo(&input, |repo, args: &UpdateArgs| writes::update(repo, args)),
            _ => Err(forge::unsupported(&call)),
        }
    }
}

squadron_extension!(GitHub);
