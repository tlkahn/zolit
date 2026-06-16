use regex::Regex;
use std::sync::LazyLock;

macro_rules! lazy_regex {
    ($pattern:expr) => {{
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).unwrap());
        &*RE
    }};
}

pub fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Decode common HTML entities (named + numeric) to their character equivalents.
pub fn decode_html_entities(s: &str) -> String {
    let mut result = s.to_string();

    // Named entities
    result = result.replace("&amp;", "&");
    result = result.replace("&lt;", "<");
    result = result.replace("&gt;", ">");
    result = result.replace("&quot;", "\"");
    result = result.replace("&apos;", "'");
    result = result.replace("&nbsp;", " ");

    // Decimal numeric entities: &#NNN;
    let re_dec = lazy_regex!(r"&#(\d+);");
    result = re_dec
        .replace_all(&result, |caps: &regex::Captures| {
            caps[1]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_else(|| caps[0].to_string())
        })
        .to_string();

    // Hex numeric entities: &#xHHH;
    let re_hex = lazy_regex!(r"(?i)&#x([0-9a-f]+);");
    result = re_hex
        .replace_all(&result, |caps: &regex::Captures| {
            u32::from_str_radix(&caps[1], 16)
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_else(|| caps[0].to_string())
        })
        .to_string();

    result
}

/// Convert simple HTML (as produced by Zotero notes) to Markdown.
///
/// Handles paragraphs, line breaks, headings, bold/italic, links, lists,
/// blockquotes, code, and HTML entities. Unknown tags are stripped.
pub fn html_to_markdown(html: &str) -> String {
    if html.is_empty() {
        return String::new();
    }

    let mut s = html.to_string();

    // Note: HTML entity decoding is deferred to AFTER tag stripping,
    // because decoding &lt;/&gt; to <> before stripping would cause
    // the tag stripper to misinterpret literal angle brackets as tags.

    // Step 1: Block-level conversions (order matters)

    // <br> / <br/> / <br /> -> newline
    s = lazy_regex!(r"(?i)<br\s*/?>").replace_all(&s, "\n").to_string();

    // Headings: <h1>...<h6>
    static H_REGEXES: LazyLock<[(Regex, &str); 6]> = LazyLock::new(|| [
        (Regex::new(r"(?is)<h1[^>]*>(.*?)</h1>").unwrap(), "#"),
        (Regex::new(r"(?is)<h2[^>]*>(.*?)</h2>").unwrap(), "##"),
        (Regex::new(r"(?is)<h3[^>]*>(.*?)</h3>").unwrap(), "###"),
        (Regex::new(r"(?is)<h4[^>]*>(.*?)</h4>").unwrap(), "####"),
        (Regex::new(r"(?is)<h5[^>]*>(.*?)</h5>").unwrap(), "#####"),
        (Regex::new(r"(?is)<h6[^>]*>(.*?)</h6>").unwrap(), "######"),
    ]);
    for (re_h, hashes) in H_REGEXES.iter() {
        s = re_h
            .replace_all(&s, |caps: &regex::Captures| {
                format!("\n\n{} {}\n\n", hashes, caps[1].trim())
            })
            .to_string();
    }

    // Blockquotes
    let re_bq = lazy_regex!(r"(?is)<blockquote[^>]*>(.*?)</blockquote>");
    s = re_bq
        .replace_all(&s, |caps: &regex::Captures| {
            let inner = strip_html_tags(&caps[1]);
            inner
                .lines()
                .map(|line| format!("> {}", line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .to_string();

    // Ordered lists: <ol><li>...</li></ol>
    s = lazy_regex!(r"(?is)<ol[^>]*>(.*?)</ol>")
        .replace_all(&s, |caps: &regex::Captures| {
            let re_li = lazy_regex!(r"(?is)<li[^>]*>(.*?)</li>");
            let mut counter = 0usize;
            let items: String = re_li
                .replace_all(&caps[1], |li_caps: &regex::Captures| {
                    counter += 1;
                    format!("\n{}. {}", counter, li_caps[1].trim())
                })
                .to_string();
            format!("\n{}\n", items.trim())
        })
        .to_string();

    // Unordered lists: <ul><li>...</li></ul>
    s = lazy_regex!(r"(?is)<ul[^>]*>(.*?)</ul>")
        .replace_all(&s, |caps: &regex::Captures| {
            let re_li = lazy_regex!(r"(?is)<li[^>]*>(.*?)</li>");
            let items: String = re_li
                .replace_all(&caps[1], |li_caps: &regex::Captures| {
                    format!("\n- {}", li_caps[1].trim())
                })
                .to_string();
            format!("\n{}\n", items.trim())
        })
        .to_string();

    // Paragraphs: <p>...</p> -> double newline separation
    s = lazy_regex!(r"(?is)<p[^>]*>(.*?)</p>")
        .replace_all(&s, |caps: &regex::Captures| {
            format!("\n\n{}\n\n", caps[1].trim())
        })
        .to_string();

    // <div> -> paragraph break
    s = lazy_regex!(r"(?is)<div[^>]*>(.*?)</div>")
        .replace_all(&s, |caps: &regex::Captures| {
            format!("\n\n{}\n\n", caps[1].trim())
        })
        .to_string();

    // Step 3: Inline conversions

    // Links: <a href="url">text</a> -> [text](url)
    s = lazy_regex!(r#"(?is)<a\s[^>]*href\s*=\s*"([^"]*)"[^>]*>(.*?)</a>"#)
        .replace_all(&s, |caps: &regex::Captures| {
            format!("[{}]({})", caps[2].trim(), &caps[1])
        })
        .to_string();

    // Bold: <strong>/<b>
    s = lazy_regex!(r"(?is)<(?:strong|b)[^>]*>(.*?)</(?:strong|b)>").replace_all(&s, "**$1**").to_string();

    // Italic: <em>/<i>
    s = lazy_regex!(r"(?is)<(?:em|i)[^>]*>(.*?)</(?:em|i)>").replace_all(&s, "*$1*").to_string();

    // Code: <code>
    s = lazy_regex!(r"(?is)<code>(.*?)</code>").replace_all(&s, "`$1`").to_string();

    // Step 4: Strip all remaining HTML tags
    s = lazy_regex!(r"<[^>]+>").replace_all(&s, "").to_string();

    // Step 5: Decode HTML entities (after tag stripping, so &lt;/&gt; aren't
    // mistaken for tags)
    s = decode_html_entities(&s);

    // Step 6: Post-process
    // Collapse 3+ newlines to 2
    s = lazy_regex!(r"\n{3,}").replace_all(&s, "\n\n").to_string();

    // Trim trailing whitespace per line
    s = s
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");

    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_tags_works() {
        assert_eq!(strip_html_tags("<p>Hello <b>world</b></p>"), "Hello world");
        assert_eq!(strip_html_tags("plain text"), "plain text");
        assert_eq!(strip_html_tags("<br/>line<br>break"), "linebreak");
        assert_eq!(strip_html_tags(""), "");
        assert_eq!(strip_html_tags("<p>  spaced   out  </p>"), "spaced out");
    }

    // -----------------------------------------------------------------------
    // html_to_markdown tests
    // -----------------------------------------------------------------------

    #[test]
    fn html_to_markdown_empty() {
        assert_eq!(html_to_markdown(""), "");
    }

    #[test]
    fn html_to_markdown_plain_text() {
        assert_eq!(html_to_markdown("no html here"), "no html here");
    }

    #[test]
    fn html_to_markdown_paragraphs() {
        assert_eq!(
            html_to_markdown("<p>First</p><p>Second</p>"),
            "First\n\nSecond"
        );
    }

    #[test]
    fn html_to_markdown_inline_formatting() {
        assert_eq!(
            html_to_markdown("<em>italic</em> and <strong>bold</strong>"),
            "*italic* and **bold**"
        );
        assert_eq!(
            html_to_markdown("<i>italic</i> and <b>bold</b>"),
            "*italic* and **bold**"
        );
    }

    #[test]
    fn html_to_markdown_links() {
        assert_eq!(
            html_to_markdown(r#"<a href="https://example.com">click</a>"#),
            "[click](https://example.com)"
        );
    }

    #[test]
    fn html_to_markdown_br() {
        assert_eq!(
            html_to_markdown("line1<br>line2<br/>line3"),
            "line1\nline2\nline3"
        );
    }

    #[test]
    fn html_to_markdown_unordered_list() {
        assert_eq!(
            html_to_markdown("<ul><li>one</li><li>two</li></ul>"),
            "- one\n- two"
        );
    }

    #[test]
    fn html_to_markdown_ordered_list() {
        assert_eq!(
            html_to_markdown("<ol><li>first</li><li>second</li></ol>"),
            "1. first\n2. second"
        );
    }

    #[test]
    fn html_to_markdown_blockquote() {
        assert_eq!(
            html_to_markdown("<blockquote>quoted text</blockquote>"),
            "> quoted text"
        );
    }

    #[test]
    fn html_to_markdown_headings() {
        assert_eq!(html_to_markdown("<h1>Title</h1>"), "# Title");
        assert_eq!(html_to_markdown("<h3>Sub</h3>"), "### Sub");
    }

    #[test]
    fn html_to_markdown_entities() {
        assert_eq!(html_to_markdown("&amp; &lt; &gt; &quot;"), "& < > \"");
        assert_eq!(html_to_markdown("&#65;&#x42;"), "AB");
    }

    #[test]
    fn html_to_markdown_strips_unknown_tags() {
        assert_eq!(html_to_markdown("<span class='x'>text</span>"), "text");
    }

    #[test]
    fn html_to_markdown_code() {
        assert_eq!(html_to_markdown("<code>foo()</code>"), "`foo()`");
    }

    #[test]
    fn html_to_markdown_nested() {
        assert_eq!(
            html_to_markdown("<p>A <em>bold <strong>and italic</strong></em> end</p>"),
            "A *bold **and italic*** end"
        );
    }

    #[test]
    fn html_to_markdown_zotero_note_example() {
        // Typical Zotero child note HTML
        let html = "<p>This is a <b>child note</b> with HTML.</p>";
        let md = html_to_markdown(html);
        assert_eq!(md, "This is a **child note** with HTML.");
    }

    #[test]
    fn html_to_markdown_two_paragraphs() {
        let html = "<p>Second note</p><p>with two paragraphs.</p>";
        let md = html_to_markdown(html);
        assert_eq!(md, "Second note\n\nwith two paragraphs.");
    }
}
