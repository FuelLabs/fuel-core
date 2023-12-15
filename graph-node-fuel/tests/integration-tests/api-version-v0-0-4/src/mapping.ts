import {
  ethereum,
  DataSourceContext,
  dataSource,
  Address,
  BigInt,
} from "@graphprotocol/graph-ts";
import { Template } from "../generated/templates";
import { DataSourceCount } from "../generated/schema";

export function handleBlock(block: ethereum.Block): void {
  let context = new DataSourceContext();
  context.setBigInt("number", block.number);

  Template.createWithContext(
    changetype<Address>(Address.fromHexString(
      "0x2E645469f354BB4F5c8a05B3b30A929361cf77eC"
    )),
    context
  );
}

export function handleBlockTemplate(block: ethereum.Block): void {
  let count = DataSourceCount.load(block.number.toString());
  if (count == null) {
    count = new DataSourceCount(block.number.toString());
    count.count = 0;
  }

  let ctx = dataSource.context();
  let number = ctx.getBigInt("number");
  assert(
    count.count == number.toI32(),
    "wrong count, found " + BigInt.fromI32(count.count).toString()
  );
  count.count += 1;
  count.save();
}
