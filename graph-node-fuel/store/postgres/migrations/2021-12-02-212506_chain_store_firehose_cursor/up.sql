alter table public.ethereum_networks
    add column head_block_cursor text default null;

update public.ethereum_networks
    set genesis_block_hash = 'a7110b9052e1be68f7fa8bb4065bf54e731205801878e708db7464ec4b9b8014'
    where name = 'near-mainnet';