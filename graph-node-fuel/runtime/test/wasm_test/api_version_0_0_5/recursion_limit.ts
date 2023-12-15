export * from './common/global';

import { Entity, Value } from './common/types'

declare namespace store {
    function get(entity: string, id: string): Entity | null
    function set(entity: string, id: string, data: Entity): void
    function remove(entity: string, id: string): void
}

export function recursionLimit(depth: i32): void {
    let user = new Entity();
    var val = Value.fromI32(7);
    for (let i = 0; i < depth; i++) {
        val = Value.fromArray([val]);
    }
    user.set("foobar", val);
    store.set("User", "user_id", user);
}