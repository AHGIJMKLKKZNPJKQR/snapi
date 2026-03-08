pub mod typescript;

pub fn registry() -> Vec<Box<dyn snapi_core::generator::Generator>> {
    vec![
        Box::new(typescript::fetch::FetchGenerator),
        Box::new(typescript::axios::AxiosGenerator),
    ]
}
