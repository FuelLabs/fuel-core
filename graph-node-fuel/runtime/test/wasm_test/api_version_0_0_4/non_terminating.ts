import "allocator/arena";
export { memory };

// Test that non-terminating handlers are killed by timeout.
export function loop(): void {
    while (true) {}
}

export function rabbit_hole(): void {
    rabbit_hole()
}
