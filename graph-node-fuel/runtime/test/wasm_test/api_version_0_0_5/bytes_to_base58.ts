export * from './common/global'

declare namespace typeConversion {
    function bytesToBase58(n: Uint8Array): string
}

export function bytes_to_base58(n: Uint8Array): string {
    return typeConversion.bytesToBase58(n)
}
