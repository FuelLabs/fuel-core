export * from './common/global'

declare namespace typeConversion {
    function bytesToString(bytes: Uint8Array): string
}

declare namespace ipfs {
    function cat(hash: String): Uint8Array
}

export function ipfsCatString(hash: string): string {
    return typeConversion.bytesToString(ipfs.cat(hash))
}

export function ipfsCat(hash: string): Uint8Array {
    return ipfs.cat(hash)
}
