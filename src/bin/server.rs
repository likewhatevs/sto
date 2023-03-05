use anyhow::Result;
use clap::Parser;
use dotenvy::dotenv;
use once_cell::sync::Lazy;
use rocket::serde::json::Json;
use rocket::State;
use rocket_include_tera::{
    tera_resources_initialize, tera_response, tera_response_cache, EtagIfNoneMatch,
    TeraContextManager, TeraResponse,
};
use serde_derive::{Deserialize, Serialize};
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use std::env;
use sto::defs::ServerArgs;
use tera::{Context, Tera};
#[macro_use]
extern crate rocket;
use serde_json::json;
#[macro_use]
extern crate log;
use sqlx::{Pool, Postgres};

static MIGRATOR: Migrator = sqlx::migrate!();

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct D3FlamegraphData {
    pub name: String,
    pub value: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<D3FlamegraphData>>,
}

#[get("/data/<id>")]
fn hello(id: u64) -> String {
    format!("Hello, {} year old named !", id)
}

#[get("/data.json")]
fn data() -> Json<D3FlamegraphData> {
    Json(D3FlamegraphData {
        name: "asdasda".to_string(),
        value: 12,
        children: Option::from(vec![
            D3FlamegraphData {
                name: "dqwd".to_string(),
                value: 2,
                children: None,
            },
            D3FlamegraphData {
                name: "dsqwd".to_string(),
                value: 3,
                children: None,
            },
        ]),
    })
}

#[get("/")]
fn index(cm: &State<TeraContextManager>, etag_if_none_match: EtagIfNoneMatch) -> TeraResponse {
    tera_response_cache!(cm, etag_if_none_match, "index", {
        println!("Generate index-2 and cache it...");
        tera_response!(cm, EtagIfNoneMatch::default(), "index", json!({}))
    })
}

#[rocket::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenv().ok();
    pretty_env_logger::init();

    let db = env::var("DATABASE_URL")?;

    let pool = PgPoolOptions::new()
        .max_connections(100)
        .connect(db.as_str())
        .await?;

    MIGRATOR.run(&pool).await?;

    let _rocket = rocket::build()
        .attach(TeraResponse::fairing(|tera| {
            tera_resources_initialize!(
                tera,
                "index" => "src/templates/index.tera",
            );
        }))
        .mount("/", routes![index, data])
        .manage(pool)
        .ignite()
        .await?
        .launch()
        .await?;

    Ok(())
}
