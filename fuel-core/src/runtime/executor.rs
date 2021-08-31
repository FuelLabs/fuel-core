use std::path::Path;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use wasmer_compiler_llvm::LLVM;
use wasmer_engine_universal::Universal;
use wasmer::{
    imports,
    Instance,
    LazyInit,
    Memory,
    Module,
    NativeFunc,
    Store,
    WasmerEnv,
};
use crate::runtime::ffi;
use crate::runtime::{IndexerError, IndexerResult, Manifest};
use crate::runtime::database::Database;


#[derive(WasmerEnv, Clone)]
pub struct IndexEnv {
    #[wasmer(export)]
    memory: LazyInit<Memory>,
    #[wasmer(export(name = "alloc_fn"))]
    alloc: LazyInit<NativeFunc<u32, u32>>,
    pub db: Arc<Mutex<Database>>,
}


impl IndexEnv {
    pub fn new(db_conn: String) -> IndexerResult<IndexEnv> {
        let db = Arc::new(Mutex::new(Database::new(db_conn)?));
        Ok(IndexEnv {
            memory: Default::default(),
            alloc: Default::default(),
            db
        })
    }
}


/// Responsible for loading a single indexer module, triggering events.
#[derive(Debug)]
pub struct IndexExecutor {
    instance: Instance,
    module: Module,
    store: Store,
    events: HashMap<String, Vec<String>>,
}


impl IndexExecutor {
    pub fn new(db_conn: String, manifest: Manifest, wasm_bytes: impl AsRef<[u8]>) -> IndexerResult<IndexExecutor> {
        let compiler = LLVM::default();
        let store = Store::new(&Universal::new(compiler).engine());
        let module = Module::new(&store, &wasm_bytes)?;

        let mut import_object = imports! {};
        
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

    /// Trigger a WASM event handler, passing in a serialized event struct.
    pub fn trigger_event(&self, event_name: &str, bytes: Vec<u8>) -> IndexerResult<()> {
        let alloc_fn = self.instance
            .exports
            .get_native_function::<u32, u32>("alloc_fn")?;
        let memory = self.instance.exports.get_memory("memory")?;

        if let Some(ref handlers) = self.events.get(event_name) {
            for handler in handlers.iter() {
                let fun = self.instance
                    .exports
                    .get_native_function::<(u32, u32), ()>(handler)?;

                let wasm_mem = alloc_fn.call(bytes.len() as u32)?;
                let range = wasm_mem as usize..wasm_mem as usize + bytes.len();

                unsafe {
                    memory.data_unchecked_mut()[range]
                        .copy_from_slice(&bytes);
                }

                let _result = fun.call(wasm_mem, bytes.len() as u32)?;
            }
        }
        Ok(())
    }
}
