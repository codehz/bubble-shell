use bubble_core::{NodeDesc, Template};

pub fn compile(source: &str) -> Result<Template, String> {
    let mut parser = Parser {
        source: source.as_bytes(),
        index: 0,
        next_id: 1,
    };
    parser.skip();
    let nodes = parser.element(0)?;
    parser.skip();
    if parser.index != parser.source.len() {
        return Err("trailing input after the root element".into());
    }
    if nodes.is_empty() || nodes[0].parent != 0 {
        return Err("missing root".into());
    }
    Ok(Template { nodes })
}

struct Parser<'a> {
    source: &'a [u8],
    index: usize,
    next_id: u32,
}

impl<'a> Parser<'a> {
    fn element(&mut self, parent: u32) -> Result<Vec<NodeDesc>, String> {
        self.expect("<")?;
        let name = self.ident()?;
        if name == "Repeat" {
            return self.repeat(parent);
        }
        let mut node = match name.as_str() {
            "View" => NodeDesc::view(self.alloc(), parent),
            "Text" => NodeDesc::text(self.alloc(), parent, ""),
            other => return Err(format!("unknown element {other}")),
        };
        self.attrs(&mut node)?;
        self.skip();
        if self.eat("/>") {
            return Ok(vec![node]);
        }
        self.expect(">")?;
        let mut nodes = vec![node];
        let id = nodes[0].id;
        if nodes[0].kind == bubble_core::Kind::Text {
            let text = self.text_until_tag()?;
            nodes[0].text = text;
        } else {
            loop {
                self.skip();
                if self.starts_with("</") {
                    break;
                }
                nodes.extend(self.element(id)?);
            }
        }
        self.expect("</")?;
        let close = self.ident()?;
        if close != name {
            return Err(format!("expected </{name}>, found </{close}>"));
        }
        self.expect(">")?;
        Ok(nodes)
    }

    fn repeat(&mut self, parent: u32) -> Result<Vec<NodeDesc>, String> {
        let mut count = None;
        loop {
            self.skip();
            if self.starts_with(">") || self.starts_with("/>") {
                break;
            }
            let key = self.ident()?;
            self.skip();
            self.expect("=")?;
            self.skip();
            if key == "count" {
                self.expect("{")?;
                count = Some(self.integer()?);
                self.expect("}")?;
            } else {
                return Err(format!("unknown Repeat attribute {key}"));
            }
        }
        let count = count.ok_or("Repeat requires count")?;
        self.expect(">")?;
        let start = self.index;
        self.skip_element()?;
        let body = std::str::from_utf8(&self.source[start..self.index]).unwrap();
        self.expect("</")?;
        let close = self.ident()?;
        if close != "Repeat" {
            return Err(format!("expected </Repeat>, found </{close}>"));
        }
        self.expect(">")?;
        let mut nodes = Vec::new();
        for index in 0..count {
            let expanded = body.replace("{index}", &format!("{index:02}"));
            let mut child = Parser {
                source: expanded.as_bytes(),
                index: 0,
                next_id: self.next_id,
            };
            child.skip();
            let parsed = child.element(parent)?;
            child.skip();
            if child.index != child.source.len() {
                return Err("Repeat body must be one element".into());
            }
            self.next_id = child.next_id;
            nodes.extend(parsed);
        }
        Ok(nodes)
    }

    fn attrs(&mut self, node: &mut NodeDesc) -> Result<(), String> {
        loop {
            self.skip();
            if self.starts_with(">") || self.starts_with("/>") {
                return Ok(());
            }
            let key = self.ident()?;
            self.skip();
            if self.eat("=") {
                self.skip();
                self.value(node, &key)?;
            } else {
                self.flag(node, &key)?;
            }
        }
    }

    fn value(&mut self, node: &mut NodeDesc, key: &str) -> Result<(), String> {
        self.expect("{")?;
        self.skip();
        match key {
            "background" => node.background = self.integer()?,
            "color" => node.color = self.integer()?,
            "fontSize" => node.font_size = self.number()?,
            "radius" => node.radius = self.number()?,
            "handler" => node.handler = self.integer()?,
            "flexGrow" => node.flex_grow = self.number()?,
            "flexShrink" => node.flex_shrink = self.number()?,
            "width" => node.width = Some(self.number()?),
            "height" => node.height = Some(self.number()?),
            "padding" => node.padding = self.padding()?,
            other => return Err(format!("unknown attribute {other}")),
        }
        self.skip();
        self.expect("}")?;
        Ok(())
    }

    fn flag(&mut self, node: &mut NodeDesc, key: &str) -> Result<(), String> {
        match key {
            "scrollY" => node.scroll_y = true,
            "row" => node.row = true,
            other => return Err(format!("unknown attribute {other}")),
        }
        Ok(())
    }

    fn padding(&mut self) -> Result<[f32; 4], String> {
        self.skip();
        if self.eat("[") {
            let mut values = Vec::new();
            loop {
                self.skip();
                values.push(self.number()?);
                self.skip();
                if self.eat("]") {
                    break;
                }
                self.expect(",")?;
            }
            return match values.len() {
                1 => Ok([values[0]; 4]),
                4 => Ok([values[0], values[1], values[2], values[3]]),
                _ => Err("padding needs 1 or 4 numbers".into()),
            };
        }
        let value = self.number()?;
        Ok([value; 4])
    }

    fn integer(&mut self) -> Result<u32, String> {
        self.skip();
        if self.eat("0x") || self.eat("0X") {
            return self.hex();
        }
        let start = self.index;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.index += 1;
        }
        if start == self.index {
            return Err(format!("expected an integer at {}", self.here(start)));
        }
        let text = std::str::from_utf8(&self.source[start..self.index]).unwrap();
        text.parse().map_err(|_| format!("invalid integer {text}"))
    }

    fn hex(&mut self) -> Result<u32, String> {
        let mut value = 0u32;
        let digits = self.index;
        while let Some(byte) = self.source.get(self.index).copied() {
            let digit = match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                b'_' => {
                    self.index += 1;
                    continue;
                }
                _ => break,
            };
            self.index += 1;
            value = value
                .checked_shl(4)
                .and_then(|shifted| shifted.checked_add(u32::from(digit)))
                .ok_or("hex literal overflows u32")?;
        }
        if self.index == digits {
            return Err("expected hex digits".into());
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<f32, String> {
        self.skip();
        let start = self.index;
        if self.peek() == Some(b'-') {
            self.index += 1;
        }
        let mut saw_digit = false;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            saw_digit = true;
            self.index += 1;
        }
        if self.peek() == Some(b'.') {
            self.index += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                saw_digit = true;
                self.index += 1;
            }
        }
        if !saw_digit {
            return Err(format!("expected a number at {}", self.here(start)));
        }
        let text = std::str::from_utf8(&self.source[start..self.index]).unwrap();
        text.parse().map_err(|_| format!("invalid number {text}"))
    }

    fn text_until_tag(&mut self) -> Result<String, String> {
        let start = self.index;
        while self.index < self.source.len() && self.source[self.index] != b'<' {
            self.index += 1;
        }
        let text = std::str::from_utf8(&self.source[start..self.index]).map_err(|_| "text is not utf-8")?;
        Ok(text.trim().to_owned())
    }

    fn skip_element(&mut self) -> Result<(), String> {
        self.skip();
        self.expect("<")?;
        if self.starts_with("/") {
            return Err("expected an element".into());
        }
        let name = self.ident()?;
        loop {
            self.skip();
            if self.eat("/>") {
                return Ok(());
            }
            if self.eat(">") {
                break;
            }
            self.index += 1;
        }
        loop {
            self.skip();
            if self.starts_with("</") {
                self.expect("</")?;
                let close = self.ident()?;
                if close != name {
                    return Err(format!("expected </{name}>, found </{close}>"));
                }
                self.expect(">")?;
                return Ok(());
            }
            if self.starts_with("<") {
                self.skip_element()?;
                continue;
            }
            if self.index >= self.source.len() {
                return Err("unclosed element".into());
            }
            self.index += 1;
        }
    }

    fn ident(&mut self) -> Result<String, String> {
        self.skip();
        let start = self.index;
        while matches!(self.peek(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9')) {
            self.index += 1;
        }
        if start == self.index {
            return Err(format!("expected a name at {}", self.here(start)));
        }
        Ok(std::str::from_utf8(&self.source[start..self.index]).unwrap().to_owned())
    }

    fn expect(&mut self, token: &str) -> Result<(), String> {
        self.skip();
        if self.eat(token) {
            Ok(())
        } else {
            Err(format!("expected {token} at {}", self.here(self.index)))
        }
    }

    fn eat(&mut self, token: &str) -> bool {
        if self.starts_with(token) {
            self.index += token.len();
            true
        } else {
            false
        }
    }

    fn starts_with(&self, token: &str) -> bool {
        self.source[self.index..].starts_with(token.as_bytes())
    }

    fn skip(&mut self) {
        loop {
            while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
                self.index += 1;
            }
            if self.eat("<!--") {
                while !self.eat("-->") {
                    if self.index >= self.source.len() {
                        return;
                    }
                    self.index += 1;
                }
                continue;
            }
            break;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.source.get(self.index).copied()
    }

    fn alloc(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn here(&self, at: usize) -> String {
        let line = self.source[..at].iter().filter(|byte| **byte == b'\n').count() + 1;
        format!("line {line}")
    }
}

#[cfg(test)]
mod tests {
    use super::compile;

    #[test]
    fn compiles_the_demo_tree() {
        let template = compile(include_str!("../app.tsx")).unwrap();
        assert_eq!(template.nodes.len(), 46);
        assert_eq!(template.nodes[1].id, 2);
        assert_eq!(template.nodes[1].text, "Count: 0");
        assert_eq!(template.nodes[1].font_size, 28.0);
        assert!(template.nodes[4].scroll_y);
        assert_eq!(template.nodes[4].flex_grow, 1.0);
        assert_eq!(template.nodes[5].id, 6);
        assert_eq!(template.nodes[6].parent, 6);
        assert_eq!(template.nodes[6].text, "Row 00");
        assert_eq!(template.nodes[45].text, "Row 39");
        assert_eq!(template.nodes[2].handler, 1);
    }
}
