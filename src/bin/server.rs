use anyhow::Result;
use once_cell::sync::OnceCell;
use std::borrow::Cow;

use dotenvy::dotenv;

use chrono::format::Numeric::Day;
use chrono::{DateTime, Utc};
use reqwest::header::{REFERER, REFRESH};
use rocket::http::{ContentType, Header};
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::serde::msgpack::MsgPack;
use rocket::{Build, Response, Rocket, State};
use rocket_include_tera::{
    tera_resources_initialize, tera_response, tera_response_cache, EtagIfNoneMatch,
    TeraContextManager, TeraResponse,
};
use rust_embed::RustEmbed;
use serde_derive::{Deserialize, Serialize};
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;

#[macro_use]
extern crate rocket;
use serde_json::json;
use sqlx::{query, Connection, Pool, Postgres, QueryBuilder};
use sto::defs::{ProfiledBinary, StoData};

#[macro_use]
extern crate log;

#[derive(RustEmbed)]
#[folder = "d3-flame-graph/dist/"]
struct Dist;

static DB_POOL: OnceCell<Pool<Postgres>> = OnceCell::new();

static MIGRATOR: Migrator = sqlx::migrate!();

const BIND_LIMIT: usize = 65535;

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
async fn dist(file: PathBuf) -> Option<(ContentType, Cow<'static, [u8]>)> {
    let filename = file.display().to_string();
    let asset = Dist::get(&filename)?;
    let content_type = file
        .extension()
        .and_then(OsStr::to_str)
        .and_then(ContentType::from_extension)
        .unwrap_or(ContentType::Bytes);
    Some((content_type, asset.data))
}

#[post("/data/samples", format = "json", data = "<data>")]
async fn data_ingest(data: Json<StoData>) {
    let mut deser_data = data.0;
    let snd_vec = deser_data.stack_node_datas.into_iter();
    let sn_vec = deser_data.stack_nodes.into_iter();
    let pb_vec = deser_data.profiled_binaries.into_iter();
    DB_POOL.get().expect("err getting db").acquire().await.expect("err getting db").transaction(
        |mut conn|Box::pin(async move {
            let mut qb_1: QueryBuilder<Postgres> = QueryBuilder::new(
                "insert into stack_node_data(id, symbol, file, line_number) "
            );
            qb_1.push_values(snd_vec.take(BIND_LIMIT / 4), |mut b, snd| {
                let id = snd.id as i64;
                let line_no = match snd.line_number {
                    Some(x) => {Some(x as i32)},
                    None => {None}
                };
                b.push_bind(id)
                    .push_bind(snd.symbol)
                    .push_bind(snd.file)
                    .push_bind(line_no);
            });
            qb_1.push(" ON CONFLICT DO NOTHING ");
            let mut q1 = qb_1.build();
            q1.execute(&mut *conn).await;
            let mut qb_2: QueryBuilder<Postgres> = QueryBuilder::new(
                "insert into stack_node(id, parent_id, stack_node_data_id, profiled_binary_id, sample_count) "
            );
            qb_2.push_values(sn_vec.take(BIND_LIMIT / 4), |mut b, sn| {
                let id = sn.id as i64;
                let parent_id = match sn.parent_id {
                    Some(x) => {Some(x as i64)},
                    None => {None}
                };
                let snd_id = sn.stack_node_data_id as i64;
                let pb_id = sn.profiled_binary_id as i64;
                let sample_count = sn.sample_count as i64;
                b.push_bind(id)
                    .push_bind(parent_id)
                    .push_bind(snd_id)
                    .push_bind(pb_id)
                    .push(sample_count);
            });
            qb_2.push(" ON CONFLICT DO UPDATE SET stack_node.sample_count = stack_node.sample_count + excluded.sample_count ");
            let mut q2 = qb_2.build();
            q2.execute(&mut *conn).await;


            let mut qb_3: QueryBuilder<Postgres> = QueryBuilder::new(
                "insert into profiled_binary(id, event, build_id, basename, updated_at, sample_count, raw_data_size, processed_data_size) "
            );
            qb_3.push_values(pb_vec.take(BIND_LIMIT / 4), |mut b, pb| {
                let id = pb.id as i64;
                let updated_at = chrono::Utc::now();
                let sample_count = pb.sample_count as i64;
                let raw_data_size = pb.raw_data_size as i64;
                let processed_data_size = pb.processed_data_size as i64;
                b.push_bind(id)
                    .push_bind(pb.event)
                    .push_bind(pb.build_id)
                    .push_bind(pb.basename)
                    .push_bind(updated_at)
                    .push_bind(sample_count)
                    .push_bind(raw_data_size)
                    .push_bind(processed_data_size);
            });
            qb_3.push(" ON CONFLICT DO UPDATE SET profiled_binary.sample_count = profiled_binary.sample_count + excluded.sample_count, profiled_binary.updated_at = excluded.updated_at, profiled_binary.raw_data_size = profiled_binary.raw_data_size + excluded.raw_data_size, profiled_binary.processed_data_size = profiled_binary.processed_data_size + excluded.processed_data_size ");
            let mut q3 = qb_3.build();
            q3.execute(&mut *conn).await
        })
    );
}

#[get("/dag/<id>")]
async fn data(id: u64) -> Json<D3FlamegraphData> {
    if id == 123 {
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
        });
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
async fn list() -> Json<Vec<ProfiledBinary>> {
    Json(vec![ProfiledBinary {
        id: 123,
        event: "CYCLE".to_string(),
        build_id: Some("whatevs".to_string()),
        basename: "binary".to_string(),
        updated_at: Some(Utc::now()),
        created_at: Some(Utc::now() - chrono::Duration::days(1)),
        sample_count: 10,
        raw_data_size: 0,
        processed_data_size: 0,
    }])
}

#[get("/")]
async fn index(
    cm: &State<TeraContextManager>,
    etag_if_none_match: EtagIfNoneMatch<'_>,
) -> TeraResponse {
    tera_response_cache!(cm, etag_if_none_match, "index", {
        println!("Generate index-2 and cache it...");
        tera_response!(
            cm,
            EtagIfNoneMatch::default(),
            "index",
            json!({"binaries":[{"name": "somename", "id": 123, "version": 1,
            "date": Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()}]})
        )
    })
}

#[rocket::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenv().ok();
    pretty_env_logger::init();

    let db = env::var("DATABASE_URL").expect("error, DATABASE_URL envvar must be set.");

    DB_POOL.set(
        PgPoolOptions::new()
            .max_connections(100)
            .connect(db.as_str())
            .await?,
    );

    MIGRATOR
        .run(DB_POOL.get().expect("err getting db pool"))
        .await?;

    let _rocket = rocket::build()
        .attach(TeraResponse::fairing(|tera| {
            tera_resources_initialize!(
                tera,
                "index" => "src/templates/index.tera",
            );
        }))
        .mount("/", routes![index, data, dist, data_ingest])
        .ignite()
        .await?
        .launch()
        .await?;

    Ok(())
}
