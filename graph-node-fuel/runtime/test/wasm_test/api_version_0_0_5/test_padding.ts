export * from './common/global'

export class UnitTestTypeBool{
    str_pref: string;
    under_test: boolean;
    str_suff: string;
    large: i64 ;
    tail:  boolean ;



    constructor(str_pref: string, under_test: boolean, str_suff:string, large: i64, tail:boolean) {
        this.str_pref = str_pref;
        this.under_test = under_test;
        this.str_suff = str_suff;
        this.large = large;
        this.tail = tail;
    }
}

export function test_padding_bool(p: UnitTestTypeBool): void {
     assert(p.str_pref == "pref", "parm.str_pref: Assertion failed!");
     assert(p.under_test == true, "parm.under_test: Assertion failed!");
     assert(p.str_suff == "suff", "parm.str_suff: Assertion failed!");
     assert(p.large == 9223372036854775807, "parm.large: Assertion failed!");
     assert(p.tail == true, "parm.tail: Assertion failed!");
}

export class UnitTestTypeI8{
    str_pref: string;
    under_test: i8;
    str_suff: string;
    large: i64 ;
    tail:  boolean ;



    constructor(str_pref: string, under_test: i8, str_suff:string, large: i64, tail:boolean) {
        this.str_pref = str_pref;
        this.under_test = under_test;
        this.str_suff = str_suff;
        this.large = large;
        this.tail = tail;
    }
}

export function test_padding_i8(p: UnitTestTypeI8): void {
    assert(p.str_pref == "pref", "parm.str_pref: Assertion failed!");
    assert(p.under_test == 127, "parm.under_test: Assertion failed!");
    assert(p.str_suff == "suff", "parm.str_suff: Assertion failed!");
    assert(p.large == 9223372036854775807, "parm.large: Assertion failed!");
    assert(p.tail == true, "parm.tail: Assertion failed!");
}


export class UnitTestTypeU16{
    str_pref: string;
    under_test: i16;
    str_suff: string;
    large: i64 ;
    tail:  boolean ;



    constructor(str_pref: string, under_test: i16, str_suff:string, large: i64, tail:boolean) {
        this.str_pref = str_pref;
        this.under_test = under_test;
        this.str_suff = str_suff;
        this.large = large;
        this.tail = tail;
    }
}

export function test_padding_i16(p: UnitTestTypeU16): void {
    assert(p.str_pref == "pref", "parm.str_pref: Assertion failed!");
    assert(p.under_test == 32767, "parm.under_test: Assertion failed!");
    assert(p.str_suff == "suff", "parm.str_suff: Assertion failed!");
    assert(p.large == 9223372036854775807, "parm.large: Assertion failed!");
    assert(p.tail == true, "parm.tail: Assertion failed!");
}

export class UnitTestTypeU32{
    str_pref: string;
    under_test: i32;
    str_suff: string;
    large: i64 ;
    tail:  boolean ;



    constructor(str_pref: string, under_test: i32, str_suff:string, large: i64, tail:boolean) {
        this.str_pref = str_pref;
        this.under_test = under_test;
        this.str_suff = str_suff;
        this.large = large;
        this.tail = tail;
    }
}

export function test_padding_i32(p: UnitTestTypeU32): void {
    assert(p.str_pref == "pref", "parm.str_pref: Assertion failed!");
    assert(p.under_test == 2147483647, "parm.under_test: Assertion failed!");
    assert(p.str_suff == "suff", "parm.str_suff: Assertion failed!");
    assert(p.large == 9223372036854775807, "parm.large: Assertion failed!");
    assert(p.tail == true, "parm.tail: Assertion failed!");
}


export class ManualPadding{
    nonce: i64 ;
    str_suff: string;
    tail: i64 ;

    constructor(nonce: i64, str_suff:string, tail:i64) {
        this.nonce = nonce;
        this.str_suff = str_suff;
        this.tail = tail
    }
}

export function test_padding_manual(p: ManualPadding): void {
    assert(p.nonce == 9223372036854775807, "parm.nonce: Assertion failed!");
    assert(p.str_suff == "suff", "parm.str_suff: Assertion failed!");
    assert(p.tail == 9223372036854775807, "parm.tail: Assertion failed!");
}
