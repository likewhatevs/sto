-- Add up migration script here
create table stack_node_data
(
    id       bigint primary key,
    symbol   text    not null,
    file     text,
    line_number     integer
);

create table profiled_binary
(
    id            bigint primary key,
    event         text not null,
    build_id      text not null,
    basename      text not null,
    updated_at    timestamptz not null,
    created_at    timestamptz not null,
    sample_count  bigint      not null,
    raw_data_size bigint      not null
);

create table stack_node
(
    id                 bigint primary key,
    parent_id          bigint references stack_node (id) on delete cascade,
    stack_node_data_id bigint references stack_node_data (id) on delete cascade not null,
    profiled_binary_id bigint references profiled_binary (id) on delete cascade not null,
    sample_count       bigint                                                   not null
);

create unique index on stack_node_data (symbol, file, line_number);
create unique index on stack_node (parent_id, id);

-- should cover paths.
ALTER TABLE stack_node_data
    ADD COLUMN search_col tsvector
        GENERATED ALWAYS AS (to_tsvector('english', coalesce(symbol, '') || ' ' || coalesce(file, '') || ' ' ||
                                                    coalesce(line_number::text, ''))) STORED;


