export * from './common/global'
import { BigInt } from './common/types'

declare namespace bigInt {
    function plus(x: BigInt, y: BigInt): BigInt
    function minus(x: BigInt, y: BigInt): BigInt
    function times(x: BigInt, y: BigInt): BigInt
    function dividedBy(x: BigInt, y: BigInt): BigInt
    function mod(x: BigInt, y: BigInt): BigInt
}

export function plus(x: BigInt, y: BigInt): BigInt {
    return bigInt.plus(x, y)
}

export function minus(x: BigInt, y: BigInt): BigInt {
    return bigInt.minus(x, y)
}

export function times(x: BigInt, y: BigInt): BigInt {
    return bigInt.times(x, y)
}

export function dividedBy(x: BigInt, y: BigInt): BigInt {
    return bigInt.dividedBy(x, y)
}

export function mod(x: BigInt, y: BigInt): BigInt {
    return bigInt.mod(x, y)
}
