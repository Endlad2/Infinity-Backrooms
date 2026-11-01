use std::{collections::BTreeMap, error::Error, fmt};

#[derive(Debug, Clone)]
pub struct XmlNode {
    pub name: String,
    pub attributes: BTreeMap<String, String>,
    pub children: Vec<XmlNode>,
    pub text: String,
}

impl XmlNode {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }

    pub fn child(&self, name: &str) -> Option<&XmlNode> {
        self.children.iter().find(|node| node.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a XmlNode> + 'a {
        self.children.iter().filter(move |node| node.name == name)
    }
}

#[derive(Debug, Clone)]
pub struct XmlError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}:{}", self.message, self.line, self.column)
    }
}

impl Error for XmlError {}

pub fn parse(input: &str) -> Result<XmlNode, XmlError> {
    let mut parser = Parser { input, pos: 0 };
    parser.skip_misc()?;
    let root = parser.parse_element()?;
    parser.skip_misc()?;
    if !parser.eof() {
        return Err(parser.error("unexpected data after root element"));
    }
    Ok(root)
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn rest(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn starts_with(&self, value: &str) -> bool {
        self.rest().starts_with(value)
    }

    fn bump(&mut self, bytes: usize) {
        self.pos += bytes;
    }

    fn error(&self, message: impl Into<String>) -> XmlError {
        let prefix = &self.input[..self.pos.min(self.input.len())];
        let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = prefix.rsplit_once('\n').map(|(_, tail)| tail.chars().count() + 1).unwrap_or_else(|| prefix.chars().count() + 1);
        XmlError { message: message.into(), line, column }
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.rest().chars().next() {
            if !ch.is_whitespace() { break; }
            self.bump(ch.len_utf8());
        }
    }

    fn skip_misc(&mut self) -> Result<(), XmlError> {
        loop {
            self.skip_ws();
            if self.starts_with("<?") {
                self.skip_until("?>", "unterminated XML processing instruction")?;
            } else if self.starts_with("<!--") {
                self.skip_until("-->", "unterminated XML comment")?;
            } else if self.starts_with("<!DOCTYPE") || self.starts_with("<!doctype") {
                self.skip_doctype()?;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn skip_until(&mut self, marker: &str, message: &str) -> Result<(), XmlError> {
        if let Some(relative) = self.rest().find(marker) {
            self.bump(relative + marker.len());
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn skip_doctype(&mut self) -> Result<(), XmlError> {
        let mut depth = 0usize;
        let mut quote: Option<char> = None;
        while let Some(ch) = self.rest().chars().next() {
            self.bump(ch.len_utf8());
            if let Some(q) = quote {
                if ch == q { quote = None; }
                continue;
            }
            match ch {
                '\'' | '"' => quote = Some(ch),
                '[' => depth += 1,
                ']' => depth = depth.saturating_sub(1),
                '>' if depth == 0 => return Ok(()),
                _ => {}
            }
        }
        Err(self.error("unterminated DOCTYPE"))
    }

    fn parse_element(&mut self) -> Result<XmlNode, XmlError> {
        if !self.starts_with("<") {
            return Err(self.error("expected '<'"));
        }
        self.bump(1);
        if self.starts_with("/") || self.starts_with("!") || self.starts_with("?") {
            return Err(self.error("expected opening element"));
        }

        let name = self.parse_name()?;
        let mut attributes = BTreeMap::new();
        loop {
            self.skip_ws();
            if self.starts_with("/>") {
                self.bump(2);
                return Ok(XmlNode { name, attributes, children: Vec::new(), text: String::new() });
            }
            if self.starts_with(">") {
                self.bump(1);
                break;
            }
            let key = self.parse_name()?;
            self.skip_ws();
            if !self.starts_with("=") {
                return Err(self.error(format!("expected '=' after attribute {key}")));
            }
            self.bump(1);
            self.skip_ws();
            let value = self.parse_quoted_value()?;
            if attributes.insert(key.clone(), value).is_some() {
                return Err(self.error(format!("duplicate attribute {key}")));
            }
        }

        let mut text = String::new();
        let mut children = Vec::new();
        loop {
            if self.eof() {
                return Err(self.error(format!("unterminated element <{name}>")));
            }
            if self.starts_with("</") {
                self.bump(2);
                let close = self.parse_name()?;
                if close != name {
                    return Err(self.error(format!("mismatched closing element </{close}>, expected </{name}>")));
                }
                self.skip_ws();
                if !self.starts_with(">") {
                    return Err(self.error("expected '>' after closing element"));
                }
                self.bump(1);
                break;
            }
            if self.starts_with("<!--") {
                self.skip_until("-->", "unterminated XML comment")?;
                continue;
            }
            if self.starts_with("<![CDATA[") {
                self.bump("<![CDATA[".len());
                let Some(end) = self.rest().find("]]>") else {
                    return Err(self.error("unterminated CDATA section"));
                };
                text.push_str(&self.rest()[..end]);
                self.bump(end + 3);
                continue;
            }
            if self.starts_with("<?") {
                self.skip_until("?>", "unterminated XML processing instruction")?;
                continue;
            }
            if self.starts_with("<") {
                children.push(self.parse_element()?);
                continue;
            }

            let end = self.rest().find('<').unwrap_or(self.rest().len());
            let raw = &self.rest()[..end];
            text.push_str(&decode_entities(raw).map_err(|message| self.error(message))?);
            self.bump(end);
        }

        Ok(XmlNode { name, attributes, children, text: text.trim().to_string() })
    }

    fn parse_name(&mut self) -> Result<String, XmlError> {
        let start = self.pos;
        while let Some(ch) = self.rest().chars().next() {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.') {
                self.bump(ch.len_utf8());
            } else {
                break;
            }
        }
        if self.pos == start {
            Err(self.error("expected XML name"))
        } else {
            Ok(self.input[start..self.pos].to_string())
        }
    }

    fn parse_quoted_value(&mut self) -> Result<String, XmlError> {
        let Some(quote) = self.rest().chars().next() else {
            return Err(self.error("expected quoted attribute value"));
        };
        if quote != '\'' && quote != '"' {
            return Err(self.error("attribute value must be quoted"));
        }
        self.bump(quote.len_utf8());
        let start = self.pos;
        while let Some(ch) = self.rest().chars().next() {
            if ch == quote {
                let raw = &self.input[start..self.pos];
                self.bump(ch.len_utf8());
                return decode_entities(raw).map_err(|message| self.error(message));
            }
            self.bump(ch.len_utf8());
        }
        Err(self.error("unterminated attribute value"))
    }
}

fn decode_entities(input: &str) -> Result<String, String> {
    if !input.contains('&') {
        return Ok(input.to_string());
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        rest = &rest[index + 1..];
        let Some(end) = rest.find(';') else {
            return Err("unterminated XML entity".into());
        };
        let entity = &rest[..end];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                let code = u32::from_str_radix(&entity[2..], 16).map_err(|_| format!("invalid hex entity &{entity};"))?;
                let ch = char::from_u32(code).ok_or_else(|| format!("invalid Unicode entity &{entity};"))?;
                out.push(ch);
            }
            _ if entity.starts_with('#') => {
                let code = entity[1..].parse::<u32>().map_err(|_| format!("invalid decimal entity &{entity};"))?;
                let ch = char::from_u32(code).ok_or_else(|| format!("invalid Unicode entity &{entity};"))?;
                out.push(ch);
            }
            _ => return Err(format!("unknown XML entity &{entity};")),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_level_xml_without_external_crate() {
        let root = parse(r#"<?xml version='1.0'?><level id='0'><meta name='Lobby'/><script><![CDATA[bi.log('x < y')]]></script></level>"#).unwrap();
        assert_eq!(root.name, "level");
        assert_eq!(root.attr("id"), Some("0"));
        assert_eq!(root.child("meta").unwrap().attr("name"), Some("Lobby"));
        assert!(root.child("script").unwrap().text.contains("x < y"));
    }

    #[test]
    fn decodes_entities() {
        let root = parse("<root value='a&amp;b'>&lt;ok&gt; &#x41;</root>").unwrap();
        assert_eq!(root.attr("value"), Some("a&b"));
        assert_eq!(root.text, "<ok> A");
    }
}
