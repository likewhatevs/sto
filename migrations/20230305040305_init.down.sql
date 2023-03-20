-- Add down migration script here
drop function findRegressions(start_time timestamp);
drop function subtree(rootid bigint);
drop index stack_node_data_symbol_file_line_number_idx;
drop index stack_node_parent_id_id_idx;
drop table stack_node;
drop table stack_node_data;
drop table executable;
