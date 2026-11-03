struct Parser<'a> {
    input: &'a [u8],
    position: usize,
    depth: u8,
}

impl Parser<'_> {
    fn skip_space(&mut self) {
        while self.input.get(self.position).is_some_and(u8::is_ascii_whitespace) {
            self.position += 1;
        }
    }

    fn take(&mut self, byte: u8) -> bool {
        self.skip_space();
        if self.input.get(self.position) == Some(&byte) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expression(&mut self) -> Option<f64> {
        let mut value = self.term()?;
        loop {
            if self.take(b'+') {
                value += self.term()?;
            } else if self.take(b'-') {
                value -= self.term()?;
            } else {
                return Some(value);
            }
        }
    }

    fn term(&mut self) -> Option<f64> {
        let mut value = self.factor()?;
        loop {
            if self.take(b'*') {
                value *= self.factor()?;
            } else if self.take(b'/') {
                value /= self.factor()?;
            } else {
                return Some(value);
            }
        }
    }

    fn factor(&mut self) -> Option<f64> {
        self.skip_space();
        if self.depth >= 32 {
            return None;
        }
        self.depth += 1;
        let value = if self.take(b'-') {
            self.factor().map(|value| -value)
        } else if self.take(b'+') {
            self.factor()
        } else if self.take(b'(') {
            self.expression().filter(|_| self.take(b')'))
        } else {
            self.number()
        };
        self.depth -= 1;
        value
    }

    fn number(&mut self) -> Option<f64> {
        self.skip_space();
        let start = self.position;
        while self.input.get(self.position).is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.') {
            self.position += 1;
        }
        std::str::from_utf8(&self.input[start..self.position]).ok()?.parse().ok()
    }
}

pub fn calculate(input: &str) -> Option<f64> {
    if input.len() > 256 {
        return None;
    }
    let mut parser = Parser { input: input.as_bytes(), position: 0, depth: 0 };
    let value = parser.expression()?;
    parser.skip_space();
    (parser.position == parser.input.len() && value.is_finite()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::calculate;

    #[test]
    fn arithmetic_respects_precedence_parentheses_and_unary_signs() {
        assert_eq!(calculate("48 * 12"), Some(576.0));
        assert_eq!(calculate("-(2 + 3) * 4 / 2"), Some(-10.0));
        assert_eq!(calculate("1.5 + .5"), Some(2.0));
    }

    #[test]
    fn malformed_or_unbounded_input_is_not_executed() {
        for input in ["", "1 / 0", "1 +", "1 + alert(1)", "1.2.3", "((((1)"] {
            assert_eq!(calculate(input), None, "{input}");
        }
        assert_eq!(calculate(&"(".repeat(300)), None);
    }
}
