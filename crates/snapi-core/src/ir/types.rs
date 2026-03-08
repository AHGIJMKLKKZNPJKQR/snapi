use indexmap::IndexMap;

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrStringConstraints {
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub pattern: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrIntConstraints {
    pub minimum: Option<i64>,
    pub maximum: Option<i64>,
    pub format: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrFloatConstraints {
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrField {
    pub ty: IrType,
    pub required: bool,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrObject {
    pub name: Option<String>,
    pub fields: IndexMap<String, IrField>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrEnumVariant {
    pub name: String,
    pub ty: IrType,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrEnum {
    pub name: String,
    pub variants: Vec<IrEnumVariant>,
    pub discriminator: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum IrType {
    String(IrStringConstraints),
    Integer(IrIntConstraints),
    Float(IrFloatConstraints),
    Boolean,
    Null,
    Array {
        items: Box<IrType>,
        min: Option<u64>,
        max: Option<u64>,
    },
    Map(Box<IrType>),
    Object(IrObject),
    Enum(IrEnum),
    Union(Vec<IrType>),
    Intersection(Vec<IrObject>),
    Optional(Box<IrType>),
    Recursive(String),
    Any,
}
