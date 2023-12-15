use never::Never;
use wasmtime::Trap;

use graph::runtime::AscPtr;

/// Helper trait for the `link!` macro.
pub trait IntoWasmRet {
    type Ret: wasmtime::WasmRet;

    fn into_wasm_ret(self) -> Self::Ret;
}

impl IntoWasmRet for () {
    type Ret = Self;
    fn into_wasm_ret(self) -> Self {
        self
    }
}

impl IntoWasmRet for Never {
    type Ret = ();
    fn into_wasm_ret(self) -> Self::Ret {
        unreachable!()
    }
}

impl IntoWasmRet for i32 {
    type Ret = Self;
    fn into_wasm_ret(self) -> Self {
        self
    }
}

impl IntoWasmRet for i64 {
    type Ret = Self;
    fn into_wasm_ret(self) -> Self {
        self
    }
}

impl IntoWasmRet for f64 {
    type Ret = Self;
    fn into_wasm_ret(self) -> Self {
        self
    }
}

impl IntoWasmRet for u64 {
    type Ret = u64;
    fn into_wasm_ret(self) -> u64 {
        self
    }
}

impl IntoWasmRet for bool {
    type Ret = i32;
    fn into_wasm_ret(self) -> i32 {
        self.into()
    }
}

impl<C> IntoWasmRet for AscPtr<C> {
    type Ret = u32;
    fn into_wasm_ret(self) -> u32 {
        self.wasm_ptr()
    }
}

impl<T> IntoWasmRet for Result<T, Trap>
where
    T: IntoWasmRet,
    T::Ret: wasmtime::WasmTy,
{
    type Ret = Result<T::Ret, Trap>;
    fn into_wasm_ret(self) -> Self::Ret {
        self.map(|x| x.into_wasm_ret())
    }
}
