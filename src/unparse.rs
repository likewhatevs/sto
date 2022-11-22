// ok so this part is tricky.
// process...
// reconstruct flame-graphable data via a template.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
// construct two indices on the data.
// one vec sorted by node depth, one map keyed by IDs.
// from the deepest nodes first, traverse up parent IDs, subtracting the occurences of the deepest child node from all it's parents.
// create a chain of nodes traversed throughout this process.
// invert the chain of nodes
// generate count of deepest child node entries of the template, using this chain.
// discard the deepest child node, repeat until all nodes are processed.
// feed the generated data into flamegraph and cross fingers that things look the same.
use crate::structs::{StackNodeData, StoData};
use serde_derive::{Deserialize, Serialize};
use tera::{Context, Tera};

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
}

pub fn construct_template_data(
    mut sto: StoData,
) -> Result<Vec<StackNodeDataListTemplate>, anyhow::Error> {
    let mut depth_vec = Vec::new();
    for (a, b) in sto.stack_nodes.iter() {
        depth_vec.push(b.clone());
    }
    depth_vec.sort_by_key(|x| x.depth);
    depth_vec.reverse();
    let mut node_map = sto.stack_nodes;
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
            symbol: sto.stack_node_datas.get(&first.id).unwrap().clone().symbol,
            file: sto
                .stack_node_datas
                .get(&first.id)
                .clone()
                .unwrap()
                .clone()
                .file,
            line_number: sto.stack_node_datas.get(&first.id).unwrap().line_number,
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
                symbol: sto
                    .stack_node_datas
                    .get(&parent_node.id)
                    .unwrap()
                    .clone()
                    .symbol,
                file: sto
                    .stack_node_datas
                    .get(&parent_node.id)
                    .clone()
                    .unwrap()
                    .clone()
                    .file,
                line_number: sto
                    .stack_node_datas
                    .get(&parent_node.id)
                    .unwrap()
                    .line_number,
            };
            path.push(parent_tmpl_data);
            parent = parent_node.parent_id;
        }
        // ok so now we are at the root, construct an object and invert the list
        path.reverse();
        let template = StackNodeDataListTemplate {
            data_list: path.clone(),
            event: sto.profiled_binaries.values().next().unwrap().clone().event,
            count: first_count,
        };
        results.push(template);
    }
    return Ok(results);
}

pub fn unparse_and_write(
    stack_node_data_lists: Vec<StackNodeDataListTemplate>,
    outfile: PathBuf,
) -> Result<(), anyhow::Error> {
    let mut tera = Tera::default();
    let template_str = r#"""
{% for stack_node_data_list in stack_node_data_lists %}
{% for i in range(end=stack_node_data_list.count) %}
perf 209124 [000]  7006.226761:          1 {{stack_node_data_list.event}}:uk:
{% for stack_node_data in stack_node_data_list %}
{% if stack_node_data.symbol %}
        ffffffffb12d1f18 {{stack_node_data.symbol}}+0x38 ([kernel.kallsyms])
{% else %}
        ffffffffb140cbed dummy_data+0x9d ([kernel.kallsyms])
{% endif %}
{% if stack_node_data.file && stack_node_data.line_number %}
  {{stack_node_data.file}}:{{stack_node_data.line_number}}
{% elif stack_node_data.file %}
  {{stack_node_data.file}}[112d8f]
{% else %}
  dummy_data[112d8f]
{% endif %}
{% endfor %}
{% endfor %}
{% endfor %}
"""#;
    tera.add_raw_template("perf_template.data", template_str)?;
    let mut context = Context::new();
    context.insert("stack_node_data_lists", &stack_node_data_lists);
    let mut file = std::fs::File::create(outfile)?;
    let mut buf = std::io::BufWriter::new(file);
    tera.render_to("perf_template.data", &context, buf)?;
    Ok(())
}
