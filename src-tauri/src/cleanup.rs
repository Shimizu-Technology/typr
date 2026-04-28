pub fn cleanup_text(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.trim().is_empty() {
        return String::new();
    }

    let mut lines: Vec<String> = normalized
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect();

    while matches!(lines.first(), Some(line) if line.trim().is_empty()) {
        lines.remove(0);
    }

    while matches!(lines.last(), Some(line) if line.trim().is_empty()) {
        lines.pop();
    }

    let mut blank_count = 0usize;
    let mut result = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                result.push(String::new());
            }
        } else {
            blank_count = 0;
            result.push(line);
        }
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim_whitespace() {
        assert_eq!(cleanup_text("  hello world  "), "  hello world");
    }

    #[test]
    fn test_preserve_internal_spaces() {
        assert_eq!(cleanup_text("hello    world"), "hello    world");
    }

    #[test]
    fn test_preserve_line_breaks() {
        assert_eq!(cleanup_text("hello\nworld"), "hello\nworld");
    }

    #[test]
    fn test_preserve_indentation() {
        assert_eq!(cleanup_text("    let x = 1;    "), "    let x = 1;");
    }

    #[test]
    fn test_collapse_extra_blank_lines() {
        assert_eq!(cleanup_text("a\n\n\n\nb"), "a\n\nb");
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(cleanup_text(""), "");
        assert_eq!(cleanup_text("   "), "");
    }

    #[test]
    fn test_already_clean() {
        assert_eq!(cleanup_text("Hello world."), "Hello world.");
    }
}
