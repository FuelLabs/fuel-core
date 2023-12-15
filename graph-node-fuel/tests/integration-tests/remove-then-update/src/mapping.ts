import { BigInt, Bytes, store } from "@graphprotocol/graph-ts";
import { Trigger } from "../generated/Contract/Contract";
import { Foo } from "../generated/schema";

export function handleTrigger(event: Trigger): void {
  if (event.params.x == 0) {
    create();
  } else if (event.params.x == 1) {
    store.remove("Foo", "0");

    let obj = new Foo("0");
    obj.removed = true;
    obj.save();

    obj = <Foo>Foo.load("0");
    assert(obj.value == null);
  }
}

function create(): void {
  let obj = new Foo("0");
  obj.value = "bla";
  obj.removed = false;
  obj.save();
}
