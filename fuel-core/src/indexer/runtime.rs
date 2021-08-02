use std::path::Path;
use wasmer_compiler_cranelift::Cranelift;
use wasmer_engine_universal::Universal;
use wasmer::{
    Instance,
    LazyInit,
    Memory,
    Module,
    Store,
    WasmerEnv,
};
use wasmer_wasi::WasiState;
use crate::indexer::ffi::get_ffi_exports;
use crate::indexer::IndexerResult;
use fuel_tx::{ContractId, Salt, Transaction};


#[derive(Clone, Debug, Default)]
pub struct FakeDb;


impl FakeDb {
    pub fn get_transaction(&self) -> Transaction {
        Transaction::create(
            42, 50, 200, 1, Salt::default(),
            vec![], vec![], vec![], vec![],
        )
    }
}


#[derive(WasmerEnv, Clone, Debug, Default)]
pub struct IndexEnv {
    #[wasmer(export)]
    memory: LazyInit<Memory>,
    pub db: FakeDb,
}


#[derive(Debug)]
pub struct IndexExecutor {
    instance: Instance,
    module: Module,
    store: Store,
}


impl IndexExecutor {
    pub fn new(wasm_bytes: impl AsRef<[u8]>) -> IndexerResult<IndexExecutor> {
        let compiler = Cranelift::default();
        let store = Store::new(&Universal::new(compiler).engine());
        let module = Module::new(&store, &wasm_bytes)?;
        let mut wasi_env = WasiState::new("indexer-runtime")
            // .env("ENV_KEY", "ENV_VALUE")
            .finalize()?;

        let mut import_object = wasi_env.import_object(&module)?;
        
        let mut env = IndexEnv::default();
        let exports = get_ffi_exports(&env, &store);
        import_object.register("env", exports);

        let instance = Instance::new(&module, &import_object)?;
        env.init_with_instance(&instance)?;

        Ok(IndexExecutor {
            instance,
            module,
            store,
        })
    }

    /// Restore index from index file
    pub fn from_file(_index: impl AsRef<Path>) -> IndexerResult<IndexExecutor> {
        unimplemented!()
    }

    pub fn save_index_state(&self) -> IndexerResult<IndexExecutor> {
        unimplemented!()
    }

    /// Initialize an index from the provided wasm module.
    pub fn run_indexer(&self) -> IndexerResult<()> {
        let index_fn = self.instance.exports.get_function("run_indexer")?;
        let _result = index_fn.call(&[])?;
        Ok(())
    }
}
