create table stack_node_data
(
    id       bigint primary key,
    line     integer not null,
    symbol   text    not null,
    filename text    not null
);

create table executable
(
    id            bigint primary key,
    name          text        not null,
    creation_time timestamptz not null,
    sample_count  bigint      not null
);

create table stack_node
(
    id                 bigint primary key,
    parent_id          bigint references stack_node (id) on delete cascade,
    executable_id      bigint references executable (id) on delete cascade      not null,
    stack_node_data_id bigint references stack_node_data (id) on delete cascade not null,
    sample_count       bigint                                                   not null
);

-- communicate constraints for optimizer (in exchange for qps).
create unique index on stack_node_data (line, symbol, line);
create unique index on stack_node (parent_id, stack_node_data_id);

-- should cover paths.
ALTER TABLE stack_node_data
    ADD COLUMN search_col tsvector
        GENERATED ALWAYS AS (to_tsvector('english', coalesce(symbol, '') || ' ' || coalesce(filename, '') || ' ' ||
                                                    coalesce(line::text, ''))) STORED;


