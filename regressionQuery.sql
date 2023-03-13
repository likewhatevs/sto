with verOne as (select pb.build_id,
                       pb.basename,
                       sum(sn.sample_count) / pb.sample_count as normalized_presence,
                       snd.symbol,
                       snd.line_number,
--        snd.id as snd_id, (because different file names)
                       snd.file
                from profiled_binary pb
                         inner join
                     stack_node sn on sn.profiled_binary_id = pb.id
                         inner join stack_node_data snd on sn.stack_node_data_id = snd.id
                where basename = 'testAppThree'
                  and build_id = 'one'
                group by pb.id, snd.id),
     verTwo as (select pb.build_id,
                       pb.basename,
                       sum(sn.sample_count) / pb.sample_count as normalized_presence,
                       snd.symbol,
                       snd.line_number,
--        snd.id as snd_id, (because different file names)
                       snd.file
                from profiled_binary pb
                         inner join
                     stack_node sn on sn.profiled_binary_id = pb.id
                         inner join stack_node_data snd on sn.stack_node_data_id = snd.id
                where basename = 'testAppThree'
                  and build_id = 'two'
                group by pb.id, snd.id)
-- inner join on snd_id unless file-name mismatch, as is case in demo.
select verTwo.symbol,
       verTwo.line_number,
       verTwo.file,
       (verTwo.normalized_presence - verOne.normalized_presence) / verOne.normalized_presence * 100 as pct_diff
from verTwo
         left join verOne on verOne.symbol = verTwo.symbol
where verOne.line_number = verTwo.line_number
  and verTwo.normalized_presence - verOne.normalized_presence > 0 order by pct_diff desc;