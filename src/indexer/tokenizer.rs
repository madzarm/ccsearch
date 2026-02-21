/// Preprocesses text for BM25 indexing.
/// Lowercases, removes excessive whitespace, and strips code-heavy content.
pub fn preprocess_for_bm25(text: &str) -> String {
    let mut result = String::with_capacity(text.len());

    for line in text.lines() {
        let trimmed = line.trim();

        // Skip lines that look like raw code/data
        if is_code_line(trimmed) {
            continue;
        }

        // Normalize whitespace
        let normalized: String = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");

        if !normalized.is_empty() {
            result.push_str(&normalized.to_lowercase());
            result.push(' ');
        }
    }

    result.trim().to_string()
}

/// Detects lines that are likely code rather than natural language
fn is_code_line(line: &str) -> bool {
    // Skip very long lines (likely code or data)
    if line.len() > 500 {
        return true;
    }

    // Skip lines that are mostly special characters
    let alpha_count = line.chars().filter(|c| c.is_alphabetic()).count();
    let total = line.len();
    if total > 20 && alpha_count < total / 4 {
        return true;
    }

    // Skip common code patterns
    let code_prefixes = [
        "import ",
        "from ",
        "use ",
        "fn ",
        "pub fn ",
        "def ",
        "class ",
        "const ",
        "let ",
        "var ",
        "function ",
        "export ",
        "module ",
        "CREATE ",
        "SELECT ",
        "INSERT ",
        "UPDATE ",
        "DELETE ",
        "```",
        "---",
        "===",
        "///",
        "//",
        "/*",
        "*/",
        "#!",
    ];
    for prefix in &code_prefixes {
        if line.starts_with(prefix) {
            return true;
        }
    }

    false
}

/// Extracts meaningful terms from text for BM25 document creation
pub fn extract_terms(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .filter(|w| w.len() > 2 && w.len() < 50)
        .map(|w| w.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_simple() {
        let input = "Hello   World\n  This is a test  ";
        let result = preprocess_for_bm25(input);
        assert_eq!(result, "hello world this is a test");
    }

    #[test]
    fn test_preprocess_filters_code() {
        let input = "Fix the auth bug\nimport os\ndef main():\n  pass\nThe issue is resolved";
        let result = preprocess_for_bm25(input);
        assert!(result.contains("fix the auth bug"));
        assert!(result.contains("the issue is resolved"));
        assert!(!result.contains("import os"));
    }

    #[test]
    fn test_extract_terms() {
        let terms = extract_terms("Hello world, this is a test!");
        assert!(terms.contains(&"hello".to_string()));
        assert!(terms.contains(&"world".to_string()));
        assert!(terms.contains(&"this".to_string()));
        assert!(terms.contains(&"test".to_string()));
        // "is" and "a" are filtered (too short)
        assert!(!terms.contains(&"is".to_string()));
        assert!(!terms.contains(&"a".to_string()));
    }
}
