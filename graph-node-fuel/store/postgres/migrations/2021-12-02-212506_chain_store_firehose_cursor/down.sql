update public.ethereum_networks
    set genesis_block_hash = '0000000000000000000000000000000000000000000000000000000000000000'
    where name = 'near-mainnet';

alter table public.ethereum_networks
    drop column head_block_cursor;
