#[cfg(test)]
mod tests {
    use super::*;

    fn workflow_text(name: &str) -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        fs::read_to_string(root.join(".github/workflows").join(name)).unwrap()
    }

    fn parsed(text: &str) -> YamlValue {
        serde_yaml::from_str(text).unwrap()
    }

    fn publish_errors(text: &str) -> Vec<String> {
        let mut errors = Vec::new();
        validate_release_publish(text, &parsed(text), &mut errors);
        errors
    }

    fn relay_errors(text: &str) -> Vec<String> {
        let mut errors = Vec::new();
        validate_release_relay(text, &parsed(text), &mut errors);
        errors
    }

    fn homebrew_errors(text: &str) -> Vec<String> {
        let mut errors = Vec::new();
        validate_homebrew_handoff(&parsed(text), &mut errors);
        errors
    }

    include!("tests/group_1.rs");
    include!("tests/group_2.rs");
    include!("tests/group_3.rs");
    include!("tests/group_4.rs");

}
