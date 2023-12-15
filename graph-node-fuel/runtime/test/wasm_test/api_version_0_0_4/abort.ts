import "allocator/arena";

export { memory };

export function abort(): void {
  assert(false, "not true")
}
