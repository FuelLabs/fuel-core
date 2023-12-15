create index if not exists
	eth_call_cache_block_number_idx
on
	eth_call_cache(block_number);

do $$
declare
    tables cursor for select namespace
                        from ethereum_networks
                       where namespace != 'public';
begin
	for table_record in tables loop
		execute
			'create index if not exists call_cache_block_number_idx on '
			|| table_record.namespace
			|| '.'
			|| 'call_cache(block_number)';
	end loop;
end;
$$;
