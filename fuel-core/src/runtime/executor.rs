use std::path::Path;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
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
use crate::runtime::ffi;
use crate::runtime::{IndexerError, IndexerResult, Manifest};
use crate::runtime::database::Database;


#[derive(WasmerEnv, Clone, Debug)]
pub struct IndexEnv {
    #[wasmer(export)]
    memory: LazyInit<Memory>,
    pub db: Arc<Mutex<Database>>,
}


impl IndexEnv {
    pub fn new(db_conn: String) -> IndexerResult<IndexEnv> {
        let db = Arc::new(Mutex::new(Database::new(db_conn)?));
        Ok(IndexEnv {
            memory: Default::default(),
            db
        })
    }
}


#[derive(Debug)]
pub struct IndexExecutor {
    instance: Instance,
    module: Module,
    store: Store,
    events: HashMap<String, Vec<String>>,
}


impl IndexExecutor {
    pub fn new(db_conn: String, manifest: Manifest, wasm_bytes: impl AsRef<[u8]>) -> IndexerResult<IndexExecutor> {
        let compiler = Cranelift::default();
        let store = Store::new(&Universal::new(compiler).engine());
        let module = Module::new(&store, &wasm_bytes)?;
        let mut wasi_env = WasiState::new("indexer-runtime").finalize()?;

        let mut import_object = wasi_env.import_object(&module)?;
        
        let mut env = IndexEnv::new(db_conn)?;
        let exports = ffi::get_exports(&env, &store);
        import_object.register("env", exports);

        let instance = Instance::new(&module, &import_object)?;
        env.init_with_instance(&instance)?;
        env.db.lock().expect("mutex lock failed").load_schema(&instance)?;

        let mut events = HashMap::new();

        for handler in manifest.handlers {
            let handlers = events
                .entry(handler.event)
                .or_insert(vec![]);

            if !instance.exports.contains(&handler.handler) {
                return Err(IndexerError::MissingHandler(handler.handler));
            }

            handlers.push(handler.handler);
        }

        Ok(IndexExecutor {
            instance,
            module,
            store,
            events,
        })
    }

    /// Restore index from wasm file
    pub fn from_file(_index: &Path) -> IndexerResult<IndexExecutor> {
        unimplemented!()
    }

    /// Initialize an index from the provided wasm module.
    pub fn trigger_event(&self, event_name: &str) -> IndexerResult<()> {
        if let Some(ref handlers) = self.events.get(event_name) {
            for handler in handlers.iter() {
                let fun = self.instance
                    .exports
                    .get_function(handler)?;
                let _result = fun.call(&[])?;
            }
        }
        Ok(())
    }
}
