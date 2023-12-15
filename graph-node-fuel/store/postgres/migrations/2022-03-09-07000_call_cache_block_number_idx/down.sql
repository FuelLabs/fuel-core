drop index eth_call_cache_block_number_idx;

do $$
declare
    tables cursor for select namespace
                        from ethereum_networks
                       where namespace != 'public';
begin
	for table_record in tables loop
		execute
			'drop index '
			|| table_record.namespace
			|| '.'
			|| 'call_cache_block_number_idx';
	end loop;
end;
$$;
