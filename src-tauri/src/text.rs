//! Turning outside text (HTML from feeds and e-mails) into plain, safe text.
//! HTML is never shown as HTML - only the text survives.

/// Plain text from HTML: tags removed, `<script>` / `<style>` dropped with their content,
/// block tags become line breaks, entities decoded, extra blank lines squeezed.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&decode_entities(&rest[..start]));
        rest = &rest[start..];
        let Some(end) = rest.find('>') else {
            // A lone "<" is text.
            out.push_str(&decode_entities(rest));
            rest = "";
            break;
        };
        let tag = &rest[1..end];
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        rest = &rest[end + 1..];
        if tag.starts_with("!--") {
            // Comment: skip to its end.
            if let Some(close) = rest.find("-->") {
                rest = &rest[close + 3..];
            }
            continue;
        }
        if (name == "script" || name == "style" || name == "head") && !tag.starts_with('/') {
            let closing = format!("</{name}");
            match find_ignore_case(rest, &closing) {
                Some(close) => {
                    rest = &rest[close..];
                    if let Some(gt) = rest.find('>') {
                        rest = &rest[gt + 1..];
                    }
                }
                None => rest = "",
            }
            continue;
        }
        match name.as_str() {
            "br" | "p" | "div" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "table"
            | "section" | "article" | "header" | "footer" | "blockquote" | "hr" => out.push('\n'),
            "li" if !tag.starts_with('/') => out.push_str("\n- "),
            "td" | "th" => out.push(' '),
            _ => {}
        }
    }
    out.push_str(&decode_entities(rest));
    squeeze(&out)
}

fn find_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(&needle.to_ascii_lowercase())
}

/// The common named entities and all numeric ones.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let semi = rest[..rest.len().min(12)].find(';');
        let decoded = semi.and_then(|semi| entity(&rest[1..semi]).map(|c| (c, semi)));
        match decoded {
            Some((c, semi)) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        // En and em dashes become the plain hyphen, like the named entities below.
        if code == 0x2013 || code == 0x2014 {
            return Some('-');
        }
        return char::from_u32(code).filter(|c| !c.is_control() || *c == '\n' || *c == '\t');
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" | "#39" => '\'',
        "nbsp" => ' ',
        "ndash" | "mdash" | "minus" => '-',
        "hellip" => '…',
        "bull" | "middot" => '·',
        "euro" => '€',
        "zl" => 'ł',
        "oacute" => 'ó',
        "Oacute" => 'Ó',
        "rsquo" | "lsquo" => '\'',
        "rdquo" | "ldquo" | "bdquo" => '"',
        _ => return None,
    })
}

/// Trims every line, removes control characters, keeps at most one blank line in a row.
pub fn squeeze(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank = 0;
    for line in s.lines() {
        let line: String = line
            .chars()
            .map(|c| if c == '\t' || c == '\u{a0}' { ' ' } else { c })
            .filter(|c| !c.is_control())
            .collect();
        let line = collapse_spaces(line.trim());
        if line.is_empty() {
            blank += 1;
            if blank > 1 || out.is_empty() {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(&line);
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c == ' ' {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out
}

/// One line of plain text (for titles, company names): no line breaks, no control characters.
pub fn one_line(s: &str, max: usize) -> String {
    let line = collapse_spaces(
        &s.chars()
            .map(|c| if c.is_whitespace() { ' ' } else { c })
            .filter(|c| !c.is_control())
            .collect::<String>(),
    );
    cut(line.trim(), max)
}

/// At most `max` characters (not bytes), cut on a character boundary.
pub fn cut(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => s[..i].trim_end().to_string(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_plain_text() {
        let html = "<html><head><title>x</title></head><body><h2>Role</h2><p>We&nbsp;need a <b>React</b> dev &amp; tester.</p>\
            <ul><li>TypeScript</li><li>CSS</li></ul><script>alert('x')</script><style>p{}</style><!-- note --><p>Bye &#8211; &#x142;</p></body></html>";
        assert_eq!(html_to_text(html), "Role\n\nWe need a React dev & tester.\n\n- TypeScript\n- CSS\n\nBye - ł");
    }

    #[test]
    fn broken_html_does_not_break() {
        assert_eq!(html_to_text("a < b and <unclosed"), "a < b and <unclosed");
        assert_eq!(html_to_text("&unknown; &amp"), "&unknown; &amp");
        assert_eq!(html_to_text("<script>never closed"), "");
    }

    #[test]
    fn one_line_and_cut() {
        assert_eq!(one_line(" Frontend\n  Developer\t(k/m) ", 100), "Frontend Developer (k/m)");
        assert_eq!(cut("zażółć", 3), "zaż");
        assert_eq!(cut("abc", 10), "abc");
    }
}
