//! Escaping text for XML and HTML.

/// `text` with `&`, `<`, `>` and `"` replaced by entities, safe in element text and attributes.
pub(crate) fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup_characters() {
        assert_eq!(escape(r#"<a href="x">A&B</a>"#), "&lt;a href=&quot;x&quot;&gt;A&amp;B&lt;/a&gt;");
        assert_eq!(escape("plain"), "plain");
    }
}
