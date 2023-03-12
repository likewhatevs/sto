use anyhow::Result;
use once_cell::sync::OnceCell;
use std::borrow::Cow;
use std::collections::HashMap;

use dotenvy::dotenv;

use chrono::format::Numeric::Day;
use chrono::{DateTime, Utc};
use reqwest::header::{REFERER, REFRESH};
use rocket::http::{ContentType, Header};
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::serde::msgpack::MsgPack;
use rocket::{Build, Config, Response, Rocket, State};
use rocket_include_tera::{
    tera_resources_initialize, tera_response, tera_response_cache, EtagIfNoneMatch,
    TeraContextManager, TeraResponse,
};
use rust_embed::RustEmbed;
use serde_derive::{Deserialize, Serialize};
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use std::{env, thread};
use std::ffi::OsStr;
use std::hash::Hash;
use std::path::PathBuf;
use futures::StreamExt;
use rocket::data::{ByteUnit, Limits, ToByteUnit};

#[macro_use]
extern crate rocket;
use serde_json::json;
use sqlx::{query, Connection, Pool, Postgres, QueryBuilder};
use tracing::Level;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use sto::defs::{ProfiledBinary, StackNode, StackNodeData, StoData};

#[derive(RustEmbed)]
#[folder = "d3-flame-graph/dist/"]
struct Dist;

static DB_POOL: OnceCell<Pool<Postgres>> = OnceCell::new();

static MIGRATOR: Migrator = sqlx::migrate!();

const BIND_LIMIT: usize = 65535;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct D3FlamegraphData {
    pub name: String,
    pub value: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_number: Option<i32>,
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
            q1.execute(&mut *conn).await
        })
    ).await.expect("error in data insert");

    DB_POOL.get().expect("err getting db").acquire().await.expect("err getting db").transaction(
        |mut conn|Box::pin(async move {
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
            qb_3.push(" ON CONFLICT (id) DO UPDATE SET sample_count = profiled_binary.sample_count + excluded.sample_count, updated_at = excluded.updated_at, raw_data_size = profiled_binary.raw_data_size + excluded.raw_data_size, processed_data_size = profiled_binary.processed_data_size + excluded.processed_data_size ");
            let mut q3 = qb_3.build();
            q3.execute(&mut *conn).await
        })
    ).await.expect("error in data insert");


    DB_POOL.get().expect("err getting db").acquire().await.expect("err getting db").transaction(
        |mut conn|Box::pin(async move {
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
            qb_2.push(" ON CONFLICT (id) DO UPDATE SET sample_count = stack_node.sample_count + excluded.sample_count ");
            let mut q2 = qb_2.build();
            q2.execute(&mut *conn).await
        })
    ).await.expect("error in data insert");
}

#[get("/dag/<id>")]
async fn data(id: i64) -> Json<D3FlamegraphData> {
    if id == 123 {
        return Json(D3FlamegraphData {
            name: "junk test data".to_string(),
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
    }

    // else {
    //     return Json(D3FlamegraphData {
    //         name: "select data to start!".to_string(),
    //         value: 3,
    //         filename: None,
    //         line_number: None,
    //         children: None,
    //     });
    // }


    // the dag should rly be made by some cool function in the db (or well i haven't tried that and want to see how it work).
    // for now this simpler.

    // Size of one stack frame for `factorial()` was measured experimentally
    let mut conn = DB_POOL.get().expect("err getting db").acquire().await.expect("err getting db");
    let sn = sqlx::query_as!(StackNode, "select * from stack_node where profiled_binary_id=$1", id)
        .fetch_all(&mut conn)
        .await.expect("query err");

    let snd = sqlx::query_as!(StackNodeData, "select d.id as id, d.symbol as symbol, d.file as file, d.line_number as line_number from stack_node_data d inner join stack_node n ON n.stack_node_data_id = d.id where n.profiled_binary_id = $1 ", id)
        .fetch_all(&mut conn)
        .await.expect("query err");
    let pb = sqlx::query_as!(ProfiledBinary, "select * from profiled_binary where id=$1", id)
        .fetch_one(&mut conn)
        .await.expect("query err");

    let num: u64 = 100_000_000;

    let data = thread::Builder::new().stack_size(num as usize * 0xFF).spawn(move || {
        let sd_map: HashMap<i64, StackNodeData> = HashMap::from_iter(snd);
        let sn_id_map: HashMap<i64, StackNode> = sn.iter().map(|x| (x.id.clone(), x.clone())).collect();
        let mut sn_p_id_map: HashMap<i64, Vec<StackNode>> = HashMap::new();
        // not great but can easily par map w/ rayon if need be.
        let _exec: Vec<()> = sn_id_map.iter().filter(|(k,v)| v.parent_id.is_some()).map(|(k,v)| {
            sn_p_id_map.entry(v.parent_id.unwrap()).and_modify(|e| (*e).push(v.clone())).or_insert(vec![v.clone()]);
        }).collect();

        // sorta a hack to make vis work w/o having to change.
        fn build_dag(cur_id: i64, sn_id_map: &HashMap<i64, StackNode>, sd_map: &HashMap<i64, StackNodeData>, sn_p_id_map: &HashMap<i64, Vec<StackNode>>) -> D3FlamegraphData {
            let cur_sn = sn_id_map.get(&cur_id).unwrap();
            let cur_sd = sd_map.get(&(cur_sn.stack_node_data_id.clone())).unwrap();
            D3FlamegraphData {
                name: cur_sd.symbol.clone(),
                value: cur_sn.sample_count.clone(),
                filename: cur_sd.file.clone(),
                line_number: cur_sd.line_number.clone(),
                children: match sn_p_id_map.get(&cur_id) {
                    Some(id_list) => {
                        Some(id_list.iter().map(|x| build_dag(x.id, sn_id_map, sd_map, sn_p_id_map)).collect())
                    },
                    None => { None }
                },
            }
        };
        // from all root nodes, recursively build out a dag.c
        let children: Vec<D3FlamegraphData> = sn.iter().filter(|x| x.parent_id.is_none()).map(|x| build_dag(x.id, &sn_id_map, &sd_map, &sn_p_id_map) ).collect();
        Json(D3FlamegraphData{
            name: pb.basename,
            value: children.iter().map(|x| x.value ).sum(),
            filename: None,
            line_number: None,
            children: Some(children),
        })
    }).unwrap().join().unwrap();
    data
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TemplateData{
    pub binaries: Vec<TemplateListing>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TemplateListing{
    pub name: String,
    pub id: i64,
    pub date: String,
}


#[get("/")]
async fn index(
    cm: &State<TeraContextManager>,
    etag_if_none_match: EtagIfNoneMatch<'_>,
) -> TeraResponse {
    tera_response_cache!(cm, etag_if_none_match, "index", {
        println!("Generate index-2 and cache it...");
        let dummy_listing = TemplateListing{
            name: "somename".to_string(),
            id: 123,
            date: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
        };
        let mut conn = DB_POOL.get().expect("err getting db").acquire().await.expect("err getting db");
        let pb: Vec<ProfiledBinary> = sqlx::query_as!(ProfiledBinary, "select * from profiled_binary")
        .fetch_all(&mut conn).await.expect("query err");
        let mut template_listing: Vec<TemplateListing> = pb.iter().map(|x| TemplateListing{ name: x.basename.clone(), id: x.id, date: x.created_at.unwrap().format("%Y-%m-%d %H:%M:%S").to_string() } ).collect();
        template_listing.push(dummy_listing);
        tera_response!(
            cm,
            EtagIfNoneMatch::default(),
            "index",
            TemplateData{binaries: template_listing}
        )
    })
}

#[rocket::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenvy::dotenv()?;
    use tracing_subscriber::prelude::*;

    let console_layer = console_subscriber::spawn();
    tracing_subscriber::registry()
        .with(console_layer)
        .with(tracing_subscriber::fmt::layer()
          .with_level(true)
          .with_line_number(true)
          .with_thread_names(true)
          .with_filter(tracing_subscriber::filter::LevelFilter::from_level(Level::DEBUG)))
      .init();

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

    let figment = rocket::Config::figment()
        .merge(("port", 8000))
        .merge(("address", "0.0.0.0"))
        .merge(("limits", Limits::new().limit("json", 1000.mebibytes())));


    let _rocket = rocket::custom(figment)
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
