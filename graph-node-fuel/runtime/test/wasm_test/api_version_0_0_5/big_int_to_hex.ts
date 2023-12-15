export * from './common/global'

declare namespace typeConversion {
    function bigIntToHex(n: Uint8Array): string
}

export function big_int_to_hex(n: Uint8Array): string {
    return typeConversion.bigIntToHex(n)
}
