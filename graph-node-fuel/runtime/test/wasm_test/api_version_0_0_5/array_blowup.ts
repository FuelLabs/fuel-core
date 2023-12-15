export * from './common/global'
import { Bytes, Entity, Value } from './common/types'

/** Definitions copied from graph-ts/index.ts */
declare namespace store {
    function set(entity: string, id: string, data: Entity): void
}

export function arrayBlowup(): void {
    // 1 GB array.
    let s = changetype<Bytes>(new Bytes(1_000_000_000).fill(1));

    // Repeated 100 times.
    let a = new Array<Bytes>(100).fill(s);

    let entity = new Entity();
    entity.set("field", Value.fromBytesArray(a));
    store.set("NonExisting", "foo", entity)
}

