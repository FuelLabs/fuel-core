import { Trigger } from "../generated/Contract/Contract";
import { Call } from "../generated/schema"

export function handleTrigger(event: Trigger): void {
  let call1 = new Call("one")
  call1.value = "some value"
  call1.save()

  let call2 = new Call("two")
  call2.value = "another value"
  call2.save()

  assert(false)
}
