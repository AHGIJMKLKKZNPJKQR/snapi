pub enum DocStyle {
    TripleSlash,
    JsDoc,
    GoogleStyle,
}

pub fn format_doc(description: &str, style: DocStyle) -> String {
    match style {
        DocStyle::TripleSlash => description
            .lines()
            .map(|line| format!("/// {}", line))
            .collect::<Vec<_>>()
            .join("\n"),
        DocStyle::JsDoc => {
            let mut lines = vec!["/**".to_string()];
            for line in description.lines() {
                lines.push(format!(" * {}", line));
            }
            lines.push(" */".to_string());
            lines.join("\n")
        }
        DocStyle::GoogleStyle => {
            let mut lines = vec!["\"\"\"".to_string()];
            lines.push(description.to_string());
            lines.push("\"\"\"".to_string());
            lines.join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_doc_triple_slash_single_line() {
        let result = format_doc("Fetches the user by ID.", DocStyle::TripleSlash);
        assert_eq!(result, "/// Fetches the user by ID.");
    }

    #[test]
    fn test_format_doc_triple_slash_multiline() {
        let result = format_doc("Line one.\nLine two.", DocStyle::TripleSlash);
        assert_eq!(result, "/// Line one.\n/// Line two.");
    }

    #[test]
    fn test_format_doc_jsdoc_single_line() {
        let result = format_doc("Returns a list of items.", DocStyle::JsDoc);
        assert_eq!(result, "/**\n * Returns a list of items.\n */");
    }

    #[test]
    fn test_format_doc_jsdoc_multiline() {
        let result = format_doc("First line.\nSecond line.", DocStyle::JsDoc);
        assert_eq!(result, "/**\n * First line.\n * Second line.\n */");
    }

    #[test]
    fn test_format_doc_google_style_single_line() {
        let result = format_doc("Creates an entry.", DocStyle::GoogleStyle);
        assert_eq!(result, "\"\"\"\nCreates an entry.\n\"\"\"");
    }

    #[test]
    fn test_format_doc_google_style_multiline() {
        let result = format_doc("Summary.\n\nDetailed description.", DocStyle::GoogleStyle);
        assert_eq!(result, "\"\"\"\nSummary.\n\nDetailed description.\n\"\"\"");
    }
}
