use std::borrow::Cow;
use anyhow::Result;

use dotenvy::dotenv;

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
use std::ffi::OsStr;
use std::path::PathBuf;
use rocket::http::ContentType;
use rust_embed::RustEmbed;
use chrono::{DateTime, Utc};
use chrono::format::Numeric::Day;

#[macro_use]
extern crate rocket;
use serde_json::json;
use sto::defs::ProfiledBinary;

#[macro_use]
extern crate log;

#[derive(RustEmbed)]
#[folder = "d3-flame-graph/dist/"]
struct Dist;

static MIGRATOR: Migrator = sqlx::migrate!();

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct D3FlamegraphData {
    pub name: String,
    pub value: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<D3FlamegraphData>>,
}

#[get("/dist/<file..>")]
fn dist(file: PathBuf) -> Option<(ContentType, Cow<'static, [u8]>)> {
    let filename = file.display().to_string();
    let asset = Dist::get(&filename)?;
    let content_type = file
        .extension()
        .and_then(OsStr::to_str)
        .and_then(ContentType::from_extension)
        .unwrap_or(ContentType::Bytes);
    Some((content_type, asset.data))
}



#[get("/data/<id>")]
fn data(id: u64) -> Json<D3FlamegraphData> {
    if id == 123{
        return Json(D3FlamegraphData {
            name: "asdasda".to_string(),
            value: 12,
            filename: Some("/var/asdas/ffff.cpp".to_string()),
            line_number: Some(123),
            children: Option::from(vec![
                D3FlamegraphData {
                    name: "dqwd".to_string(),
                    value: 2,
                    filename: None,
                    line_number: None,
                    children: None,
                },
                D3FlamegraphData {
                    name: "dsqwd".to_string(),
                    value: 3,
                    filename: None,
                    line_number: None,
                    children: None,
                },
            ]),
        })
    } else {
        Json(D3FlamegraphData {
            name: "select data to start!".to_string(),
            value: 3,
            filename: None,
            line_number: None,
            children: None,
        })
    }
}


#[get("/list")]
fn list() -> Json<Vec<ProfiledBinary>> {
    Json(vec![ProfiledBinary{
        id: 123,
        event: "CYCLE".to_string(),
        build_id: "whatevs".to_string(),
        basename: "binary".to_string(),
        updated_at: Utc::now(),
        created_at: Utc::now()- chrono::Duration::days(1),
        sample_count: 10,
    }])
}

#[get("/")]
fn index(cm: &State<TeraContextManager>, etag_if_none_match: EtagIfNoneMatch) -> TeraResponse {
    tera_response_cache!(cm, etag_if_none_match, "index", {
        println!("Generate index-2 and cache it...");
        tera_response!(cm, EtagIfNoneMatch::default(), "index",
            json!({"binaries":[{"name": "somename", "id": 123, "version": 1,
            "date": Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()}]}))
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
        .mount("/", routes![index, data, dist])
        .manage(pool)
        .ignite()
        .await?
        .launch()
        .await?;

    Ok(())
}
