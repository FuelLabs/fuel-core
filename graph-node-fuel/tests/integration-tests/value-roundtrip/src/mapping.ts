import { Trigger } from "../generated/Contract/Contract";
import { Foo } from "../generated/schema";

export function handleTrigger(event: Trigger): void {
  let obj = new Foo("0");
  obj.value = "b\u0000la\u0000";
  obj.save();

  // Null characters are stripped.
  obj = <Foo>Foo.load("0");
  assert(obj.value == "bla", "nulls not stripped");
}
