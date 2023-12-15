import { BigInt, Bytes } from "@graphprotocol/graph-ts";
import { Trigger, Contract } from "../generated/Contract/Contract";
import { Call } from "../generated/schema";

export function handleTrigger(event: Trigger): void {
  let contract = Contract.bind(event.address);

  let call = new Call("bytes32 -> uint256");
  call.value = contract
    .exampleFunction(changetype<Bytes>(Bytes.fromHexString("0x1234")))
    .toString();
  call.save();

  call = new Call("string -> string");
  call.value = contract.exampleFunction2("xyz");
  call.save();

  call = new Call("uint256 -> string");
  call.value = contract.exampleFunction1(BigInt.fromI32(1));
  call.save();
}
