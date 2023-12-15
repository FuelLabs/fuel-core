export * from './common/global'

declare namespace typeConversion {
    function bigIntToString(n: Uint8Array): string
}

export function big_int_to_string(n: Uint8Array): string {
    return typeConversion.bigIntToString(n)
}
