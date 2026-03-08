use heck::{ToLowerCamelCase, ToSnakeCase, ToUpperCamelCase};

pub fn to_snake_case(s: &str) -> String {
    s.to_snake_case()
}

pub fn to_pascal_case(s: &str) -> String {
    s.to_upper_camel_case()
}

pub fn to_camel_case(s: &str) -> String {
    s.to_lower_camel_case()
}

pub enum Language {
    TypeScript,
    Rust,
    Python,
    Go,
}

pub fn escape_ident(s: &str, lang: Language) -> String {
    let ts_keywords = &[
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "new",
        "null",
        "return",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "yield",
        "let",
        "static",
        "implements",
        "interface",
        "package",
        "private",
        "protected",
        "public",
        "type",
        "from",
        "as",
        "any",
        "unknown",
    ];
    let rust_keywords = &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "unsafe", "use", "where", "while",
    ];

    let reserved: &[&str] = match lang {
        Language::TypeScript => ts_keywords,
        Language::Rust => rust_keywords,
        _ => &[],
    };

    if reserved.contains(&s) {
        format!("{}_", s)
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(to_snake_case("myFieldName"), "my_field_name");
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
        assert_eq!(to_pascal_case("my_field_name"), "MyFieldName");
    }

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("hello_world"), "helloWorld");
        assert_eq!(to_camel_case("MyField"), "myField");
    }

    #[test]
    fn test_escape_ident_typescript_keyword() {
        assert_eq!(escape_ident("class", Language::TypeScript), "class_");
        assert_eq!(escape_ident("type", Language::TypeScript), "type_");
        assert_eq!(escape_ident("import", Language::TypeScript), "import_");
        assert_eq!(escape_ident("enum", Language::TypeScript), "enum_");
    }

    #[test]
    fn test_escape_ident_typescript_non_keyword() {
        assert_eq!(escape_ident("normal", Language::TypeScript), "normal");
        assert_eq!(escape_ident("userId", Language::TypeScript), "userId");
    }

    #[test]
    fn test_escape_ident_rust_keyword() {
        assert_eq!(escape_ident("type", Language::Rust), "type_");
        assert_eq!(escape_ident("fn", Language::Rust), "fn_");
        assert_eq!(escape_ident("match", Language::Rust), "match_");
        assert_eq!(escape_ident("self", Language::Rust), "self_");
    }

    #[test]
    fn test_escape_ident_rust_non_keyword() {
        assert_eq!(escape_ident("normal", Language::Rust), "normal");
        assert_eq!(escape_ident("user_id", Language::Rust), "user_id");
    }

    #[test]
    fn test_to_snake_case_already_snake() {
        assert_eq!(to_snake_case("hello_world"), "hello_world");
    }

    #[test]
    fn test_to_pascal_case_already_pascal() {
        assert_eq!(to_pascal_case("HelloWorld"), "HelloWorld");
    }

    #[test]
    fn test_to_camel_case_from_pascal() {
        assert_eq!(to_camel_case("GetUserById"), "getUserById");
    }
}
