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
    build_id      text,
    basename      text not null,
    -- just makes orm-lite thing easier to use
    updated_at    timestamptz DEFAULT now(),
    --- same
    created_at    timestamptz DEFAULT now(),
    sample_count  bigint      not null,
    raw_data_size bigint      not null,
    processed_data_size bigint not null
);

create table stack_node
(
    id                 bigint primary key,
    parent_id          bigint references stack_node (id) on delete cascade deferrable initially deferred,
    stack_node_data_id bigint references stack_node_data (id) on delete cascade deferrable initially deferred not null,
    profiled_binary_id bigint references profiled_binary (id) on delete cascade deferrable initially deferred not null,
    sample_count       bigint                                                   not null
);

create unique index on stack_node_data (symbol, file, line_number);
create unique index on stack_node (parent_id, id);

-- should cover paths.
ALTER TABLE stack_node_data
    ADD COLUMN search_col tsvector
        GENERATED ALWAYS AS (to_tsvector('english', coalesce(symbol, '') || ' ' || coalesce(file, '') || ' ' ||
                                                    coalesce(line_number::text, ''))) STORED;

CREATE OR REPLACE FUNCTION subtree(rootid bigint)
    RETURNS jsonb
    LANGUAGE sql STABLE PARALLEL SAFE AS
$func$
SELECT jsonb_agg(sub)
FROM  (
          SELECT snd.file as filename, snd.line_number as line_number, snd.symbol as name, n.sample_count as value, subtree(c.id) AS children
          FROM stack_node c
                   JOIN stack_node n ON n.id = c.parent_id
                   join stack_node_data snd on n.stack_node_data_id = snd.id
          where n.id = rootid and c.parent_id = rootid
          order by n.id
      ) sub
$func$;

