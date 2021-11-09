use crate::database::Database;
use crate::ffi;
use crate::{IndexerError, IndexerResult, Manifest};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use wasmer::{imports, Instance, LazyInit, Memory, Module, NativeFunc, Store, WasmerEnv};
use wasmer_compiler_llvm::LLVM;
use wasmer_engine_universal::Universal;

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
            db,
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
    pub fn new(
        db_conn: String,
        manifest: Manifest,
        wasm_bytes: impl AsRef<[u8]>,
    ) -> IndexerResult<IndexExecutor> {
        let compiler = LLVM::default();
        let store = Store::new(&Universal::new(compiler).engine());
        let module = Module::new(&store, &wasm_bytes)?;

        let mut import_object = imports! {};

        let mut env = IndexEnv::new(db_conn)?;
        let exports = ffi::get_exports(&env, &store);
        import_object.register("env", exports);

        let instance = Instance::new(&module, &import_object)?;
        env.init_with_instance(&instance)?;
        env.db
            .lock()
            .expect("mutex lock failed")
            .load_schema(&instance)?;

        let mut events = HashMap::new();

        for handler in manifest.handlers {
            let handlers = events.entry(handler.event).or_insert(vec![]);

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
        let alloc_fn = self
            .instance
            .exports
            .get_native_function::<u32, u32>("alloc_fn")?;
        let memory = self.instance.exports.get_memory("memory")?;

        if let Some(ref handlers) = self.events.get(event_name) {
            for handler in handlers.iter() {
                let fun = self
                    .instance
                    .exports
                    .get_native_function::<(u32, u32), ()>(handler)?;

                let wasm_mem = alloc_fn.call(bytes.len() as u32)?;
                let range = wasm_mem as usize..wasm_mem as usize + bytes.len();

                unsafe {
                    // Safety: the alloc call for wasm_mem has succeeded for bytes.len()
                    //         so we have this block of memory for copying. The fun.call() below
                    //         will release it.
                    memory.data_unchecked_mut()[range].copy_from_slice(&bytes);
                }

                let _result = fun.call(wasm_mem, bytes.len() as u32)?;
            }
        }
        Ok(())
    }
}

#[cfg(feature = "postgres")]
#[cfg(test)]
mod tests {
    use super::*;
    const DATABASE_URL: &'static str = "postgres://postgres:my-secret@127.0.0.1:5432";
    const MANIFEST: &'static str = include_str!("test_data/manifest.yaml");
    const BAD_MANIFEST: &'static str = include_str!("test_data/bad_manifest.yaml");
    const WASM_BYTES: &'static [u8] = include_bytes!("test_data/simple_wasm.wasm");
    const GOOD_DATA: &'static [u8] = include_bytes!("test_data/good_event.bin");

    #[test]
    fn test_executor() {
        let manifest: Manifest = serde_yaml::from_str(MANIFEST).expect("Bad yaml file.");
        let bad_manifest: Manifest = serde_yaml::from_str(BAD_MANIFEST).expect("Bad yaml file.");

        let executor = IndexExecutor::new(DATABASE_URL.to_string(), bad_manifest, WASM_BYTES);
        match executor {
            Err(IndexerError::MissingHandler(o)) if o == "fn_one" => (),
            e => panic!("Expected missing handler error {:#?}", e),
        }

        let executor = IndexExecutor::new(DATABASE_URL.to_string(), manifest, WASM_BYTES);
        assert!(executor.is_ok());

        let executor = executor.unwrap();

        let result = executor.trigger_event("an_event_name", b"ejfiaiddiie".to_vec());
        match result {
            Err(IndexerError::RuntimeError(_)) => (),
            e => panic!("Should have been a runtime error {:#?}", e),
        }

        let result = executor.trigger_event("an_event_name", GOOD_DATA.to_vec());
        assert!(result.is_ok());
    }
}
