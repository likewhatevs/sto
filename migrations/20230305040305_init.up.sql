-- Add up migration script here
create table stack_node_data
(
    id       bigint primary key,
    symbol   text    not null,
    file     text,
    line_number     integer
);

create table executable
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
    executable_id bigint references executable (id) on delete cascade deferrable initially deferred not null,
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


create or replace function findRegressions(start_time timestamp default CURRENT_DATE-1)
    returns table
            (
                basename text,
                "a.build_id" text,
                "b.build_id" text,
                file text,
                symbol text,
                pct_diff numeric
            )
    language plpgsql
as
$$
begin
    return query with symPctPerVersionPerBinary as (select exe.*,
                                                           sum(sn.sample_count) / exe.sample_count as normalized_presence,
                                                           snd.*,
                                                           snd.id as snd_id,
                                                           exe.id as exe_id
                                                    from executable exe
                                                             inner join
                                                         stack_node sn on sn.executable_id = exe.id
                                                             inner join stack_node_data snd on sn.stack_node_data_id = snd.id
                                                    group by exe.id, snd.id)
                 select a.basename, a.build_id, b.build_id, a.file, a.symbol,
                        (b.normalized_presence - a.normalized_presence) / a.normalized_presence * 100 as pct_diff
                 from symPctPerVersionPerBinary a cross join symPctPerVersionPerBinary b
                 where a.basename = b.basename and
                         a.exe_id != b.exe_id and
                         a.snd_id = b.snd_id
                   and b.normalized_presence - a.normalized_presence > 0
                   and b.created_at > a.created_at
                   and b.created_at > start_time
                   and a.created_at > start_time
                 order by pct_diff desc;
end;
$$;


CREATE OR REPLACE FUNCTION subtree(rootid bigint)
    RETURNS jsonb
    LANGUAGE sql
    STABLE PARALLEL SAFE AS
$func$
select jsonb_agg(x)
from (select distinct id, filename, line_number, name, value, jsonb_agg(children) as children
      from (SELECT distinct n.id,
                            snd.file        as filename,
                            snd.line_number as line_number,
                            snd.symbol      as name,
                            n.sample_count  as value,
                            subtree(c.id)   AS children
            FROM stack_node n
                     join stack_node c ON n.id = c.parent_id
                     JOIN stack_node_data snd on n.stack_node_data_id = snd.id
            where n.id = rootid
            order by n.id) sub
      group by sub.id, filename, line_number, name, value) x
$func$;


