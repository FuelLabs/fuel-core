import { Trigger } from "../generated/Contract/Contract";
import { Foo } from "../generated/schema";

export function handleTrigger(event: Trigger): void {
  let obj = new Foo("0");
  obj.value = i64.MAX_VALUE;
  obj.save();

  obj = <Foo>Foo.load("0");
  assert(obj.value == i64.MAX_VALUE, "maybe invalid value");
}
