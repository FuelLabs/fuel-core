import { Trigger } from "../generated/Contract/Contract";
import { Address, BigDecimal, BigInt, ethereum } from "@graphprotocol/graph-ts";

let one = BigDecimal.fromString("1");

export function handleTrigger(event: Trigger): void {
  testBigDecimal();
  testEthereumAbi();
}

function testBigDecimal(): void {
  // There are 35 digits after the dot.
  // big_decimal exponent will be: -35 - 6109 = -6144.
  // Minimum exponent is: -6143.
  // So 1 digit will be removed, the 8, and the 6 will be rounded to 7.
  let small = BigDecimal.fromString("0.99999999999999999999999999999999968");

  small.exp -= BigInt.fromI32(6109);

  // Round-trip through the node so it truncates.
  small = small * new BigDecimal(BigInt.fromI32(1));

  assert(small.exp == BigInt.fromI32(-6143), "wrong exp");

  // This has 33 nines and the 7 which was rounded from 6.
  assert(
    small.digits ==
      BigDecimal.fromString("9999999999999999999999999999999997").digits,
    "wrong digits " + small.digits.toString()
  );

  // This has 35 nines, but we round to 34 decimal digits.
  let big = BigDecimal.fromString("99999999999999999999999999999999999");

  // This has 35 zeros.
  let rounded = BigDecimal.fromString("100000000000000000000000000000000000");

  assert(big == rounded, "big not equal to rounded");

  // This has 35 eights, but we round to 34 decimal digits.
  let big2 = BigDecimal.fromString("88888888888888888888888888888888888");

  // This has 33 eights.
  let rounded2 = BigDecimal.fromString("88888888888888888888888888888888890");

  assert(big2 == rounded2, "big2 not equal to rounded2 " + big2.toString());

  // Test big decimal division.
  assert(one / BigDecimal.fromString("10") == BigDecimal.fromString("0.1"));

  // Test big int fromString
  assert(BigInt.fromString("8888") == BigInt.fromI32(8888));

  let bigInt = BigInt.fromString("8888888888888888");

  // Test big int bit or
  assert(
    (bigInt | BigInt.fromI32(42)) == BigInt.fromString("8888888888888890")
  );

  // Test big int bit and
  assert((bigInt & BigInt.fromI32(42)) == BigInt.fromI32(40));

  // Test big int left shift
  assert(bigInt.leftShift(6) == BigInt.fromString("568888888888888832"));

  // Test big int right shift
  assert(bigInt.rightShift(6) == BigInt.fromString("138888888888888"));
}

function testEthereumAbi(): void {
  ethereumAbiSimpleCase();
  ethereumAbiComplexCase();
}

function ethereumAbiSimpleCase(): void {
  let address = ethereum.Value.fromAddress(Address.fromString("0x0000000000000000000000000000000000000420"));

  let encoded = ethereum.encode(address)!;

  let decoded = ethereum.decode("address", encoded)!;

  assert(address.toAddress() == decoded.toAddress(), "address ethereum encoded does not equal the decoded value");
}

function ethereumAbiComplexCase(): void {
  let address = ethereum.Value.fromAddress(Address.fromString("0x0000000000000000000000000000000000000420"));
  let bigInt1 = ethereum.Value.fromUnsignedBigInt(BigInt.fromI32(62));
  let bigInt2 = ethereum.Value.fromUnsignedBigInt(BigInt.fromI32(63));
  let bool = ethereum.Value.fromBoolean(true);

  let fixedSizedArray = ethereum.Value.fromFixedSizedArray([
    bigInt1,
    bigInt2
  ]);

  let tupleArray: Array<ethereum.Value> = [
    fixedSizedArray,
    bool
  ];

  let tuple = ethereum.Value.fromTuple(changetype<ethereum.Tuple>(tupleArray));

  let token: Array<ethereum.Value> = [
    address,
    tuple
  ];

  let encoded = ethereum.encode(ethereum.Value.fromTuple(changetype<ethereum.Tuple>(token)))!;

  let decoded = ethereum.decode("(address,(uint256[2],bool))", encoded)!.toTuple();

  let decodedAddress = decoded[0].toAddress();
  let decodedTuple = decoded[1].toTuple();
  let decodedFixedSizedArray = decodedTuple[0].toArray();
  let decodedBigInt1 = decodedFixedSizedArray[0].toBigInt();
  let decodedBigInt2 = decodedFixedSizedArray[1].toBigInt();
  let decodedBool = decodedTuple[1].toBoolean();

  assert(address.toAddress() == decodedAddress, "address ethereum encoded does not equal the decoded value");
  assert(bigInt1.toBigInt() == decodedBigInt1, "uint256[0] ethereum encoded does not equal the decoded value");
  assert(bigInt2.toBigInt() == decodedBigInt2, "uint256[1] ethereum encoded does not equal the decoded value");
  assert(bool.toBoolean() == decodedBool, "boolean ethereum encoded does not equal the decoded value");
}