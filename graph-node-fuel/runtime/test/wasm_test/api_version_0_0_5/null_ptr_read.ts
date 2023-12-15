export * from './common/global';

declare namespace bigInt {
    function plus(x: BigInt, y: BigInt): BigInt
}

class BigInt extends Uint8Array {
    static fromI32(x: i32): BigInt {
        let self = new Uint8Array(4);
        self[0] = x as u8;
        self[1] = (x >> 8) as u8;
        self[2] = (x >> 16) as u8;
        self[3] = (x >> 24) as u8;
        return changetype<BigInt>(self);
    }

    @operator('+')
    plus(other: BigInt): BigInt {
        return bigInt.plus(this, other);
    }
}

class Wrapper {
    public constructor(
        public n: BigInt | null
    ) {}
}

export function nullPtrRead(): void {
    let x = BigInt.fromI32(2);
    let y: BigInt | null = null;

    let wrapper = new Wrapper(y);

    // Operator overloading works even on nullable types.
    // To fix this, the type signature of the function should
    // consider nullable types, like this:
    //
    // @operator('+')
    // plus(other: BigInt | null): BigInt {
    //   // Do null checks
    // }
    //
    // This test is proposidely doing this to make sure we give
    // the correct error message to the user.
    wrapper.n = wrapper.n + x;
}

class SafeBigInt extends Uint8Array {
    static fromI32(x: i32): SafeBigInt {
        let self = new Uint8Array(4);
        self[0] = x as u8;
        self[1] = (x >> 8) as u8;
        self[2] = (x >> 16) as u8;
        self[3] = (x >> 24) as u8;
        return changetype<SafeBigInt>(self);
    }

    @operator('+')
    plus(other: SafeBigInt): SafeBigInt {
        assert(this !== null, "Failed to sum BigInts because left hand side is 'null'");

        return changetype<SafeBigInt>(bigInt.plus(changetype<BigInt>(this), changetype<BigInt>(other)));
    }
}

class Wrapper2 {
    public constructor(
        public n: SafeBigInt | null
    ) {}
}

export function safeNullPtrRead(): void {
    let x = SafeBigInt.fromI32(2);
    let y: SafeBigInt | null = null;

    let wrapper2 = new Wrapper2(y);

    // Breaks as well, but by our assertion, before getting into
    // the Rust code.
    wrapper2.n = wrapper2.n + x;
}
