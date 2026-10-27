//! Just enough XML for SOAP answers: elements, attributes and text, without DTDs or entities
//! beyond the five predefined ones. Names lose their namespace prefix.

use std::borrow::Cow;

/// An element with its attributes, its child elements and the text directly inside it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
    pub text: String,
}

/// Deeper than any SOAP answer goes; deeper input is refused rather than walked.
const MAX_DEPTH: usize = 64;

impl Element {
    /// The first child called `name`.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Every child called `name`.
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// The element down the path of child names.
    pub fn at(&self, path: &[&str]) -> Option<&Element> {
        path.iter().try_fold(self, |e, name| e.child(name))
    }

    /// The text of the element down the path; empty if there is none.
    pub fn text_at(&self, path: &[&str]) -> &str {
        self.at(path).map_or("", |e| e.text.as_str())
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The root element of a document. None if it is not well-formed XML as far as this reads it,
/// or has a DTD.
pub fn parse(data: &[u8]) -> Option<Element> {
    let text = std::str::from_utf8(data).ok()?;
    let mut rest = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut stack: Vec<Element> = Vec::new();
    loop {
        // The root returns as it closes: running out first means it never did.
        let open = rest.find('<')?;
        if let Some(top) = stack.last_mut() {
            top.text.push_str(&unescape(&rest[..open])?);
        } else if !rest[..open].trim().is_empty() {
            return None;
        }
        rest = &rest[open..];
        // An element that ended here.
        let mut closed = None;
        if let Some(after) = rest.strip_prefix("<?") {
            rest = &after[after.find("?>")? + 2..];
        } else if let Some(after) = rest.strip_prefix("<!--") {
            rest = &after[after.find("-->")? + 3..];
        } else if let Some(after) = rest.strip_prefix("<![CDATA[") {
            let end = after.find("]]>")?;
            stack.last_mut()?.text.push_str(&after[..end]);
            rest = &after[end + 3..];
        } else if rest.starts_with("<!") {
            return None;
        } else if let Some(after) = rest.strip_prefix("</") {
            let end = after.find('>')?;
            let done = stack.pop()?;
            if local(after[..end].trim()) != done.name {
                return None;
            }
            rest = &after[end + 1..];
            closed = Some(done);
        } else {
            let (element, empty, after) = tag(&rest[1..])?;
            rest = after;
            if empty {
                closed = Some(element);
            } else if stack.len() >= MAX_DEPTH {
                return None;
            } else {
                stack.push(element);
            }
        }
        if let Some(done) = closed {
            match stack.last_mut() {
                Some(parent) => parent.children.push(done),
                None => return rest.trim().is_empty().then_some(done),
            }
        }
    }
}

/// An opening tag after its `<`: the element, whether it closed itself, and what follows.
fn tag(text: &str) -> Option<(Element, bool, &str)> {
    let end = text.find(['>', ' ', '\t', '\r', '\n', '/'])?;
    let name = &text[..end];
    if name.is_empty() {
        return None;
    }
    let mut element = Element {
        name: local(name).to_owned(),
        ..Element::default()
    };
    let mut rest = &text[end..];
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("/>") {
            return Some((element, true, after));
        }
        if let Some(after) = rest.strip_prefix('>') {
            return Some((element, false, after));
        }
        let eq = rest.find('=')?;
        let name = rest[..eq].trim();
        let value = rest[eq + 1..].trim_start();
        let quote = value.chars().next().filter(|q| *q == '"' || *q == '\'')?;
        let value = &value[1..];
        let close = value.find(quote)?;
        if name.is_empty() || name.contains(['<', '>']) {
            return None;
        }
        if name != "xmlns" && !name.starts_with("xmlns:") {
            let value = unescape(&value[..close])?;
            element
                .attrs
                .push((local(name).to_owned(), value.into_owned()));
        }
        rest = &value[close + 1..];
    }
}

/// A name without its namespace prefix.
fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, n)| n)
}

/// Text with its character and entity references replaced. None on an unknown entity.
fn unescape(text: &str) -> Option<Cow<'_, str>> {
    if !text.contains('&') {
        return Some(Cow::Borrowed(text));
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let end = after.find(';')?;
        let entity = &after[..end];
        let c = match entity {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = match entity.strip_prefix("#x").or(entity.strip_prefix("#X")) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                    None => entity.strip_prefix('#')?.parse().ok()?,
                };
                char::from_u32(code)?
            }
        };
        out.push(c);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(Cow::Owned(out))
}

/// Text made safe inside an element or a quoted attribute.
pub use ozero_domain::markup_escape as escape;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_soap_answer() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><!-- note -->
  <s:Body><m:R ResponseClass='Success' t:x="a &amp; b"><t:Subject>Tom &lt;3 &#x41;&#66;</t:Subject>
  <t:Empty/><t:C><![CDATA[<raw>]]></t:C></m:R></s:Body></s:Envelope>"#;
        let root = parse(xml).unwrap();
        assert_eq!(root.name, "Envelope");
        let r = root.at(&["Body", "R"]).unwrap();
        assert_eq!(r.attr("ResponseClass"), Some("Success"));
        assert_eq!(r.attr("x"), Some("a & b"));
        assert_eq!(r.text_at(&["Subject"]), "Tom <3 AB");
        assert_eq!(r.text_at(&["Empty"]), "");
        assert_eq!(r.text_at(&["C"]), "<raw>");
        assert_eq!(r.all("Empty").count(), 1);
        assert!(r.at(&["Nothing"]).is_none());
    }

    #[test]
    fn refuses_what_is_not_well_formed() {
        assert_eq!(parse(b"<a><b></a>"), None);
        assert_eq!(parse(b"<a>"), None);
        assert_eq!(parse(b"<a/><b/>"), None);
        assert_eq!(parse(b"text<a/>"), None);
        assert_eq!(parse(b"<a>&bogus;</a>"), None);
        assert_eq!(parse(b"<!DOCTYPE a [<!ENTITY x 'y'>]><a>&x;</a>"), None);
        assert_eq!(parse(b""), None);
        let deep = "<a>".repeat(100) + &"</a>".repeat(100);
        assert_eq!(parse(deep.as_bytes()), None);
        assert!(parse(b"\xef\xbb\xbf<a x=\"1\" />\n").is_some());
    }

    #[test]
    fn escapes_text() {
        assert_eq!(escape("a<b>&\"'\u{1}\n"), "a&lt;b&gt;&amp;&quot;&#39;\n");
        let text = "Tom & \"Jerry\" <x>";
        let xml = format!("<a v=\"{0}\">{0}</a>", escape(text));
        let root = parse(xml.as_bytes()).unwrap();
        assert_eq!(root.text, text);
        assert_eq!(root.attr("v"), Some(text));
    }
}
