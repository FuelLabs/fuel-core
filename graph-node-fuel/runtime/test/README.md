# Runtime tests

These are the unit tests that check if the WASM runtime code is working. For now we only run code compiled from the [`AssemblyScript`](https://www.assemblyscript.org/) language, which is done by [`asc`](https://github.com/AssemblyScript/assemblyscript) (the AssemblyScript Compiler) in our [`CLI`](https://github.com/graphprotocol/graph-tooling/tree/main/packages/cli).

We support two versions of their compiler/language for now:

- [`v0.6`](https://github.com/AssemblyScript/assemblyscript/releases/tag/v0.6)
- +[`v0.19.10`](https://github.com/AssemblyScript/assemblyscript/releases/tag/v0.19.10)

Because the internal ABIs changed between these versions, the runtime was added, etc, we had to duplicate the source files used for the tests (`.ts` and `.wasm`).

If you look into the [`wasm_test`](https://github.com/graphprotocol/graph-node/tree/master/runtime/test/wasm_test) folder you'll find two other folders:

- [`api_version_0_0_4`](https://github.com/graphprotocol/graph-node/tree/master/runtime/test/wasm_test/api_version_0_0_4)
- [`api_version_0_0_5`](https://github.com/graphprotocol/graph-node/tree/master/runtime/test/wasm_test/api_version_0_0_5)

This is because the first one (`0.0.4` `apiVersion`) is related to the `v0.6` of `AssemblyScript` and the second (`0.0.5` `apiVersion`) to +`v0.19.10`.

## How to change the `.ts`/`.wasm` files

### Api Version 0.0.4

First make sure your `asc` version is `v0.6`, to check use `asc --version`.

To install the correct one use:

```
npm install -g AssemblyScript/assemblyscript#v0.6
```

And to compile/change the desired test use the command below (just rename the files to the correct ones):

```
asc wasm_test/api_version_0_0_4/abi_classes.ts -b wasm_test/api_version_0_0_4/abi_classes.wasm
```

### Api Version 0.0.5

First make sure your `asc` version is +`v0.19.10`, to check use `asc --version`.

To install the correct one use:

```
# for the precise one
npm install -g assemblyscript@0.19.10

# for the latest one, it should work as well
npm install -g assemblyscript
```

And to compile/change the desired test use the command below (just rename the files to the correct ones):

```
asc --explicitStart --exportRuntime --runtime stub wasm_test/api_version_0_0_5/abi_classes.ts -b wasm_test/api_version_0_0_5/abi_classes.wasm
```

## Caveats

### Api Version 0.0.4

You'll always have to put this at the beginning of your `.ts` files:

```typescript
import "allocator/arena";

export { memory };
```

So the runtime can use the allocator properly.

### Api Version 0.0.5

Since in this version we started:

- Using the `--explicitStart` flag, that requires `__alloc(0)` to always be called before any global be defined
- To add the necessary variants for `TypeId` and using on `id_of_type` function. References from [`here`](https://github.com/graphprotocol/graph-node/blob/8bef4c005f5b1357fe29ca091c9188e1395cc227/graph/src/runtime/mod.rs#L140)

Instead of having to add this manually to all of the files, you can just import and re-export this [`common`](https://github.com/graphprotocol/graph-node/tree/master/runtime/test/wasm_test/api_version_0_0_5/common/global.ts) file like this:

```typescript
export * from './common/global'
```

And import the types you need from [`here`](https://github.com/graphprotocol/graph-node/tree/master/runtime/test/wasm_test/api_version_0_0_5/common/types.ts). If the type you need is missing, just add them there.

This way the runtime can both properly generate the headers with proper class identifiers and do memory allocations.
