use super::operation::HttpMethod;
use super::types::IrType;

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrWebhook {
    pub id: String,
    pub path: String,
    pub method: HttpMethod,
    pub payload: IrType,
}
