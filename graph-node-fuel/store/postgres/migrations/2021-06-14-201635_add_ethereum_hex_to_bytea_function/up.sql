/*
Since we deal with a lot of hex data encoded in a very specific string pattern [see link #1], we
need a function to convert that data into byte arrays (the "bytea" PostgreSQL type). Using byte
arrays is also useful because the former occupies double the space of the latter.

The idea is to have a deliberately generic database function to send bytes over so we can re-encode
them into concrete types, such as U64 and H256 and booleans.

Examples:
1. ethereum_hex_to_bytea(null)         -> null
2. ethereum_hex_to_bytea("0x1")        -> \x01
3. ethereum_hex_to_bytea("0x0")        -> \x00
4. ethereum_hex_to_bytea("0xdeadbeef") -> \xdeadbeef
5. ethereum_hex_to_bytea("0x")         -> ERROR: Can't decode an empty hexadecimal string.
6. ethereum_hex_to_bytea("")           -> ERROR: Input must start with '0x'.


[1: https://openethereum.github.io/JSONRPC#types-in-the-jsonrpc]
 */
create or replace function raise_exception_bytea (text)
    returns bytea
    as $$
begin
    raise exception '%', $1;
end;
$$
language plpgsql
volatile;

create or replace function ethereum_hex_to_bytea (eth_hex text)
    returns bytea
    as $$
    select
        case when $1 is null then
            null
        when not substring(eth_hex from 1 for  2) = '0x' then
            raise_exception_bytea('Input must start with ''0x''.')
        when length(eth_hex) = 2 then
            raise_exception_bytea('Can''t decode an empty hexadecimal string.')
        when length($1) % 2 = 0 then
            decode(right ($1, -2), 'hex')
        else
            decode(replace(eth_hex, 'x', ''), 'hex')
        end as return
$$
language sql
immutable strict;
