mod codec;

pub use codec::*;

use self::module::input::{Input, Params};

/// Replace all the existing params with the provided ones.
pub fn patch_module_params(params: String, module: &mut Module) {
    let mut inputs = vec![crate::substreams::module::Input {
        input: Some(Input::Params(Params { value: params })),
    }];

    inputs.extend(module.inputs.iter().flat_map(|input| match input.input {
        None => None,
        Some(Input::Params(_)) => None,
        Some(_) => Some(input.clone()),
    }));

    module.inputs = inputs;
}
