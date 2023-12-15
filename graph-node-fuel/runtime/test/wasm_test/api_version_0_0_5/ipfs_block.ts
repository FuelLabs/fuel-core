export * from './common/global'

declare namespace typeConversion {
    function bytesToHex(bytes: Uint8Array): string
}

declare namespace ipfs {
    function getBlock(hash: String): Uint8Array
}

export function ipfsBlockHex(hash: string): string {
    return typeConversion.bytesToHex(ipfs.getBlock(hash))
}
