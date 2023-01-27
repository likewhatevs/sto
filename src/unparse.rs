// ok so this part is tricky.
// process...
// reconstruct flame-graphable data via a template.

use std::path::PathBuf;
// construct two indices on the data.
// one vec sorted by node depth, one map keyed by IDs.
// from the deepest nodes first, traverse up parent IDs, subtracting the occurences of the deepest child node from all it's parents.
// create a chain of nodes traversed throughout this process.
// invert the chain of nodes
// generate count of deepest child node entries of the template, using this chain.
// discard the deepest child node, repeat until all nodes are processed.
// feed the generated data into flamegraph and cross fingers that things look the same.
use crate::structs::{MapStoData, StackNode};
use handlebars::Handlebars;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StackNodeDataListTemplate {
    pub data_list: Vec<StackNodeDataTemplate>,
    pub event: String,
    pub count: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StackNodeDataTemplate {
    pub symbol: String,
    pub file: String,
    pub line_number: u32,
    pub bin_file: String,
}
pub fn construct_template_data(
    sto: MapStoData,
) -> Result<Vec<StackNodeDataListTemplate>, anyhow::Error> {
    let mut depth_vec: Vec<StackNode> = Vec::new();
    for (_a, b) in sto.stack_nodes.iter() {
        depth_vec.push(b.clone());
    }
    depth_vec.sort_by_key(|x| x.depth);
    depth_vec.reverse();
    let mut node_map = sto.stack_nodes;
    let data_map = sto.stack_node_datas;
    let mut results = Vec::new();
    while !depth_vec.is_empty() {
        let mut path = Vec::new();
        let first = depth_vec.remove(0);
        let first_count = node_map.get(&first.id).unwrap().clone().occurrences;
        let mut parent = node_map.get(&first.id).unwrap().clone().parent_id;
        if first_count == 0 {
            continue;
        }
        let leaf = StackNodeDataTemplate {
            symbol: data_map.get(&first.data_id).unwrap().clone().symbol,
            file: data_map.get(&first.data_id).unwrap().clone().file,
            line_number: data_map.get(&first.data_id).unwrap().line_number,
            bin_file: data_map.get(&first.data_id).unwrap().clone().bin_file,
        };
        path.push(leaf);
        while parent != 0 {
            // decrement
            node_map
                .entry(parent)
                .and_modify(|x| x.occurrences -= first_count);
            // copy to path
            let parent_node = node_map.get(&parent).unwrap();
            let parent_tmpl_data = StackNodeDataTemplate {
                symbol: data_map.get(&parent_node.data_id).unwrap().clone().symbol,
                file: data_map.get(&parent_node.data_id).unwrap().clone().file,
                line_number: data_map.get(&parent_node.data_id).unwrap().line_number,
                bin_file: data_map.get(&parent_node.data_id).unwrap().clone().bin_file,
            };
            path.push(parent_tmpl_data);
            parent = parent_node.parent_id;
        }
        let template = StackNodeDataListTemplate {
            data_list: path.clone(),
            event: sto.profiled_binaries.values().next().unwrap().clone().event,
            count: first_count,
        };
        // tera => handlebars
        for _ in 0..first_count {
            results.push(template.clone());
        }
    }
    Ok(results)
}

pub fn unparse_and_write(
    stack_node_data_list: Vec<StackNodeDataListTemplate>,
    outfile: PathBuf,
) -> Result<(), anyhow::Error> {
    log::info!("templating");
    let reg = Handlebars::new();
    let template_str = "
{{#each stack_node_data_list}}
perf 209124 [000]  7006.226761:          1 {{event}}:uk:
{%- for stack_node_data in stack_node_data_list.data_list -%}
{{#each data_list}}
{{#if symbol}}
                  {{symbol}}+0x9d {{#if bin_file}}({{bin_file}}){{else}}([kernel.kallsyms]){{/if}}
{{/if}}
{{#if file}}
  {{file}}:{{#if line_number}}{{line_number}}[112d8f]{{else}}{{/if}}
{{else}}
  dummy_data[112d8f]
{{/if}}
{{/each}}
{{/each}}
";
    let file = std::fs::File::create(outfile)?;
    let mut buf = std::io::BufWriter::new(file);
    reg.render_template_to_write(template_str, &stack_node_data_list, &mut buf)
        .unwrap();
    Ok(())
}
