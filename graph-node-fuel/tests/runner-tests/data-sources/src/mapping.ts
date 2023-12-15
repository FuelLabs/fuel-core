import { BigInt, dataSource, ethereum, log } from "@graphprotocol/graph-ts";
import { Data } from "../generated/schema";

export function handleBlock(block: ethereum.Block): void {
  let foo = dataSource.context().getString("foo");
  let bar = dataSource.context().getI32("bar");
  let isTest = dataSource.context().getBoolean("isTest");
  if (block.number == BigInt.fromI32(0)) {
    let data = new Data("0");
    data.foo = foo;
    data.bar = bar;
    data.isTest = isTest;
    data.save();
  }
}
