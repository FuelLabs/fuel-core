use serde_reflection::{
    Samples,
    Tracer,
    TracerConfig,
};

pub(crate) fn fields_of_struct_in_order<T>() -> Vec<String>
where
    T: serde::de::DeserializeOwned,
{
    let mut tracer = Tracer::new(TracerConfig::default());
    let samples = Samples::new();
    tracer.trace_type::<T>(&samples).unwrap();
    let type_name = std::any::type_name::<T>().split("::").last().unwrap();
    let registry = tracer.registry().unwrap();
    let Some(serde_reflection::ContainerFormat::Struct(fields)) = registry.get(type_name)
    else {
        panic!("No fields?")
    };

    fields.iter().map(|f| f.name.clone()).collect()
}

// Naive Fourier series
pub(crate) fn gen_noisy_signal(input: f64, components: &[f64]) -> f64 {
    components
        .iter()
        .fold(0f64, |acc, &c| acc + f64::sin(input / c))
        / components.len() as f64
}
