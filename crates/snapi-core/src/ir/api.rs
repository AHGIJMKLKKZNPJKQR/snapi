use super::auth::IrAuthScheme;
use super::operation::IrOperation;
use super::types::IrType;
use super::webhook::IrWebhook;
use indexmap::IndexMap;

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrServer {
    pub url: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrApi {
    pub title: String,
    pub version: String,
    pub description: Option<String>,
    pub servers: Vec<IrServer>,
    pub schemas: IndexMap<String, IrType>,
    pub operations: Vec<IrOperation>,
    pub auth_schemes: IndexMap<String, IrAuthScheme>,
    pub webhooks: Vec<IrWebhook>,
}
