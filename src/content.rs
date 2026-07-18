use regex::Regex;

pub fn normalize_whitespace(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn html_to_markdown(content: &str) -> String {
    let markdown = html2md::parse_html(content);
    let images = Regex::new(r"!\[[^\]]*\]\([^)]*\)").expect("static image regex");
    let links = Regex::new(r"\[([^\]]+)\]\([^)]*\)").expect("static link regex");
    let without_images = images.replace_all(&markdown, "");
    let without_links = links.replace_all(&without_images, "$1");
    normalize_whitespace(&without_links)
        .lines()
        .map(|line| {
            if line.len() >= 3 && line.chars().all(|character| character == '-') {
                "-----"
            } else if line.len() >= 3 && line.chars().all(|character| character == '=') {
                "====="
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn chunk_chars(content: &str, chunk_size: usize) -> Vec<String> {
    if content.is_empty() {
        return vec![String::new()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for character in content.chars() {
        if current.chars().count() == chunk_size {
            chunks.push(std::mem::take(&mut current));
        }
        current.push(character);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_html_and_strips_links_images_and_blank_lines() {
        let html = r#"
            <h2>Title</h2>
            <p>Read <a href="https://example.com">linked text</a>.</p>
            <img src="diagram.png" alt="diagram">
        "#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("Title"));
        assert!(markdown.contains("Read linked text."));
        assert!(!markdown.contains("example.com"));
        assert!(!markdown.contains("diagram"));
        assert!(!markdown.contains("\n\n"));
    }

    #[test]
    fn chunks_at_unicode_character_boundaries() {
        assert_eq!(chunk_chars("a台😀b", 2), vec!["a台", "😀b"]);
    }
}
