use crate::globals::{BINARIES, DATAS, NODES};
use crate::structs::StoData;

use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
pub async fn write_sto(out_file: PathBuf) -> Result<(), anyhow::Error> {
    let data_out = StoData {
        stack_node_datas: DATAS.clone(),
        stack_nodes: NODES.clone(),
        profiled_binaries: BINARIES.clone(),
    };

    let outbuf = serde_json::to_vec(&data_out)?;
    let outfile = File::create(out_file).await?;
    let mut bufwriter = BufWriter::new(outfile);
    bufwriter.write_all(&outbuf).await?;
    bufwriter.flush().await?;
    bufwriter.shutdown().await?;
    Ok(())
}
