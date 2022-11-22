use crate::globals::{BINARIES, DATAS, NODES};
use crate::structs::StoData;

use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
pub async fn write_sto(out_file: PathBuf) -> Result<(), anyhow::Error> {
    let data_out = StoData {
        stack_node_datas: HashMap::from_iter(
            DATAS.clone().iter().map(|x| (*x.key(), x.value().clone())),
        ),
        stack_nodes: HashMap::from_iter(
            NODES.clone().iter().map(|x| (*x.key(), x.value().clone())),
        ),
        profiled_binaries: HashMap::from_iter(
            BINARIES
                .clone()
                .iter()
                .map(|x| (*x.key(), x.value().clone())),
        ),
    };
    // let mut outbuf = Vec::new();
    // data_out
    //     .serialize(&mut Serializer::new(&mut outbuf))
    //     .unwrap();
    let outbuf = serde_json::to_vec(&data_out)?;
    // outbuf.write_all(&*serde_json::to_vec(&mut data_out)?);
    let outfile = File::create(out_file).await?;
    let mut bufwriter = BufWriter::new(outfile);
    bufwriter.write_all(&outbuf).await?;
    bufwriter.flush().await?;
    bufwriter.shutdown().await?;
    Ok(())
}
