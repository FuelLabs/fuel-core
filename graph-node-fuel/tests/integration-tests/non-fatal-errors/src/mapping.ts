import {
  DataSourceContext,
  Address,
  dataSource,
} from "@graphprotocol/graph-ts";
import { Foo } from "../generated/schema";
import { ethereum } from "@graphprotocol/graph-ts/chain/ethereum";
import { Dynamic } from "../generated/templates";

export function handleBlockSuccess(block: ethereum.Block): void {
  let obj = new Foo("0");
  obj.save();
  let context = new DataSourceContext();
  context.setString("id", "00");
  Dynamic.createWithContext(
    changetype<Address>(Address.fromHexString(
      "0x2E645469f354BB4F5c8a05B3b30A929361cf77eC"
    )),
    context
  );
}

export function handleBlockError(block: ethereum.Block): void {
  let obj = new Foo("1");
  obj.save();
  let context = new DataSourceContext();
  context.setString("id", "11");
  Dynamic.createWithContext(
    changetype<Address>(Address.fromHexString(
      "0x3E645469f354BB4F5c8a05B3b30A929361cf77eD"
    )),
    context
  );
  assert(false);
}

export function handleBlockTemplate(block: ethereum.Block): void {
  let id = dataSource.context().getString("id");
  let obj = new Foo(id);
  obj.save();
}
