import { Trigger } from "../generated/Contract/Contract";
import {Foo} from "../generated/schema";


export function handleTrigger(event: Trigger): void {
  let id = `${event.block.hash.toHexString()}${event.address.toHexString()}`;
  let foo = new Foo(id);
  foo.save();
}







