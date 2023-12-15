use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::anyhow;
use anyhow::Error;
use graph::components::store::GetScope;
use graph::data::value::Word;
use graph::slog::SendSyncRefUnwindSafeKV;
use never::Never;
use semver::Version;
use wasmtime::{Memory, Trap, TrapCode};

use graph::blockchain::{Blockchain, HostFnCtx};
use graph::data::store;
use graph::data::subgraph::schema::SubgraphError;
use graph::data_source::{offchain, MappingTrigger, TriggerWithHandler};
use graph::prelude::*;
use graph::runtime::{
    asc_new,
    gas::{self, Gas, GasCounter, SaturatingInto},
    AscHeap, AscIndexId, AscType, DeterministicHostError, FromAscObj, HostExportError,
    IndexForAscTypeId, ToAscObj,
};
use graph::util::mem::init_slice;
use graph::{components::subgraph::MappingError, runtime::AscPtr};
pub use into_wasm_ret::IntoWasmRet;
pub use stopwatch::TimeoutStopwatch;

use crate::asc_abi::class::*;
use crate::error::DeterminismLevel;
use crate::gas_rules::{GAS_COST_LOAD, GAS_COST_STORE};
pub use crate::host_exports;
use crate::host_exports::HostExports;
use crate::mapping::MappingContext;
use crate::mapping::ValidModule;

mod into_wasm_ret;
pub mod stopwatch;

// Convenience for a 'top-level' asc_get, with depth 0.
fn asc_get<T, C: AscType, H: AscHeap + ?Sized>(
    heap: &H,
    ptr: AscPtr<C>,
    gas: &GasCounter,
) -> Result<T, DeterministicHostError>
where
    C: AscType + AscIndexId,
    T: FromAscObj<C>,
{
    graph::runtime::asc_get(heap, ptr, gas, 0)
}

pub trait IntoTrap {
    fn determinism_level(&self) -> DeterminismLevel;
    fn into_trap(self) -> Trap;
}

/// A flexible interface for writing a type to AS memory, any pointer can be returned.
/// Use `AscPtr::erased` to convert `AscPtr<T>` into `AscPtr<()>`.
pub trait ToAscPtr {
    fn to_asc_ptr<H: AscHeap>(
        self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscPtr<()>, HostExportError>;
}

impl ToAscPtr for offchain::TriggerData {
    fn to_asc_ptr<H: AscHeap>(
        self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscPtr<()>, HostExportError> {
        asc_new(heap, self.data.as_ref() as &[u8], gas).map(|ptr| ptr.erase())
    }
}

impl<C: Blockchain> ToAscPtr for MappingTrigger<C>
where
    C::MappingTrigger: ToAscPtr,
{
    fn to_asc_ptr<H: AscHeap>(
        self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscPtr<()>, HostExportError> {
        match self {
            MappingTrigger::Onchain(trigger) => trigger.to_asc_ptr(heap, gas),
            MappingTrigger::Offchain(trigger) => trigger.to_asc_ptr(heap, gas),
        }
    }
}

impl<T: ToAscPtr> ToAscPtr for TriggerWithHandler<T> {
    fn to_asc_ptr<H: AscHeap>(
        self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscPtr<()>, HostExportError> {
        self.trigger.to_asc_ptr(heap, gas)
    }
}

/// Handle to a WASM instance, which is terminated if and only if this is dropped.
pub struct WasmInstance<C: Blockchain> {
    pub instance: wasmtime::Instance,

    // This is the only reference to `WasmInstanceContext` that's not within the instance itself, so
    // we can always borrow the `RefCell` with no concern for race conditions.
    //
    // Also this is the only strong reference, so the instance will be dropped once this is dropped.
    // The weak references are circulary held by instance itself through host exports.
    pub instance_ctx: Rc<RefCell<Option<WasmInstanceContext<C>>>>,

    // A reference to the gas counter used for reporting the gas used.
    pub gas: GasCounter,
}

impl<C: Blockchain> Drop for WasmInstance<C> {
    fn drop(&mut self) {
        // Assert that the instance will be dropped.
        assert_eq!(Rc::strong_count(&self.instance_ctx), 1);
    }
}

impl<C: Blockchain> WasmInstance<C> {
    pub fn asc_get<T, P>(&self, asc_ptr: AscPtr<P>) -> Result<T, DeterministicHostError>
    where
        P: AscType + AscIndexId,
        T: FromAscObj<P>,
    {
        asc_get(self.instance_ctx().deref(), asc_ptr, &self.gas)
    }

    pub fn asc_new<P, T: ?Sized>(&mut self, rust_obj: &T) -> Result<AscPtr<P>, HostExportError>
    where
        P: AscType + AscIndexId,
        T: ToAscObj<P>,
    {
        asc_new(self.instance_ctx_mut().deref_mut(), rust_obj, &self.gas)
    }
}

fn is_trap_deterministic(trap: &Trap) -> bool {
    use wasmtime::TrapCode::*;

    // We try to be exhaustive, even though `TrapCode` is non-exhaustive.
    match trap.trap_code() {
        Some(MemoryOutOfBounds)
        | Some(HeapMisaligned)
        | Some(TableOutOfBounds)
        | Some(IndirectCallToNull)
        | Some(BadSignature)
        | Some(IntegerOverflow)
        | Some(IntegerDivisionByZero)
        | Some(BadConversionToInteger)
        | Some(UnreachableCodeReached) => true,

        // `Interrupt`: Can be a timeout, at least as wasmtime currently implements it.
        // `StackOverflow`: We may want to have a configurable stack size.
        // `None`: A host trap, so we need to check the `deterministic_host_trap` flag in the context.
        Some(Interrupt) | Some(StackOverflow) | None | _ => false,
    }
}

impl<C: Blockchain> WasmInstance<C> {
    pub(crate) fn handle_json_callback(
        mut self,
        handler_name: &str,
        value: &serde_json::Value,
        user_data: &store::Value,
    ) -> Result<BlockState<C>, anyhow::Error> {
        let gas_metrics = self.instance_ctx().host_metrics.gas_metrics.clone();
        let gas = GasCounter::new(gas_metrics);
        let value = asc_new(self.instance_ctx_mut().deref_mut(), value, &gas)?;
        let user_data = asc_new(self.instance_ctx_mut().deref_mut(), user_data, &gas)?;

        self.instance_ctx_mut().ctx.state.enter_handler();

        // Invoke the callback
        self.instance
            .get_func(handler_name)
            .with_context(|| format!("function {} not found", handler_name))?
            .typed()?
            .call((value.wasm_ptr(), user_data.wasm_ptr()))
            .with_context(|| format!("Failed to handle callback '{}'", handler_name))?;

        self.instance_ctx_mut().ctx.state.exit_handler();

        Ok(self.take_ctx().ctx.state)
    }

    pub(crate) fn handle_block(
        mut self,
        _logger: &Logger,
        handler_name: &str,
        block_data: Box<[u8]>,
    ) -> Result<(BlockState<C>, Gas), MappingError> {
        let obj = block_data
            .to_vec()
            .to_asc_obj(self.instance_ctx_mut().deref_mut(), &self.gas)?;

        let obj = AscPtr::alloc_obj(obj, self.instance_ctx_mut().deref_mut(), &self.gas)?;

        self.invoke_handler(handler_name, obj, Arc::new(o!()), None)
    }

    pub(crate) fn handle_trigger(
        mut self,
        trigger: TriggerWithHandler<MappingTrigger<C>>,
    ) -> Result<(BlockState<C>, Gas), MappingError>
    where
        <C as Blockchain>::MappingTrigger: ToAscPtr,
    {
        let handler_name = trigger.handler_name().to_owned();
        let gas = self.gas.clone();
        let logging_extras = trigger.logging_extras().cheap_clone();
        let error_context = trigger.trigger.error_context();
        let asc_trigger = trigger.to_asc_ptr(self.instance_ctx_mut().deref_mut(), &gas)?;

        self.invoke_handler(&handler_name, asc_trigger, logging_extras, error_context)
    }

    pub fn take_ctx(&mut self) -> WasmInstanceContext<C> {
        self.instance_ctx.borrow_mut().take().unwrap()
    }

    pub(crate) fn instance_ctx(&self) -> std::cell::Ref<'_, WasmInstanceContext<C>> {
        std::cell::Ref::map(self.instance_ctx.borrow(), |i| i.as_ref().unwrap())
    }

    pub fn instance_ctx_mut(&self) -> std::cell::RefMut<'_, WasmInstanceContext<C>> {
        std::cell::RefMut::map(self.instance_ctx.borrow_mut(), |i| i.as_mut().unwrap())
    }

    #[cfg(debug_assertions)]
    pub fn get_func(&self, func_name: &str) -> wasmtime::Func {
        self.instance.get_func(func_name).unwrap()
    }

    #[cfg(debug_assertions)]
    pub fn gas_used(&self) -> u64 {
        self.gas.get().value()
    }

    fn invoke_handler<T>(
        &mut self,
        handler: &str,
        arg: AscPtr<T>,
        logging_extras: Arc<dyn SendSyncRefUnwindSafeKV>,
        error_context: Option<String>,
    ) -> Result<(BlockState<C>, Gas), MappingError> {
        let func = self
            .instance
            .get_func(handler)
            .with_context(|| format!("function {} not found", handler));

        // Caution: Make sure all exit paths from this function call `exit_handler`.
        self.instance_ctx_mut().ctx.state.enter_handler();

        // `handle_func_call` evaluates the outcome of a WASM function call:
        // - For non-deterministic traps, it terminates early with a `MappingError`.
        // - In case of deterministic errors, it returns `Ok(Some(Error))`.
        // - On successful execution, it returns `Ok(None)`.
        let handle_func_call = |res: Result<_, Trap>, handler| {
            match res {
                Ok(()) => {
                    assert!(self.instance_ctx().possible_reorg == false);
                    assert!(self.instance_ctx().deterministic_host_trap == false);
                    Ok(None)
                }
                Err(trap) if self.instance_ctx().possible_reorg => {
                    self.instance_ctx_mut().ctx.state.exit_handler();
                    Err(MappingError::PossibleReorg(trap.into()))
                }

                // Treat timeouts anywhere in the error chain as a special case to have a better error
                // message. Any `TrapCode::Interrupt` is assumed to be a timeout.
                Err(trap)
                    if Error::from(trap.clone()).chain().any(|e| {
                        e.downcast_ref::<Trap>().and_then(|t| t.trap_code())
                            == Some(TrapCode::Interrupt)
                    }) =>
                {
                    self.instance_ctx_mut().ctx.state.exit_handler();
                    Err(MappingError::Unknown(Error::from(trap).context(format!(
                        "Handler '{}' hit the timeout of '{}' seconds",
                        handler,
                        self.instance_ctx().timeout.unwrap().as_secs()
                    ))))
                }
                Err(trap) => {
                    let trap_is_deterministic =
                        is_trap_deterministic(&trap) || self.instance_ctx().deterministic_host_trap;
                    let e = Error::from(trap);
                    match trap_is_deterministic {
                        true => Ok(Some(e)),
                        false => {
                            self.instance_ctx_mut().ctx.state.exit_handler();
                            Err(MappingError::Unknown(e))
                        }
                    }
                }
            }
        };

        let deterministic_error = match func {
            Ok(func) => match func
                .typed()
                .context("wasm function has incorrect signature")
            {
                Ok(typed_func) => {
                    match handle_func_call(typed_func.call(arg.wasm_ptr()), handler) {
                        Ok(e) => e,
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
                Err(e) => Some(e),
            },
            Err(e) => Some(e),
        };

        if let Some(deterministic_error) = deterministic_error {
            let deterministic_error = match error_context {
                Some(error_context) => deterministic_error.context(error_context),
                None => deterministic_error,
            };
            let message = format!("{:#}", deterministic_error).replace('\n', "\t");

            // Log the error and restore the updates snapshot, effectively reverting the handler.
            error!(&self.instance_ctx().ctx.logger,
                "Handler skipped due to execution failure";
                "handler" => handler,
                "error" => &message,
                logging_extras
            );
            let subgraph_error = SubgraphError {
                subgraph_id: self.instance_ctx().ctx.host_exports.subgraph_id.clone(),
                message,
                block_ptr: Some(self.instance_ctx().ctx.block_ptr.cheap_clone()),
                handler: Some(handler.to_string()),
                deterministic: true,
            };
            self.instance_ctx_mut()
                .ctx
                .state
                .exit_handler_and_discard_changes_due_to_error(subgraph_error);
        } else {
            self.instance_ctx_mut().ctx.state.exit_handler();
        }

        let gas = self.gas.get();
        Ok((self.take_ctx().ctx.state, gas))
    }
}

#[derive(Copy, Clone)]
pub struct ExperimentalFeatures {
    pub allow_non_deterministic_ipfs: bool,
}

pub struct WasmInstanceContext<C: Blockchain> {
    pub ctx: MappingContext<C>,
    pub valid_module: Arc<ValidModule>,
    pub host_metrics: Arc<HostMetrics>,
    pub(crate) timeout: Option<Duration>,

    // Used by ipfs.map.
    pub(crate) timeout_stopwatch: Arc<std::sync::Mutex<TimeoutStopwatch>>,

    // A trap ocurred due to a possible reorg detection.
    pub possible_reorg: bool,

    // A host export trap ocurred for a deterministic reason.
    pub deterministic_host_trap: bool,

    pub(crate) experimental_features: ExperimentalFeatures,

    asc_heap: AscHeapCtx,
}

struct AscHeapCtx {
    // Function wrapper for `idof<T>` from AssemblyScript
    id_of_type: Option<wasmtime::TypedFunc<u32, u32>>,

    // Function exported by the wasm module that will allocate the request number of bytes and
    // return a pointer to the first byte of allocated space.
    memory_allocate: wasmtime::TypedFunc<i32, i32>,

    api_version: semver::Version,

    // In the future there may be multiple memories, but currently there is only one memory per
    // module. And at least AS calls it "memory". There is no uninitialized memory in Wasm, memory
    // is zeroed when initialized or grown.
    memory: Memory,

    // First free byte in the current arena. Set on the first call to `raw_new`.
    arena_start_ptr: i32,

    // Number of free bytes starting from `arena_start_ptr`.
    arena_free_size: i32,
}

impl<C: Blockchain> WasmInstance<C> {
    /// Instantiates the module and sets it to be interrupted after `timeout`.
    pub fn from_valid_module_with_ctx(
        valid_module: Arc<ValidModule>,
        ctx: MappingContext<C>,
        host_metrics: Arc<HostMetrics>,
        timeout: Option<Duration>,
        experimental_features: ExperimentalFeatures,
    ) -> Result<WasmInstance<C>, anyhow::Error> {
        let mut linker = wasmtime::Linker::new(&wasmtime::Store::new(valid_module.module.engine()));
        let host_fns = ctx.host_fns.cheap_clone();
        let api_version = ctx.host_exports.api_version.clone();

        // Used by exports to access the instance context. There are two ways this can be set:
        // - After instantiation, if no host export is called in the start function.
        // - During the start function, if it calls a host export.
        // Either way, after instantiation this will have been set.
        let shared_ctx: Rc<RefCell<Option<WasmInstanceContext<C>>>> = Rc::new(RefCell::new(None));

        // We will move the ctx only once, to init `shared_ctx`. But we don't statically know where
        // it will be moved so we need this ugly thing.
        let ctx: Rc<RefCell<Option<MappingContext<C>>>> = Rc::new(RefCell::new(Some(ctx)));

        // Start the timeout watchdog task.
        let timeout_stopwatch = Arc::new(std::sync::Mutex::new(TimeoutStopwatch::start_new()));
        if let Some(timeout) = timeout {
            // This task is likely to outlive the instance, which is fine.
            let interrupt_handle = linker.store().interrupt_handle().unwrap();
            let timeout_stopwatch = timeout_stopwatch.clone();
            graph::spawn_allow_panic(async move {
                let minimum_wait = Duration::from_secs(1);
                loop {
                    let time_left =
                        timeout.checked_sub(timeout_stopwatch.lock().unwrap().elapsed());
                    match time_left {
                        None => break interrupt_handle.interrupt(), // Timed out.

                        Some(time) if time < minimum_wait => break interrupt_handle.interrupt(),
                        Some(time) => tokio::time::sleep(time).await,
                    }
                }
            });
        }

        // Because `gas` and `deterministic_host_trap` need to be accessed from the gas
        // host fn, they need to be separate from the rest of the context.
        let gas = GasCounter::new(host_metrics.gas_metrics.clone());
        let deterministic_host_trap = Rc::new(AtomicBool::new(false));

        macro_rules! link {
            ($wasm_name:expr, $rust_name:ident, $($param:ident),*) => {
                link!($wasm_name, $rust_name, "host_export_other", $($param),*)
            };

            ($wasm_name:expr, $rust_name:ident, $section:expr, $($param:ident),*) => {
                let modules = valid_module
                    .import_name_to_modules
                    .get($wasm_name)
                    .into_iter()
                    .flatten();

                // link an import with all the modules that require it.
                for module in modules {
                    let func_shared_ctx = Rc::downgrade(&shared_ctx);
                    let valid_module = valid_module.cheap_clone();
                    let host_metrics = host_metrics.cheap_clone();
                    let timeout_stopwatch = timeout_stopwatch.cheap_clone();
                    let ctx = ctx.cheap_clone();
                    let gas = gas.cheap_clone();
                    linker.func(
                        module,
                        $wasm_name,
                        move |caller: wasmtime::Caller, $($param: u32),*| {
                            let instance = func_shared_ctx.upgrade().unwrap();
                            let mut instance = instance.borrow_mut();

                            // Happens when calling a host fn in Wasm start.
                            if instance.is_none() {
                                *instance = Some(WasmInstanceContext::from_caller(
                                    caller,
                                    ctx.borrow_mut().take().unwrap(),
                                    valid_module.cheap_clone(),
                                    host_metrics.cheap_clone(),
                                    timeout,
                                    timeout_stopwatch.cheap_clone(),
                                    experimental_features.clone()
                                ).unwrap())
                            }

                            let instance = instance.as_mut().unwrap();
                            let _section = instance.host_metrics.stopwatch.start_section($section);

                            let result = instance.$rust_name(
                                &gas,
                                $($param.into()),*
                            );
                            match result {
                                Ok(result) => Ok(result.into_wasm_ret()),
                                Err(e) => {
                                    match IntoTrap::determinism_level(&e) {
                                        DeterminismLevel::Deterministic => {
                                            instance.deterministic_host_trap = true;
                                        },
                                        DeterminismLevel::PossibleReorg => {
                                            instance.possible_reorg = true;
                                        },
                                        DeterminismLevel::Unimplemented | DeterminismLevel::NonDeterministic => {},
                                    }

                                    Err(IntoTrap::into_trap(e))
                                }
                            }
                        }
                    )?;
                }
            };
        }

        // Link chain-specifc host fns.
        for host_fn in host_fns.iter() {
            let modules = valid_module
                .import_name_to_modules
                .get(host_fn.name)
                .into_iter()
                .flatten();

            for module in modules {
                let func_shared_ctx = Rc::downgrade(&shared_ctx);
                let host_fn = host_fn.cheap_clone();
                let gas = gas.cheap_clone();
                linker.func(module, host_fn.name, move |call_ptr: u32| {
                    let start = Instant::now();
                    let instance = func_shared_ctx.upgrade().unwrap();
                    let mut instance = instance.borrow_mut();

                    let instance = match &mut *instance {
                        Some(instance) => instance,

                        // Happens when calling a host fn in Wasm start.
                        None => {
                            return Err(anyhow!(
                                "{} is not allowed in global variables",
                                host_fn.name
                            )
                            .into());
                        }
                    };

                    let name_for_metrics = host_fn.name.replace('.', "_");
                    let stopwatch = &instance.host_metrics.stopwatch;
                    let _section =
                        stopwatch.start_section(&format!("host_export_{}", name_for_metrics));
                    let metrics = instance.host_metrics.cheap_clone();

                    let ctx = HostFnCtx {
                        logger: instance.ctx.logger.cheap_clone(),
                        block_ptr: instance.ctx.block_ptr.cheap_clone(),
                        heap: instance,
                        gas: gas.cheap_clone(),
                        metrics,
                    };
                    let ret = (host_fn.func)(ctx, call_ptr).map_err(|e| match e {
                        HostExportError::Deterministic(e) => {
                            instance.deterministic_host_trap = true;
                            e
                        }
                        HostExportError::PossibleReorg(e) => {
                            instance.possible_reorg = true;
                            e
                        }
                        HostExportError::Unknown(e) => e,
                    })?;
                    instance.host_metrics.observe_host_fn_execution_time(
                        start.elapsed().as_secs_f64(),
                        &name_for_metrics,
                    );
                    Ok(ret)
                })?;
            }
        }

        link!("ethereum.encode", ethereum_encode, params_ptr);
        link!("ethereum.decode", ethereum_decode, params_ptr, data_ptr);

        link!("abort", abort, message_ptr, file_name_ptr, line, column);

        link!("store.get", store_get, "host_export_store_get", entity, id);
        link!(
            "store.loadRelated",
            store_load_related,
            "host_export_store_load_related",
            entity,
            id,
            field
        );
        link!(
            "store.get_in_block",
            store_get_in_block,
            "host_export_store_get_in_block",
            entity,
            id
        );
        link!(
            "store.set",
            store_set,
            "host_export_store_set",
            entity,
            id,
            data
        );

        // All IPFS-related functions exported by the host WASM runtime should be listed in the
        // graph::data::subgraph::features::IPFS_ON_ETHEREUM_CONTRACTS_FUNCTION_NAMES array for
        // automatic feature detection to work.
        //
        // For reference, search this codebase for: ff652476-e6ad-40e4-85b8-e815d6c6e5e2
        link!("ipfs.cat", ipfs_cat, "host_export_ipfs_cat", hash_ptr);
        link!(
            "ipfs.map",
            ipfs_map,
            "host_export_ipfs_map",
            link_ptr,
            callback,
            user_data,
            flags
        );
        // The previous ipfs-related functions are unconditionally linked for backward compatibility
        if experimental_features.allow_non_deterministic_ipfs {
            link!(
                "ipfs.getBlock",
                ipfs_get_block,
                "host_export_ipfs_get_block",
                hash_ptr
            );
        }

        link!("store.remove", store_remove, entity_ptr, id_ptr);

        link!("typeConversion.bytesToString", bytes_to_string, ptr);
        link!("typeConversion.bytesToHex", bytes_to_hex, ptr);
        link!("typeConversion.bigIntToString", big_int_to_string, ptr);
        link!("typeConversion.bigIntToHex", big_int_to_hex, ptr);
        link!("typeConversion.stringToH160", string_to_h160, ptr);
        link!("typeConversion.bytesToBase58", bytes_to_base58, ptr);

        link!("json.fromBytes", json_from_bytes, ptr);
        link!("json.try_fromBytes", json_try_from_bytes, ptr);
        link!("json.toI64", json_to_i64, ptr);
        link!("json.toU64", json_to_u64, ptr);
        link!("json.toF64", json_to_f64, ptr);
        link!("json.toBigInt", json_to_big_int, ptr);

        link!("crypto.keccak256", crypto_keccak_256, ptr);

        link!("bigInt.plus", big_int_plus, x_ptr, y_ptr);
        link!("bigInt.minus", big_int_minus, x_ptr, y_ptr);
        link!("bigInt.times", big_int_times, x_ptr, y_ptr);
        link!("bigInt.dividedBy", big_int_divided_by, x_ptr, y_ptr);
        link!("bigInt.dividedByDecimal", big_int_divided_by_decimal, x, y);
        link!("bigInt.mod", big_int_mod, x_ptr, y_ptr);
        link!("bigInt.pow", big_int_pow, x_ptr, exp);
        link!("bigInt.fromString", big_int_from_string, ptr);
        link!("bigInt.bitOr", big_int_bit_or, x_ptr, y_ptr);
        link!("bigInt.bitAnd", big_int_bit_and, x_ptr, y_ptr);
        link!("bigInt.leftShift", big_int_left_shift, x_ptr, bits);
        link!("bigInt.rightShift", big_int_right_shift, x_ptr, bits);

        link!("bigDecimal.toString", big_decimal_to_string, ptr);
        link!("bigDecimal.fromString", big_decimal_from_string, ptr);
        link!("bigDecimal.plus", big_decimal_plus, x_ptr, y_ptr);
        link!("bigDecimal.minus", big_decimal_minus, x_ptr, y_ptr);
        link!("bigDecimal.times", big_decimal_times, x_ptr, y_ptr);
        link!("bigDecimal.dividedBy", big_decimal_divided_by, x, y);
        link!("bigDecimal.equals", big_decimal_equals, x_ptr, y_ptr);

        link!("dataSource.create", data_source_create, name, params);
        link!(
            "dataSource.createWithContext",
            data_source_create_with_context,
            name,
            params,
            context
        );
        link!("dataSource.address", data_source_address,);
        link!("dataSource.network", data_source_network,);
        link!("dataSource.context", data_source_context,);

        link!("ens.nameByHash", ens_name_by_hash, ptr);

        link!("log.log", log_log, level, msg_ptr);

        // `arweave and `box` functionality was removed, but apiVersion <= 0.0.4 must link it.
        if api_version <= Version::new(0, 0, 4) {
            link!("arweave.transactionData", arweave_transaction_data, ptr);
            link!("box.profile", box_profile, ptr);
        }

        // link the `gas` function
        // See also e3f03e62-40e4-4f8c-b4a1-d0375cca0b76
        {
            let gas = gas.cheap_clone();
            linker.func("gas", "gas", move |gas_used: u32| -> Result<(), Trap> {
                // Gas metering has a relevant execution cost cost, being called tens of thousands
                // of times per handler, but it's not worth having a stopwatch section here because
                // the cost of measuring would be greater than the cost of `consume_host_fn`. Last
                // time this was benchmarked it took < 100ns to run.
                if let Err(e) = gas.consume_host_fn_with_metrics(gas_used.saturating_into(), "gas")
                {
                    deterministic_host_trap.store(true, Ordering::SeqCst);
                    return Err(e.into_trap());
                }

                Ok(())
            })?;
        }

        let instance = linker.instantiate(&valid_module.module)?;

        // Usually `shared_ctx` is still `None` because no host fns were called during start.
        if shared_ctx.borrow().is_none() {
            *shared_ctx.borrow_mut() = Some(WasmInstanceContext::from_instance(
                &instance,
                ctx.borrow_mut().take().unwrap(),
                valid_module,
                host_metrics,
                timeout,
                timeout_stopwatch,
                experimental_features,
            )?);
        }

        match api_version {
            version if version <= Version::new(0, 0, 4) => {}
            _ => {
                instance
                    .get_func("_start")
                    .context("`_start` function not found")?
                    .typed::<(), ()>()?
                    .call(())?;
            }
        }

        Ok(WasmInstance {
            instance,
            instance_ctx: shared_ctx,
            gas,
        })
    }
}

fn host_export_error_from_trap(trap: Trap, context: String) -> HostExportError {
    let trap_is_deterministic = is_trap_deterministic(&trap);
    let e = Error::from(trap).context(context);
    match trap_is_deterministic {
        true => HostExportError::Deterministic(e),
        false => HostExportError::Unknown(e),
    }
}

// This impl is a convenience that delegates to `self.asc_heap`.
impl<C: Blockchain> AscHeap for WasmInstanceContext<C> {
    fn raw_new(&mut self, bytes: &[u8], gas: &GasCounter) -> Result<u32, DeterministicHostError> {
        self.asc_heap.raw_new(bytes, gas)
    }

    fn read<'a>(
        &self,
        offset: u32,
        buffer: &'a mut [MaybeUninit<u8>],
        gas: &GasCounter,
    ) -> Result<&'a mut [u8], DeterministicHostError> {
        self.asc_heap.read(offset, buffer, gas)
    }

    fn read_u32(&self, offset: u32, gas: &GasCounter) -> Result<u32, DeterministicHostError> {
        self.asc_heap.read_u32(offset, gas)
    }

    fn api_version(&self) -> Version {
        self.asc_heap.api_version()
    }

    fn asc_type_id(&mut self, type_id_index: IndexForAscTypeId) -> Result<u32, HostExportError> {
        self.asc_heap.asc_type_id(type_id_index)
    }
}

impl AscHeap for AscHeapCtx {
    fn raw_new(&mut self, bytes: &[u8], gas: &GasCounter) -> Result<u32, DeterministicHostError> {
        // The cost of writing to wasm memory from the host is the same as of writing from wasm
        // using load instructions.
        gas.consume_host_fn_with_metrics(
            Gas::new(GAS_COST_STORE as u64 * bytes.len() as u64),
            "raw_new",
        )?;

        // We request large chunks from the AssemblyScript allocator to use as arenas that we
        // manage directly.

        static MIN_ARENA_SIZE: i32 = 10_000;

        let size = i32::try_from(bytes.len()).unwrap();
        if size > self.arena_free_size {
            // Allocate a new arena. Any free space left in the previous arena is left unused. This
            // causes at most half of memory to be wasted, which is acceptable.
            let arena_size = size.max(MIN_ARENA_SIZE);

            // Unwrap: This may panic if more memory needs to be requested from the OS and that
            // fails. This error is not deterministic since it depends on the operating conditions
            // of the node.
            self.arena_start_ptr = self.memory_allocate.call(arena_size).unwrap();
            self.arena_free_size = arena_size;

            match &self.api_version {
                version if *version <= Version::new(0, 0, 4) => {}
                _ => {
                    // This arithmetic is done because when you call AssemblyScripts's `__alloc`
                    // function, it isn't typed and it just returns `mmInfo` on it's header,
                    // differently from allocating on regular types (`__new` for example).
                    // `mmInfo` has size of 4, and everything allocated on AssemblyScript memory
                    // should have alignment of 16, this means we need to do a 12 offset on these
                    // big chunks of untyped allocation.
                    self.arena_start_ptr += 12;
                    self.arena_free_size -= 12;
                }
            };
        };

        let ptr = self.arena_start_ptr as usize;

        // Unwrap: We have just allocated enough space for `bytes`.
        self.memory.write(ptr, bytes).unwrap();
        self.arena_start_ptr += size;
        self.arena_free_size -= size;

        Ok(ptr as u32)
    }

    fn read_u32(&self, offset: u32, gas: &GasCounter) -> Result<u32, DeterministicHostError> {
        gas.consume_host_fn_with_metrics(Gas::new(GAS_COST_LOAD as u64 * 4), "read_u32")?;
        let mut bytes = [0; 4];
        self.memory.read(offset as usize, &mut bytes).map_err(|_| {
            DeterministicHostError::from(anyhow!(
                "Heap access out of bounds. Offset: {} Size: {}",
                offset,
                4
            ))
        })?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read<'a>(
        &self,
        offset: u32,
        buffer: &'a mut [MaybeUninit<u8>],
        gas: &GasCounter,
    ) -> Result<&'a mut [u8], DeterministicHostError> {
        // The cost of reading wasm memory from the host is the same as of reading from wasm using
        // load instructions.
        gas.consume_host_fn_with_metrics(
            Gas::new(GAS_COST_LOAD as u64 * (buffer.len() as u64)),
            "read",
        )?;

        let offset = offset as usize;

        unsafe {
            // Safety: This was copy-pasted from Memory::read, and we ensure
            // nothing else is writing this memory because we don't call into
            // WASM here.
            let src = self
                .memory
                .data_unchecked()
                .get(offset..)
                .and_then(|s| s.get(..buffer.len()))
                .ok_or(DeterministicHostError::from(anyhow!(
                    "Heap access out of bounds. Offset: {} Size: {}",
                    offset,
                    buffer.len()
                )))?;

            Ok(init_slice(src, buffer))
        }
    }

    fn api_version(&self) -> Version {
        self.api_version.clone()
    }

    fn asc_type_id(&mut self, type_id_index: IndexForAscTypeId) -> Result<u32, HostExportError> {
        self.id_of_type
            .as_ref()
            .unwrap() // Unwrap ok because it's only called on correct apiVersion, look for AscPtr::generate_header
            .call(type_id_index as u32)
            .map_err(|trap| {
                host_export_error_from_trap(
                    trap,
                    format!("Failed to call 'asc_type_id' with '{:?}'", type_id_index),
                )
            })
    }
}

impl<C: Blockchain> WasmInstanceContext<C> {
    pub fn from_instance(
        instance: &wasmtime::Instance,
        ctx: MappingContext<C>,
        valid_module: Arc<ValidModule>,
        host_metrics: Arc<HostMetrics>,
        timeout: Option<Duration>,
        timeout_stopwatch: Arc<std::sync::Mutex<TimeoutStopwatch>>,
        experimental_features: ExperimentalFeatures,
    ) -> Result<Self, anyhow::Error> {
        // Provide access to the WASM runtime linear memory
        let memory = instance
            .get_memory("memory")
            .context("Failed to find memory export in the WASM module")?;

        let memory_allocate = match &ctx.host_exports.api_version {
            version if *version <= Version::new(0, 0, 4) => instance
                .get_func("memory.allocate")
                .context("`memory.allocate` function not found"),
            _ => instance
                .get_func("allocate")
                .context("`allocate` function not found"),
        }?
        .typed()?
        .clone();

        let id_of_type = match &ctx.host_exports.api_version {
            version if *version <= Version::new(0, 0, 4) => None,
            _ => Some(
                instance
                    .get_func("id_of_type")
                    .context("`id_of_type` function not found")?
                    .typed()?
                    .clone(),
            ),
        };

        Ok(WasmInstanceContext {
            asc_heap: AscHeapCtx {
                memory_allocate,
                memory,
                arena_start_ptr: 0,
                arena_free_size: 0,
                api_version: ctx.host_exports.api_version.clone(),
                id_of_type,
            },
            ctx,
            valid_module,
            host_metrics,
            timeout,
            timeout_stopwatch,
            possible_reorg: false,
            deterministic_host_trap: false,
            experimental_features,
        })
    }

    pub fn from_caller(
        caller: wasmtime::Caller,
        ctx: MappingContext<C>,
        valid_module: Arc<ValidModule>,
        host_metrics: Arc<HostMetrics>,
        timeout: Option<Duration>,
        timeout_stopwatch: Arc<std::sync::Mutex<TimeoutStopwatch>>,
        experimental_features: ExperimentalFeatures,
    ) -> Result<Self, anyhow::Error> {
        let memory = caller
            .get_export("memory")
            .and_then(|e| e.into_memory())
            .context("Failed to find memory export in the WASM module")?;

        let memory_allocate = match &ctx.host_exports.api_version {
            version if *version <= Version::new(0, 0, 4) => caller
                .get_export("memory.allocate")
                .and_then(|e| e.into_func())
                .context("`memory.allocate` function not found"),
            _ => caller
                .get_export("allocate")
                .and_then(|e| e.into_func())
                .context("`allocate` function not found"),
        }?
        .typed()?
        .clone();

        let id_of_type = match &ctx.host_exports.api_version {
            version if *version <= Version::new(0, 0, 4) => None,
            _ => Some(
                caller
                    .get_export("id_of_type")
                    .and_then(|e| e.into_func())
                    .context("`id_of_type` function not found")?
                    .typed()?
                    .clone(),
            ),
        };

        Ok(WasmInstanceContext {
            asc_heap: AscHeapCtx {
                memory_allocate,
                memory,
                arena_start_ptr: 0,
                arena_free_size: 0,
                api_version: ctx.host_exports.api_version.clone(),
                id_of_type,
            },
            ctx,
            valid_module,
            host_metrics,
            timeout,
            timeout_stopwatch,
            possible_reorg: false,
            deterministic_host_trap: false,
            experimental_features,
        })
    }

    fn store_get_scoped(
        &mut self,
        gas: &GasCounter,
        entity_ptr: AscPtr<AscString>,
        id_ptr: AscPtr<AscString>,
        scope: GetScope,
    ) -> Result<AscPtr<AscEntity>, HostExportError> {
        let _timer = self
            .host_metrics
            .cheap_clone()
            .time_host_fn_execution_region("store_get");

        let entity_type: String = asc_get(self, entity_ptr, gas)?;
        let id: String = asc_get(self, id_ptr, gas)?;
        let entity_option = self.ctx.host_exports.store_get(
            &mut self.ctx.state,
            entity_type.clone(),
            id.clone(),
            gas,
            scope,
        )?;

        if self.ctx.instrument {
            debug!(self.ctx.logger, "store_get";
                    "type" => &entity_type,
                    "id" => &id,
                    "found" => entity_option.is_some());
        }

        let ret = match entity_option {
            Some(entity) => {
                let _section = self
                    .host_metrics
                    .stopwatch
                    .start_section("store_get_asc_new");
                asc_new(&mut self.asc_heap, &entity.sorted_ref(), gas)?
            }
            None => match &self.ctx.debug_fork {
                Some(fork) => {
                    let entity_option = fork.fetch(entity_type, id).map_err(|e| {
                        HostExportError::Unknown(anyhow!(
                            "store_get: failed to fetch entity from the debug fork: {}",
                            e
                        ))
                    })?;
                    match entity_option {
                        Some(entity) => {
                            let _section = self
                                .host_metrics
                                .stopwatch
                                .start_section("store_get_asc_new");
                            let entity = asc_new(self, &entity.sorted(), gas)?;
                            self.store_set(gas, entity_ptr, id_ptr, entity)?;
                            entity
                        }
                        None => AscPtr::null(),
                    }
                }
                None => AscPtr::null(),
            },
        };

        Ok(ret)
    }
}

// Implementation of externals.
impl<C: Blockchain> WasmInstanceContext<C> {
    /// function abort(message?: string | null, fileName?: string | null, lineNumber?: u32, columnNumber?: u32): void
    /// Always returns a trap.
    pub fn abort(
        &mut self,
        gas: &GasCounter,
        message_ptr: AscPtr<AscString>,
        file_name_ptr: AscPtr<AscString>,
        line_number: u32,
        column_number: u32,
    ) -> Result<Never, DeterministicHostError> {
        let message = match message_ptr.is_null() {
            false => Some(asc_get(self, message_ptr, gas)?),
            true => None,
        };
        let file_name = match file_name_ptr.is_null() {
            false => Some(asc_get(self, file_name_ptr, gas)?),
            true => None,
        };
        let line_number = match line_number {
            0 => None,
            _ => Some(line_number),
        };
        let column_number = match column_number {
            0 => None,
            _ => Some(column_number),
        };

        self.ctx
            .host_exports
            .abort(message, file_name, line_number, column_number, gas)
    }

    /// function store.set(entity: string, id: string, data: Entity): void
    pub fn store_set(
        &mut self,
        gas: &GasCounter,
        entity_ptr: AscPtr<AscString>,
        id_ptr: AscPtr<AscString>,
        data_ptr: AscPtr<AscEntity>,
    ) -> Result<(), HostExportError> {
        let stopwatch = &self.host_metrics.stopwatch;
        stopwatch.start_section("host_export_store_set__wasm_instance_context_store_set");

        let entity: String = asc_get(self, entity_ptr, gas)?;
        let id: String = asc_get(self, id_ptr, gas)?;
        let data = asc_get(self, data_ptr, gas)?;

        if self.ctx.instrument {
            debug!(self.ctx.logger, "store_set";
                    "type" => &entity,
                    "id" => &id);
        }

        self.ctx.host_exports.store_set(
            &self.ctx.logger,
            &mut self.ctx.state,
            self.ctx.block_ptr.number,
            &self.ctx.proof_of_indexing,
            entity,
            id,
            data,
            stopwatch,
            gas,
        )?;

        Ok(())
    }

    /// function store.remove(entity: string, id: string): void
    pub fn store_remove(
        &mut self,
        gas: &GasCounter,
        entity_ptr: AscPtr<AscString>,
        id_ptr: AscPtr<AscString>,
    ) -> Result<(), HostExportError> {
        let entity: String = asc_get(self, entity_ptr, gas)?;
        let id: String = asc_get(self, id_ptr, gas)?;
        if self.ctx.instrument {
            debug!(self.ctx.logger, "store_remove";
                    "type" => &entity,
                    "id" => &id);
        }
        self.ctx.host_exports.store_remove(
            &self.ctx.logger,
            &mut self.ctx.state,
            &self.ctx.proof_of_indexing,
            entity,
            id,
            gas,
        )
    }

    /// function store.get(entity: string, id: string): Entity | null
    pub fn store_get(
        &mut self,
        gas: &GasCounter,
        entity_ptr: AscPtr<AscString>,
        id_ptr: AscPtr<AscString>,
    ) -> Result<AscPtr<AscEntity>, HostExportError> {
        self.store_get_scoped(gas, entity_ptr, id_ptr, GetScope::Store)
    }

    /// function store.get_in_block(entity: string, id: string): Entity | null
    pub fn store_get_in_block(
        &mut self,
        gas: &GasCounter,
        entity_ptr: AscPtr<AscString>,
        id_ptr: AscPtr<AscString>,
    ) -> Result<AscPtr<AscEntity>, HostExportError> {
        self.store_get_scoped(gas, entity_ptr, id_ptr, GetScope::InBlock)
    }

    /// function store.loadRelated(entity_type: string, id: string, field: string): Array<Entity>
    pub fn store_load_related(
        &mut self,
        gas: &GasCounter,
        entity_type_ptr: AscPtr<AscString>,
        id_ptr: AscPtr<AscString>,
        field_ptr: AscPtr<AscString>,
    ) -> Result<AscPtr<Array<AscPtr<AscEntity>>>, HostExportError> {
        let entity_type: String = asc_get(self, entity_type_ptr, gas)?;
        let id: String = asc_get(self, id_ptr, gas)?;
        let field: String = asc_get(self, field_ptr, gas)?;
        let entities = self.ctx.host_exports.store_load_related(
            &mut self.ctx.state,
            entity_type.clone(),
            id.clone(),
            field.clone(),
            gas,
        )?;

        let entities: Vec<Vec<(Word, Value)>> =
            entities.into_iter().map(|entity| entity.sorted()).collect();
        let ret = asc_new(self, &entities, gas)?;
        Ok(ret)
    }

    /// function typeConversion.bytesToString(bytes: Bytes): string
    pub fn bytes_to_string(
        &mut self,
        gas: &GasCounter,
        bytes_ptr: AscPtr<Uint8Array>,
    ) -> Result<AscPtr<AscString>, HostExportError> {
        let string = self.ctx.host_exports.bytes_to_string(
            &self.ctx.logger,
            asc_get(self, bytes_ptr, gas)?,
            gas,
        )?;
        asc_new(self, &string, gas)
    }

    /// Converts bytes to a hex string.
    /// function typeConversion.bytesToHex(bytes: Bytes): string
    /// References:
    /// https://godoc.org/github.com/ethereum/go-ethereum/common/hexutil#hdr-Encoding_Rules
    /// https://github.com/ethereum/web3.js/blob/f98fe1462625a6c865125fecc9cb6b414f0a5e83/packages/web3-utils/src/utils.js#L283
    pub fn bytes_to_hex(
        &mut self,
        gas: &GasCounter,
        bytes_ptr: AscPtr<Uint8Array>,
    ) -> Result<AscPtr<AscString>, HostExportError> {
        let bytes: Vec<u8> = asc_get(self, bytes_ptr, gas)?;
        gas.consume_host_fn_with_metrics(
            gas::DEFAULT_GAS_OP.with_args(gas::complexity::Size, &bytes),
            "bytes_to_hex",
        )?;

        // Even an empty string must be prefixed with `0x`.
        // Encodes each byte as a two hex digits.
        let hex = format!("0x{}", hex::encode(bytes));
        asc_new(self, &hex, gas)
    }

    /// function typeConversion.bigIntToString(n: Uint8Array): string
    pub fn big_int_to_string(
        &mut self,
        gas: &GasCounter,
        big_int_ptr: AscPtr<AscBigInt>,
    ) -> Result<AscPtr<AscString>, HostExportError> {
        let n: BigInt = asc_get(self, big_int_ptr, gas)?;
        gas.consume_host_fn_with_metrics(
            gas::DEFAULT_GAS_OP.with_args(gas::complexity::Mul, (&n, &n)),
            "big_int_to_string",
        )?;
        asc_new(self, &n.to_string(), gas)
    }

    /// function bigInt.fromString(x: string): BigInt
    pub fn big_int_from_string(
        &mut self,
        gas: &GasCounter,
        string_ptr: AscPtr<AscString>,
    ) -> Result<AscPtr<AscBigInt>, HostExportError> {
        let result = self
            .ctx
            .host_exports
            .big_int_from_string(asc_get(self, string_ptr, gas)?, gas)?;
        asc_new(self, &result, gas)
    }

    /// function typeConversion.bigIntToHex(n: Uint8Array): string
    pub fn big_int_to_hex(
        &mut self,
        gas: &GasCounter,
        big_int_ptr: AscPtr<AscBigInt>,
    ) -> Result<AscPtr<AscString>, HostExportError> {
        let n: BigInt = asc_get(self, big_int_ptr, gas)?;
        let hex = self.ctx.host_exports.big_int_to_hex(n, gas)?;
        asc_new(self, &hex, gas)
    }

    /// function typeConversion.stringToH160(s: String): H160
    pub fn string_to_h160(
        &mut self,
        gas: &GasCounter,
        str_ptr: AscPtr<AscString>,
    ) -> Result<AscPtr<AscH160>, HostExportError> {
        let s: String = asc_get(self, str_ptr, gas)?;
        let h160 = self.ctx.host_exports.string_to_h160(&s, gas)?;
        asc_new(self, &h160, gas)
    }

    /// function json.fromBytes(bytes: Bytes): JSONValue
    pub fn json_from_bytes(
        &mut self,
        gas: &GasCounter,
        bytes_ptr: AscPtr<Uint8Array>,
    ) -> Result<AscPtr<AscEnum<JsonValueKind>>, HostExportError> {
        let bytes: Vec<u8> = asc_get(self, bytes_ptr, gas)?;
        let result = self
            .ctx
            .host_exports
            .json_from_bytes(&bytes, gas)
            .with_context(|| {
                format!(
                    "Failed to parse JSON from byte array. Bytes (truncated to 1024 chars): `{:?}`",
                    &bytes[..bytes.len().min(1024)],
                )
            })
            .map_err(DeterministicHostError::from)?;
        asc_new(self, &result, gas)
    }

    /// function json.try_fromBytes(bytes: Bytes): Result<JSONValue, boolean>
    pub fn json_try_from_bytes(
        &mut self,
        gas: &GasCounter,
        bytes_ptr: AscPtr<Uint8Array>,
    ) -> Result<AscPtr<AscResult<AscPtr<AscEnum<JsonValueKind>>, bool>>, HostExportError> {
        let bytes: Vec<u8> = asc_get(self, bytes_ptr, gas)?;
        let result = self
            .ctx
            .host_exports
            .json_from_bytes(&bytes, gas)
            .map_err(|e| {
                warn!(
                    &self.ctx.logger,
                    "Failed to parse JSON from byte array";
                    "bytes" => format!("{:?}", bytes),
                    "error" => format!("{}", e)
                );

                // Map JSON errors to boolean to match the `Result<JSONValue, boolean>`
                // result type expected by mappings
                true
            });
        asc_new(self, &result, gas)
    }

    /// function ipfs.cat(link: String): Bytes
    pub fn ipfs_cat(
        &mut self,
        gas: &GasCounter,
        link_ptr: AscPtr<AscString>,
    ) -> Result<AscPtr<Uint8Array>, HostExportError> {
        // Note on gas: There is no gas costing for the ipfs call itself,
        // since it's not enabled on the network.

        if !self.experimental_features.allow_non_deterministic_ipfs {
            return Err(HostExportError::Deterministic(anyhow!(
                "`ipfs.cat` is deprecated. Improved support for IPFS will be added in the future"
            )));
        }

        let link = asc_get(self, link_ptr, gas)?;
        let ipfs_res = self.ctx.host_exports.ipfs_cat(&self.ctx.logger, link);
        match ipfs_res {
            Ok(bytes) => asc_new(self, &*bytes, gas).map_err(Into::into),

            // Return null in case of error.
            Err(e) => {
                info!(&self.ctx.logger, "Failed ipfs.cat, returning `null`";
                                    "link" => asc_get::<String, _, _>(self, link_ptr, gas)?,
                                    "error" => e.to_string());
                Ok(AscPtr::null())
            }
        }
    }

    /// function ipfs.getBlock(link: String): Bytes
    pub fn ipfs_get_block(
        &mut self,
        gas: &GasCounter,
        link_ptr: AscPtr<AscString>,
    ) -> Result<AscPtr<Uint8Array>, HostExportError> {
        // Note on gas: There is no gas costing for the ipfs call itself,
        // since it's not enabled on the network.

        if !self.experimental_features.allow_non_deterministic_ipfs {
            return Err(HostExportError::Deterministic(anyhow!(
                "`ipfs.getBlock` is deprecated. Improved support for IPFS will be added in the future"
            )));
        }

        let link = asc_get(self, link_ptr, gas)?;
        let ipfs_res = self.ctx.host_exports.ipfs_get_block(&self.ctx.logger, link);
        match ipfs_res {
            Ok(bytes) => asc_new(self, &*bytes, gas).map_err(Into::into),

            // Return null in case of error.
            Err(e) => {
                info!(&self.ctx.logger, "Failed ipfs.getBlock, returning `null`";
                                    "link" => asc_get::<String, _, _>(self, link_ptr, gas)?,
                                    "error" => e.to_string());
                Ok(AscPtr::null())
            }
        }
    }

    /// function ipfs.map(link: String, callback: String, flags: String[]): void
    pub fn ipfs_map(
        &mut self,
        gas: &GasCounter,
        link_ptr: AscPtr<AscString>,
        callback: AscPtr<AscString>,
        user_data: AscPtr<AscEnum<StoreValueKind>>,
        flags: AscPtr<Array<AscPtr<AscString>>>,
    ) -> Result<(), HostExportError> {
        // Note on gas:
        // Ideally we would consume gas the same as ipfs_cat and then share
        // gas across the spawned modules for callbacks.

        if !self.experimental_features.allow_non_deterministic_ipfs {
            return Err(HostExportError::Deterministic(anyhow!(
                "`ipfs.map` is deprecated. Improved support for IPFS will be added in the future"
            )));
        }

        let link: String = asc_get(self, link_ptr, gas)?;
        let callback: String = asc_get(self, callback, gas)?;
        let user_data: store::Value = asc_get(self, user_data, gas)?;

        let flags = asc_get(self, flags, gas)?;

        // Pause the timeout while running ipfs_map, ensure it will be restarted by using a guard.
        self.timeout_stopwatch.lock().unwrap().stop();
        let defer_stopwatch = self.timeout_stopwatch.clone();
        let _stopwatch_guard = defer::defer(|| defer_stopwatch.lock().unwrap().start());

        let start_time = Instant::now();
        let output_states = HostExports::ipfs_map(
            &self.ctx.host_exports.link_resolver.clone(),
            self,
            link.clone(),
            &callback,
            user_data,
            flags,
        )?;

        debug!(
            &self.ctx.logger,
            "Successfully processed file with ipfs.map";
            "link" => &link,
            "callback" => &*callback,
            "n_calls" => output_states.len(),
            "time" => format!("{}ms", start_time.elapsed().as_millis())
        );
        for output_state in output_states {
            self.ctx.state.extend(output_state);
        }

        Ok(())
    }

    /// Expects a decimal string.
    /// function json.toI64(json: String): i64
    pub fn json_to_i64(
        &mut self,
        gas: &GasCounter,
        json_ptr: AscPtr<AscString>,
    ) -> Result<i64, DeterministicHostError> {
        self.ctx
            .host_exports
            .json_to_i64(asc_get(self, json_ptr, gas)?, gas)
    }

    /// Expects a decimal string.
    /// function json.toU64(json: String): u64
    pub fn json_to_u64(
        &mut self,
        gas: &GasCounter,
        json_ptr: AscPtr<AscString>,
    ) -> Result<u64, DeterministicHostError> {
        self.ctx
            .host_exports
            .json_to_u64(asc_get(self, json_ptr, gas)?, gas)
    }

    /// Expects a decimal string.
    /// function json.toF64(json: String): f64
    pub fn json_to_f64(
        &mut self,
        gas: &GasCounter,
        json_ptr: AscPtr<AscString>,
    ) -> Result<f64, DeterministicHostError> {
        self.ctx
            .host_exports
            .json_to_f64(asc_get(self, json_ptr, gas)?, gas)
    }

    /// Expects a decimal string.
    /// function json.toBigInt(json: String): BigInt
    pub fn json_to_big_int(
        &mut self,
        gas: &GasCounter,
        json_ptr: AscPtr<AscString>,
    ) -> Result<AscPtr<AscBigInt>, HostExportError> {
        let big_int = self
            .ctx
            .host_exports
            .json_to_big_int(asc_get(self, json_ptr, gas)?, gas)?;
        asc_new(self, &*big_int, gas)
    }

    /// function crypto.keccak256(input: Bytes): Bytes
    pub fn crypto_keccak_256(
        &mut self,
        gas: &GasCounter,
        input_ptr: AscPtr<Uint8Array>,
    ) -> Result<AscPtr<Uint8Array>, HostExportError> {
        let input = self
            .ctx
            .host_exports
            .crypto_keccak_256(asc_get(self, input_ptr, gas)?, gas)?;
        asc_new(self, input.as_ref(), gas)
    }

    /// function bigInt.plus(x: BigInt, y: BigInt): BigInt
    pub fn big_int_plus(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigInt>,
        y_ptr: AscPtr<AscBigInt>,
    ) -> Result<AscPtr<AscBigInt>, HostExportError> {
        let result = self.ctx.host_exports.big_int_plus(
            asc_get(self, x_ptr, gas)?,
            asc_get(self, y_ptr, gas)?,
            gas,
        )?;
        asc_new(self, &result, gas)
    }

    /// function bigInt.minus(x: BigInt, y: BigInt): BigInt
    pub fn big_int_minus(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigInt>,
        y_ptr: AscPtr<AscBigInt>,
    ) -> Result<AscPtr<AscBigInt>, HostExportError> {
        let result = self.ctx.host_exports.big_int_minus(
            asc_get(self, x_ptr, gas)?,
            asc_get(self, y_ptr, gas)?,
            gas,
        )?;
        asc_new(self, &result, gas)
    }

    /// function bigInt.times(x: BigInt, y: BigInt): BigInt
    pub fn big_int_times(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigInt>,
        y_ptr: AscPtr<AscBigInt>,
    ) -> Result<AscPtr<AscBigInt>, HostExportError> {
        let result = self.ctx.host_exports.big_int_times(
            asc_get(self, x_ptr, gas)?,
            asc_get(self, y_ptr, gas)?,
            gas,
        )?;
        asc_new(self, &result, gas)
    }

    /// function bigInt.dividedBy(x: BigInt, y: BigInt): BigInt
    pub fn big_int_divided_by(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigInt>,
        y_ptr: AscPtr<AscBigInt>,
    ) -> Result<AscPtr<AscBigInt>, HostExportError> {
        let result = self.ctx.host_exports.big_int_divided_by(
            asc_get(self, x_ptr, gas)?,
            asc_get(self, y_ptr, gas)?,
            gas,
        )?;
        asc_new(self, &result, gas)
    }

    /// function bigInt.dividedByDecimal(x: BigInt, y: BigDecimal): BigDecimal
    pub fn big_int_divided_by_decimal(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigInt>,
        y_ptr: AscPtr<AscBigDecimal>,
    ) -> Result<AscPtr<AscBigDecimal>, HostExportError> {
        let x = BigDecimal::new(asc_get(self, x_ptr, gas)?, 0);
        let result =
            self.ctx
                .host_exports
                .big_decimal_divided_by(x, asc_get(self, y_ptr, gas)?, gas)?;
        asc_new(self, &result, gas)
    }

    /// function bigInt.mod(x: BigInt, y: BigInt): BigInt
    pub fn big_int_mod(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigInt>,
        y_ptr: AscPtr<AscBigInt>,
    ) -> Result<AscPtr<AscBigInt>, HostExportError> {
        let result = self.ctx.host_exports.big_int_mod(
            asc_get(self, x_ptr, gas)?,
            asc_get(self, y_ptr, gas)?,
            gas,
        )?;
        asc_new(self, &result, gas)
    }

    /// function bigInt.pow(x: BigInt, exp: u8): BigInt
    pub fn big_int_pow(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigInt>,
        exp: u32,
    ) -> Result<AscPtr<AscBigInt>, HostExportError> {
        let exp = u8::try_from(exp).map_err(|e| DeterministicHostError::from(Error::from(e)))?;
        let result = self
            .ctx
            .host_exports
            .big_int_pow(asc_get(self, x_ptr, gas)?, exp, gas)?;
        asc_new(self, &result, gas)
    }

    /// function bigInt.bitOr(x: BigInt, y: BigInt): BigInt
    pub fn big_int_bit_or(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigInt>,
        y_ptr: AscPtr<AscBigInt>,
    ) -> Result<AscPtr<AscBigInt>, HostExportError> {
        let result = self.ctx.host_exports.big_int_bit_or(
            asc_get(self, x_ptr, gas)?,
            asc_get(self, y_ptr, gas)?,
            gas,
        )?;
        asc_new(self, &result, gas)
    }

    /// function bigInt.bitAnd(x: BigInt, y: BigInt): BigInt
    pub fn big_int_bit_and(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigInt>,
        y_ptr: AscPtr<AscBigInt>,
    ) -> Result<AscPtr<AscBigInt>, HostExportError> {
        let result = self.ctx.host_exports.big_int_bit_and(
            asc_get(self, x_ptr, gas)?,
            asc_get(self, y_ptr, gas)?,
            gas,
        )?;
        asc_new(self, &result, gas)
    }

    /// function bigInt.leftShift(x: BigInt, bits: u8): BigInt
    pub fn big_int_left_shift(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigInt>,
        bits: u32,
    ) -> Result<AscPtr<AscBigInt>, HostExportError> {
        let bits = u8::try_from(bits).map_err(|e| DeterministicHostError::from(Error::from(e)))?;
        let result =
            self.ctx
                .host_exports
                .big_int_left_shift(asc_get(self, x_ptr, gas)?, bits, gas)?;
        asc_new(self, &result, gas)
    }

    /// function bigInt.rightShift(x: BigInt, bits: u8): BigInt
    pub fn big_int_right_shift(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigInt>,
        bits: u32,
    ) -> Result<AscPtr<AscBigInt>, HostExportError> {
        let bits = u8::try_from(bits).map_err(|e| DeterministicHostError::from(Error::from(e)))?;
        let result =
            self.ctx
                .host_exports
                .big_int_right_shift(asc_get(self, x_ptr, gas)?, bits, gas)?;
        asc_new(self, &result, gas)
    }

    /// function typeConversion.bytesToBase58(bytes: Bytes): string
    pub fn bytes_to_base58(
        &mut self,
        gas: &GasCounter,
        bytes_ptr: AscPtr<Uint8Array>,
    ) -> Result<AscPtr<AscString>, HostExportError> {
        let result = self
            .ctx
            .host_exports
            .bytes_to_base58(asc_get(self, bytes_ptr, gas)?, gas)?;
        asc_new(self, &result, gas)
    }

    /// function bigDecimal.toString(x: BigDecimal): string
    pub fn big_decimal_to_string(
        &mut self,
        gas: &GasCounter,
        big_decimal_ptr: AscPtr<AscBigDecimal>,
    ) -> Result<AscPtr<AscString>, HostExportError> {
        let result = self
            .ctx
            .host_exports
            .big_decimal_to_string(asc_get(self, big_decimal_ptr, gas)?, gas)?;
        asc_new(self, &result, gas)
    }

    /// function bigDecimal.fromString(x: string): BigDecimal
    pub fn big_decimal_from_string(
        &mut self,
        gas: &GasCounter,
        string_ptr: AscPtr<AscString>,
    ) -> Result<AscPtr<AscBigDecimal>, HostExportError> {
        let result = self
            .ctx
            .host_exports
            .big_decimal_from_string(asc_get(self, string_ptr, gas)?, gas)?;
        asc_new(self, &result, gas)
    }

    /// function bigDecimal.plus(x: BigDecimal, y: BigDecimal): BigDecimal
    pub fn big_decimal_plus(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigDecimal>,
        y_ptr: AscPtr<AscBigDecimal>,
    ) -> Result<AscPtr<AscBigDecimal>, HostExportError> {
        let result = self.ctx.host_exports.big_decimal_plus(
            asc_get(self, x_ptr, gas)?,
            asc_get(self, y_ptr, gas)?,
            gas,
        )?;
        asc_new(self, &result, gas)
    }

    /// function bigDecimal.minus(x: BigDecimal, y: BigDecimal): BigDecimal
    pub fn big_decimal_minus(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigDecimal>,
        y_ptr: AscPtr<AscBigDecimal>,
    ) -> Result<AscPtr<AscBigDecimal>, HostExportError> {
        let result = self.ctx.host_exports.big_decimal_minus(
            asc_get(self, x_ptr, gas)?,
            asc_get(self, y_ptr, gas)?,
            gas,
        )?;
        asc_new(self, &result, gas)
    }

    /// function bigDecimal.times(x: BigDecimal, y: BigDecimal): BigDecimal
    pub fn big_decimal_times(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigDecimal>,
        y_ptr: AscPtr<AscBigDecimal>,
    ) -> Result<AscPtr<AscBigDecimal>, HostExportError> {
        let result = self.ctx.host_exports.big_decimal_times(
            asc_get(self, x_ptr, gas)?,
            asc_get(self, y_ptr, gas)?,
            gas,
        )?;
        asc_new(self, &result, gas)
    }

    /// function bigDecimal.dividedBy(x: BigDecimal, y: BigDecimal): BigDecimal
    pub fn big_decimal_divided_by(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigDecimal>,
        y_ptr: AscPtr<AscBigDecimal>,
    ) -> Result<AscPtr<AscBigDecimal>, HostExportError> {
        let result = self.ctx.host_exports.big_decimal_divided_by(
            asc_get(self, x_ptr, gas)?,
            asc_get(self, y_ptr, gas)?,
            gas,
        )?;
        asc_new(self, &result, gas)
    }

    /// function bigDecimal.equals(x: BigDecimal, y: BigDecimal): bool
    pub fn big_decimal_equals(
        &mut self,
        gas: &GasCounter,
        x_ptr: AscPtr<AscBigDecimal>,
        y_ptr: AscPtr<AscBigDecimal>,
    ) -> Result<bool, HostExportError> {
        self.ctx.host_exports.big_decimal_equals(
            asc_get(self, x_ptr, gas)?,
            asc_get(self, y_ptr, gas)?,
            gas,
        )
    }

    /// function dataSource.create(name: string, params: Array<string>): void
    pub fn data_source_create(
        &mut self,
        gas: &GasCounter,
        name_ptr: AscPtr<AscString>,
        params_ptr: AscPtr<Array<AscPtr<AscString>>>,
    ) -> Result<(), HostExportError> {
        let name: String = asc_get(self, name_ptr, gas)?;
        let params: Vec<String> = asc_get(self, params_ptr, gas)?;
        self.ctx.host_exports.data_source_create(
            &self.ctx.logger,
            &mut self.ctx.state,
            name,
            params,
            None,
            self.ctx.block_ptr.number,
            gas,
        )
    }

    /// function createWithContext(name: string, params: Array<string>, context: DataSourceContext): void
    pub fn data_source_create_with_context(
        &mut self,
        gas: &GasCounter,
        name_ptr: AscPtr<AscString>,
        params_ptr: AscPtr<Array<AscPtr<AscString>>>,
        context_ptr: AscPtr<AscEntity>,
    ) -> Result<(), HostExportError> {
        let name: String = asc_get(self, name_ptr, gas)?;
        let params: Vec<String> = asc_get(self, params_ptr, gas)?;
        let context: HashMap<_, _> = asc_get(self, context_ptr, gas)?;
        let context = DataSourceContext::from(context);

        self.ctx.host_exports.data_source_create(
            &self.ctx.logger,
            &mut self.ctx.state,
            name,
            params,
            Some(context),
            self.ctx.block_ptr.number,
            gas,
        )
    }

    /// function dataSource.address(): Bytes
    pub fn data_source_address(
        &mut self,
        gas: &GasCounter,
    ) -> Result<AscPtr<Uint8Array>, HostExportError> {
        asc_new(
            self,
            self.ctx.host_exports.data_source_address(gas)?.as_slice(),
            gas,
        )
    }

    /// function dataSource.network(): String
    pub fn data_source_network(
        &mut self,
        gas: &GasCounter,
    ) -> Result<AscPtr<AscString>, HostExportError> {
        asc_new(self, &self.ctx.host_exports.data_source_network(gas)?, gas)
    }

    /// function dataSource.context(): DataSourceContext
    pub fn data_source_context(
        &mut self,
        gas: &GasCounter,
    ) -> Result<AscPtr<AscEntity>, HostExportError> {
        asc_new(
            self,
            &self
                .ctx
                .host_exports
                .data_source_context(gas)?
                .map(|e| e.sorted())
                .unwrap_or(vec![]),
            gas,
        )
    }

    pub fn ens_name_by_hash(
        &mut self,
        gas: &GasCounter,
        hash_ptr: AscPtr<AscString>,
    ) -> Result<AscPtr<AscString>, HostExportError> {
        let hash: String = asc_get(self, hash_ptr, gas)?;
        let name = self.ctx.host_exports.ens_name_by_hash(&hash, gas)?;
        if name.is_none() && self.ctx.host_exports.is_ens_data_empty()? {
            return Err(anyhow!(
                "Missing ENS data: see https://github.com/graphprotocol/ens-rainbow"
            )
            .into());
        }

        // map `None` to `null`, and `Some(s)` to a runtime string
        name.map(|name| asc_new(self, &*name, gas).map_err(Into::into))
            .unwrap_or(Ok(AscPtr::null()))
    }

    pub fn log_log(
        &mut self,
        gas: &GasCounter,
        level: u32,
        msg: AscPtr<AscString>,
    ) -> Result<(), DeterministicHostError> {
        let level = LogLevel::from(level).into();
        let msg: String = asc_get(self, msg, gas)?;
        self.ctx
            .host_exports
            .log_log(&self.ctx.mapping_logger, level, msg, gas)
    }

    /// function encode(token: ethereum.Value): Bytes | null
    pub fn ethereum_encode(
        &mut self,
        gas: &GasCounter,
        token_ptr: AscPtr<AscEnum<EthereumValueKind>>,
    ) -> Result<AscPtr<Uint8Array>, HostExportError> {
        let data = self
            .ctx
            .host_exports
            .ethereum_encode(asc_get(self, token_ptr, gas)?, gas);

        // return `null` if it fails
        data.map(|bytes| asc_new(self, &*bytes, gas))
            .unwrap_or(Ok(AscPtr::null()))
    }

    /// function decode(types: String, data: Bytes): ethereum.Value | null
    pub fn ethereum_decode(
        &mut self,
        gas: &GasCounter,
        types_ptr: AscPtr<AscString>,
        data_ptr: AscPtr<Uint8Array>,
    ) -> Result<AscPtr<AscEnum<EthereumValueKind>>, HostExportError> {
        let result = self.ctx.host_exports.ethereum_decode(
            asc_get(self, types_ptr, gas)?,
            asc_get(self, data_ptr, gas)?,
            gas,
        );

        // return `null` if it fails
        result
            .map(|param| asc_new(self, &param, gas))
            .unwrap_or(Ok(AscPtr::null()))
    }

    /// function arweave.transactionData(txId: string): Bytes | null
    pub fn arweave_transaction_data(
        &mut self,
        _gas: &GasCounter,
        _tx_id: AscPtr<AscString>,
    ) -> Result<AscPtr<Uint8Array>, HostExportError> {
        Err(HostExportError::Deterministic(anyhow!(
            "`arweave.transactionData` has been removed."
        )))
    }

    /// function box.profile(address: string): JSONValue | null
    pub fn box_profile(
        &mut self,
        _gas: &GasCounter,
        _address: AscPtr<AscString>,
    ) -> Result<AscPtr<AscJson>, HostExportError> {
        Err(HostExportError::Deterministic(anyhow!(
            "`box.profile` has been removed."
        )))
    }
}
