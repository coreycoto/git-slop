pub(crate) fn visible_controls(value: &str) -> String {
    let mut rendered = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' => rendered.push_str("\\n"),
            '\r' => rendered.push_str("\\r"),
            '\t' => rendered.push_str("\\t"),
            control if control.is_control() => {
                rendered.push_str(&format!("\\u{{{:x}}}", control as u32));
            }
            printable => rendered.push(printable),
        }
    }
    rendered
}

pub(crate) fn markdown_escape(value: &str) -> String {
    visible_controls(value)
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('<', "\\<")
        .replace('>', "\\>")
        .replace('`', "\\`")
}

pub(crate) fn inline_code(value: &str) -> String {
    let value = visible_controls(value);
    let mut longest_run = 0usize;
    let mut current_run = 0usize;
    for character in value.chars() {
        if character == '`' {
            current_run += 1;
            longest_run = longest_run.max(current_run);
        } else {
            current_run = 0;
        }
    }
    if longest_run == 0 {
        return format!("`{value}`");
    }
    let fence = "`".repeat(longest_run.saturating_add(1));
    format!("{fence} {value} {fence}")
}

pub(crate) fn github_property_escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(':', "%3A")
        .replace(',', "%2C")
}

pub(crate) fn github_message_escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_are_visible_without_adding_output_lines() {
        assert_eq!(
            visible_controls("a\n\r\tb\u{1b}[31m"),
            "a\\n\\r\\tb\\u{1b}[31m"
        );
    }

    #[test]
    fn github_command_fields_escape_delimiters_and_line_breaks() {
        assert_eq!(
            github_property_escape("a,b:c%\r\n::warning::x"),
            "a%2Cb%3Ac%25%0D%0A%3A%3Awarning%3A%3Ax"
        );
        assert_eq!(
            github_message_escape("message%\r\n::error::x"),
            "message%25%0D%0A::error::x"
        );
    }

    #[test]
    fn markdown_and_inline_code_handle_controls_and_backtick_runs() {
        assert_eq!(markdown_escape("a|`b\n"), "a\\|\\`b\\\\n");
        assert_eq!(inline_code("a``b\n"), "``` a``b\\n ```");
    }
}
