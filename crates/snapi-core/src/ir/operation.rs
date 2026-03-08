use super::types::IrType;

#[derive(Debug, Clone, serde::Serialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum ParamLocation {
    Path,
    Query,
    Header,
    Cookie,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrParam {
    pub name: String,
    pub location: ParamLocation,
    pub required: bool,
    pub ty: IrType,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrRequestBody {
    pub required: bool,
    pub content_type: String,
    pub ty: IrType,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub ty: Option<IrType>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrOperation {
    pub id: String,
    pub path: String,
    pub method: HttpMethod,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub params: Vec<IrParam>,
    pub body: Option<IrRequestBody>,
    pub responses: Vec<IrResponse>,
    pub deprecated: bool,
}
