import "allocator/arena";

export { memory };

// Sequence of 20 `u8`s.
type Address = Uint8Array;

// Sequences of `u8`s.
type Bytes = Uint8Array;

// Sequence of 4 `u64`s.
type Int = Uint64Array;
type Uint = Uint64Array;

enum TokenKind {
    ADDRESS = 0,
    FIXED_BYTES = 1,
    BYTES = 2,
    INT = 3,
    UINT = 4,
    BOOL = 5,
    STRING = 6,
    FIXED_ARRAY = 7,
    ARRAY = 8
}

// Big enough to fit any pointer or native this.data.
type Payload = u64

export class Token {
    kind: TokenKind
    data: Payload
}

export function token_to_address(token: Token): Address {  
    assert(token.kind == TokenKind.ADDRESS, "Token is not an address.");
    return changetype<Address>(token.data as u32);
}
  
export function token_to_bytes(token: Token): Bytes {
    assert(token.kind == TokenKind.FIXED_BYTES 
            || token.kind == TokenKind.BYTES, "Token is not bytes.")
    return changetype<Bytes>(token.data as u32)
}
  
export function token_to_int(token: Token): Int {
    assert(token.kind == TokenKind.INT
            || token.kind == TokenKind.UINT, "Token is not an int or uint.")
    return changetype<Int>(token.data as u32)
}

export function token_to_uint(token: Token): Uint {
    assert(token.kind == TokenKind.INT
            || token.kind == TokenKind.UINT, "Token is not an int or uint.")
    return changetype<Uint>(token.data as u32)
}

export function token_to_bool(token: Token): boolean {
    assert(token.kind == TokenKind.BOOL, "Token is not a boolean.")
    return token.data != 0
}
  
export function token_to_string(token: Token): string {
    assert(token.kind == TokenKind.STRING, "Token is not a string.")
    return changetype<string>(token.data as u32)
}
  
export function token_to_array(token: Token): Array<Token> {
    assert(token.kind == TokenKind.FIXED_ARRAY ||
        token.kind == TokenKind.ARRAY, "Token is not an array.")
    return changetype<Array<Token>>(token.data as u32)
}


export function token_from_address(address: Address): Token {  
    let token: Token;
    token.kind = TokenKind.ADDRESS;
    token.data = address as u64;
    return token
}

export function token_from_bytes(bytes: Bytes): Token {
    let token: Token;
    token.kind = TokenKind.BYTES;
    token.data = bytes as u64;
    return token
}
  
export function token_from_int(int: Int): Token {
    let token: Token;
    token.kind = TokenKind.INT;
    token.data = int as u64;
    return token
}

export function token_from_uint(uint: Uint): Token {
    let token: Token;
    token.kind = TokenKind.UINT;
    token.data = uint as u64;
    return token
}

export function token_from_bool(bool: boolean): Token {
    let token: Token;
    token.kind = TokenKind.BOOL;
    token.data = bool as u64;
    return token
}
  
export function token_from_string(str: string): Token {
    let token: Token;
    token.kind = TokenKind.STRING;
    token.data = str as u64;
    return token
}
  
export function token_from_array(array: Token): Token {
    let token: Token;
    token.kind = TokenKind.ARRAY;
    token.data = array as u64;
    return token
}
