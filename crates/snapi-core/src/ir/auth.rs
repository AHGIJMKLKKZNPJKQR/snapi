#[derive(Debug, Clone, serde::Serialize)]
pub enum IrAuthScheme {
    ApiKey { name: String, location: String },
    Bearer { scheme: String },
    OAuth2 { flows: Vec<String> },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrAuthRequirement {
    pub scheme_name: String,
    pub scopes: Vec<String>,
}
