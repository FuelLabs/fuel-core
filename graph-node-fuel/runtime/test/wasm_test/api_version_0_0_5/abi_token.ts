export * from './common/global'
import { Address, Bytes, Token, TokenKind, Int64, Uint64 } from './common/types'

export function token_to_address(token: Token): Address {
    assert(token.kind == TokenKind.ADDRESS, "Token is not an address.");
    return changetype<Address>(token.data as u32);
}

export function token_to_bytes(token: Token): Bytes {
    assert(token.kind == TokenKind.FIXED_BYTES 
            || token.kind == TokenKind.BYTES, "Token is not bytes.")
    return changetype<Bytes>(token.data as u32)
}
  
export function token_to_int(token: Token): Int64 {
    assert(token.kind == TokenKind.INT
            || token.kind == TokenKind.UINT, "Token is not an int or uint.")
    return changetype<Int64>(token.data as u32)
}

export function token_to_uint(token: Token): Uint64 {
    assert(token.kind == TokenKind.INT
            || token.kind == TokenKind.UINT, "Token is not an int or uint.")
    return changetype<Uint64>(token.data as u32)
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
    let token = new Token();
    token.kind = TokenKind.ADDRESS;
    token.data = changetype<u32>(address);
    return token
}

export function token_from_bytes(bytes: Bytes): Token {
    let token = new Token();
    token.kind = TokenKind.BYTES;
    token.data = changetype<u32>(bytes);
    return token
}
  
export function token_from_int(int: Int64): Token {
    let token = new Token();
    token.kind = TokenKind.INT;
    token.data = changetype<u32>(int);
    return token
}

export function token_from_uint(uint: Uint64): Token {
    let token = new Token();
    token.kind = TokenKind.UINT;
    token.data = changetype<u32>(uint);
    return token
}

export function token_from_bool(bool: boolean): Token {
    let token = new Token();
    token.kind = TokenKind.BOOL;
    token.data = bool as u32;
    return token
}
  
export function token_from_string(str: string): Token {
    let token = new Token();
    token.kind = TokenKind.STRING;
    token.data = changetype<u32>(str);
    return token
}
  
export function token_from_array(array: Token): Token {
    let token = new Token();
    token.kind = TokenKind.ARRAY;
    token.data = changetype<u32>(array);
    return token
}
