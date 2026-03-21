//! 小工具函数

use std::sync::OnceLock;

pub(crate) fn normalize_for_embedding(text: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\s+").expect("valid regex"));
    re.replace_all(text.trim(), " ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_trims_and_collapses_whitespace() {
        assert_eq!(normalize_for_embedding("  a \n\t b  "), "a b");
    }
}
