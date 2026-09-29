//! Converts the small HTML subset used by translations and tafsirs into Pango markup.

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn decode_entity(name: &str) -> Option<String> {
    Some(match name {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" | "#39" => "'".into(),
        "nbsp" => "\u{a0}".into(),
        "hellip" => "…".into(),
        "mdash" => "—".into(),
        "ndash" => "–".into(),
        "lsquo" => "‘".into(),
        "rsquo" => "’".into(),
        "ldquo" => "“".into(),
        "rdquo" => "”".into(),
        n if n.starts_with("#x") || n.starts_with("#X") => {
            char::from_u32(u32::from_str_radix(&n[2..], 16).ok()?)?.to_string()
        }
        n if n.starts_with('#') => char::from_u32(n[1..].parse().ok()?)?.to_string(),
        _ => return None,
    })
}

#[derive(Default, Clone, Copy)]
pub struct HtmlOptions {
    /// Keep footnote markers (`<sup foot_note=..>`) as superscripts instead of dropping them.
    pub keep_footnotes: bool,
    /// Treat block tags as paragraph breaks (tafsir); otherwise flatten to one line.
    pub blocks: bool,
}

/// Convert HTML to Pango markup. Unknown tags are dropped, keeping their text.
pub fn html_to_pango(html: &str, opts: HtmlOptions) -> String {
    let mut out = String::with_capacity(html.len());
    let mut stack: Vec<&'static str> = Vec::new();
    let mut skip_depth = 0usize;
    let mut rest = html;
    let mut pending_space = false;

    let push_text = |out: &mut String, text: &str, pending_space: &mut bool| {
        for c in text.chars() {
            if c.is_whitespace() && c != '\u{a0}' {
                *pending_space = true;
            } else {
                if *pending_space && !out.is_empty() && !out.ends_with('\n') {
                    out.push(' ');
                }
                *pending_space = false;
                match c {
                    '&' => out.push_str("&amp;"),
                    '<' => out.push_str("&lt;"),
                    '>' => out.push_str("&gt;"),
                    _ => out.push(c),
                }
            }
        }
    };

    let paragraph = |out: &mut String| {
        let trimmed = out.trim_end_matches([' ', '\n']).len();
        out.truncate(trimmed);
        if !out.is_empty() {
            out.push_str("\n\n");
        }
    };

    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix('<') {
            let Some(end) = stripped.find('>') else {
                if skip_depth == 0 {
                    push_text(&mut out, rest, &mut pending_space);
                }
                break;
            };
            let tag = &stripped[..end];
            rest = &stripped[end + 1..];
            let closing = tag.starts_with('/');
            let body = tag.trim_start_matches('/').trim_end_matches('/');
            let name_end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
            let name = body[..name_end].to_ascii_lowercase();
            let is_footnote = name == "sup" && body.contains("foot_note");

            if skip_depth > 0 {
                if name == "sup" {
                    if closing {
                        skip_depth -= 1;
                    } else {
                        skip_depth += 1;
                    }
                }
                continue;
            }
            if is_footnote && !opts.keep_footnotes && !closing {
                skip_depth = 1;
                continue;
            }

            let open: Option<(&'static str, &'static str)> = match name.as_str() {
                "b" | "strong" => Some(("b", "<b>")),
                "i" | "em" => Some(("i", "<i>")),
                "sup" => Some(("sup", "<sup>")),
                "sub" => Some(("sub", "<sub>")),
                "u" => Some(("u", "<u>")),
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    Some(("span", "<span weight=\"bold\" size=\"large\">"))
                }
                _ => None,
            };
            let is_block = matches!(
                name.as_str(),
                "p" | "div"
                    | "br"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "li"
                    | "ul"
                    | "ol"
                    | "blockquote"
            );
            let is_br = name == "br";
            let block_break = |out: &mut String, pending_space: &mut bool| {
                if opts.blocks {
                    if is_br {
                        out.push('\n');
                    } else {
                        paragraph(out);
                    }
                    *pending_space = false;
                } else {
                    *pending_space = true;
                }
            };
            if is_block && !closing {
                block_break(&mut out, &mut pending_space);
            }
            if let Some((close_name, open_tag)) = open {
                if closing {
                    if let Some(pos) = stack.iter().rposition(|t| *t == close_name) {
                        // close everything above it to keep markup well-formed
                        while stack.len() > pos {
                            let t = stack.pop().unwrap();
                            out.push_str("</");
                            out.push_str(t);
                            out.push('>');
                        }
                    }
                } else if !body.ends_with('/') {
                    if pending_space && !out.is_empty() && !out.ends_with('\n') {
                        out.push(' ');
                        pending_space = false;
                    }
                    out.push_str(open_tag);
                    stack.push(close_name);
                }
            }
            // Block breaks go outside inline spans: before an opening tag, after a closing one.
            if is_block && closing {
                block_break(&mut out, &mut pending_space);
            }
        } else if let Some(stripped) = rest.strip_prefix('&') {
            let end = stripped.find(';').filter(|&e| e <= 10);
            match end.and_then(|e| decode_entity(&stripped[..e]).map(|d| (e, d))) {
                Some((e, decoded)) => {
                    if skip_depth == 0 {
                        push_text(&mut out, &decoded, &mut pending_space);
                    }
                    rest = &stripped[e + 1..];
                }
                None => {
                    if skip_depth == 0 {
                        push_text(&mut out, "&", &mut pending_space);
                    }
                    rest = stripped;
                }
            }
        } else {
            let next = rest.find(['<', '&']).unwrap_or(rest.len());
            if skip_depth == 0 {
                push_text(&mut out, &rest[..next], &mut pending_space);
            }
            rest = &rest[next..];
        }
    }
    while let Some(t) = stack.pop() {
        out.push_str("</");
        out.push_str(t);
        out.push('>');
    }
    out.trim_end().to_string()
}

/// Strip all tags, leaving plain text (for copy to clipboard, search snippets).
pub fn html_to_plain(html: &str) -> String {
    let markup = html_to_pango(html, HtmlOptions::default());
    let mut out = String::new();
    let mut in_tag = false;
    for c in markup.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Convert Western digits to Arabic-Indic digits (١٢٣).
pub fn arabic_digits(n: u32) -> String {
    n.to_string()
        .chars()
        .map(|c| char::from_u32(0x0660 + c.to_digit(10).unwrap_or(0)).unwrap_or(c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footnotes_are_dropped() {
        let s = html_to_pango(
            "In the Name of Allah<sup foot_note=\"77\">1</sup>—the Most Compassionate",
            HtmlOptions::default(),
        );
        assert_eq!(s, "In the Name of Allah—the Most Compassionate");
    }

    #[test]
    fn tags_and_entities() {
        let s = html_to_pango(
            "<h1><span style=\"x\">Intro</span></h1><p>A &amp; <em>b</em> &lt;c&gt;</p>",
            HtmlOptions {
                blocks: true,
                ..Default::default()
            },
        );
        assert_eq!(
            s,
            "<span weight=\"bold\" size=\"large\">Intro</span>\n\nA &amp; <i>b</i> &lt;c&gt;"
        );
    }

    #[test]
    fn unbalanced() {
        let s = html_to_pango("<b>x <i>y</b> z", HtmlOptions::default());
        assert_eq!(s, "<b>x <i>y</i></b> z");
    }

    #[test]
    fn digits() {
        assert_eq!(arabic_digits(255), "٢٥٥");
    }
}
