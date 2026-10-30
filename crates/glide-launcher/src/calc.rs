#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Number(f64),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Open,
    Close,
}

fn tokenize(input: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch.is_ascii_digit() || ch == '.' || ch == ',' {
            let mut literal = String::new();
            let mut separators = 0;
            while let Some(&digit) = chars.peek() {
                if digit.is_ascii_digit() {
                    literal.push(digit);
                } else if digit == '.' || digit == ',' {
                    separators += 1;
                    literal.push('.');
                } else {
                    break;
                }
                chars.next();
            }
            if separators > 1 || literal == "." {
                return None;
            }
            tokens.push(Token::Number(literal.parse().ok()?));
            continue;
        }
        let token = match ch {
            '+' => Token::Plus,
            '-' | '\u{2212}' => Token::Minus,
            '*' | '\u{d7}' => Token::Star,
            '/' | '\u{f7}' => Token::Slash,
            '%' => Token::Percent,
            '(' => Token::Open,
            ')' => Token::Close,
            _ => return None,
        };
        tokens.push(token);
        chars.next();
    }
    Some(tokens)
}

const MAX_TOKENS: usize = 256;
const MAX_NESTING: usize = 32;

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    operations: usize,
    nesting: usize,
}

impl Parser {
    fn nested<T>(&mut self, inner: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        if self.nesting >= MAX_NESTING {
            return None;
        }
        self.nesting += 1;
        let value = inner(self);
        self.nesting -= 1;
        value
    }

    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<Token> {
        let token = self.peek()?;
        self.pos += 1;
        Some(token)
    }

    fn expression(&mut self) -> Option<f64> {
        let mut value = self.term()?;
        while let Some(op @ (Token::Plus | Token::Minus)) = self.peek() {
            self.bump();
            let rhs = self.term()?;
            self.operations += 1;
            value = if op == Token::Plus { value + rhs } else { value - rhs };
        }
        Some(value)
    }

    fn term(&mut self) -> Option<f64> {
        let mut value = self.unary()?;
        loop {
            match self.peek() {
                Some(Token::Star) => {
                    self.bump();
                    value *= self.unary()?;
                }
                Some(op @ (Token::Slash | Token::Percent)) if op == Token::Slash || self.percent_is_modulo() => {
                    self.bump();
                    let rhs = self.unary()?;
                    if rhs == 0.0 {
                        return None;
                    }
                    value = if op == Token::Slash { value / rhs } else { value % rhs };
                }
                _ => return Some(value),
            }
            self.operations += 1;
        }
    }

    fn percent_is_modulo(&self) -> bool {
        matches!(self.tokens.get(self.pos + 1), Some(Token::Number(_) | Token::Open))
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some(Token::Minus) => {
                self.bump();
                Some(-self.nested(Self::unary)?)
            }
            Some(Token::Plus) => {
                self.bump();
                self.nested(Self::unary)
            }
            _ => self.postfix(),
        }
    }

    fn postfix(&mut self) -> Option<f64> {
        let mut value = self.primary()?;
        while self.peek() == Some(Token::Percent) && !self.percent_is_modulo() {
            self.bump();
            self.operations += 1;
            value /= 100.0;
        }
        Some(value)
    }

    fn primary(&mut self) -> Option<f64> {
        match self.bump()? {
            Token::Number(value) => Some(value),
            Token::Open => {
                let value = self.nested(Self::expression)?;
                if self.bump()? != Token::Close {
                    return None;
                }
                self.operations += 1;
                Some(value)
            }
            _ => None,
        }
    }
}

fn evaluate_counting(input: &str) -> Option<(f64, usize)> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() || tokens.len() > MAX_TOKENS {
        return None;
    }
    let mut parser = Parser { tokens, pos: 0, operations: 0, nesting: 0 };
    let value = parser.expression()?;
    if parser.pos != parser.tokens.len() || !value.is_finite() {
        return None;
    }
    Some((value, parser.operations))
}

#[must_use]
pub fn calculation(input: &str) -> Option<String> {
    let (value, operations) = evaluate_counting(input)?;
    if operations == 0 {
        return None;
    }
    let comma = input.contains(',') && !input.contains('.');
    Some(format_number(value, comma))
}

#[must_use]
pub fn format_number(value: f64, comma: bool) -> String {
    let value = value + 0.0;
    let rounded = value.round() + 0.0;
    let text = if (value - rounded).abs() < 1e-9 && rounded.abs() < 1e15 {
        format!("{rounded:.0}")
    } else {
        let fixed = format!("{value:.10}");
        fixed.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    if comma { text.replace('.', ",") } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(input: &str) -> Option<f64> {
        evaluate_counting(input).map(|(value, _)| value)
    }

    fn close(input: &str, want: f64) {
        let got = eval(input).unwrap_or_else(|| panic!("{input} did not evaluate"));
        assert!((got - want).abs() < 1e-9, "{input} = {got}, want {want}");
    }

    #[test]
    fn precedence_puts_multiplication_before_addition() {
        close("2+3*4", 14.0);
        close("2*3+4", 10.0);
        close("10-4-3", 3.0);
        close("100/10/5", 2.0);
    }

    #[test]
    fn a_pasted_wall_of_brackets_or_signs_is_refused_instead_of_overflowing_the_stack() {
        let brackets = "(".repeat(64 * 1024);
        assert_eq!(calculation(&brackets), None);
        let balanced = format!("{}1{}+1", "(".repeat(100_000), ")".repeat(100_000));
        assert_eq!(calculation(&balanced), None);
        let signs = format!("{}1+1", "-".repeat(64 * 1024));
        assert_eq!(calculation(&signs), None);
        let deep = format!("{}1{}+1", "(".repeat(MAX_NESTING + 1), ")".repeat(MAX_NESTING + 1));
        assert_eq!(calculation(&deep), None);
        let shallow = format!("{}1{}+1", "(".repeat(MAX_NESTING), ")".repeat(MAX_NESTING));
        assert_eq!(calculation(&shallow).as_deref(), Some("2"));
    }

    #[test]
    fn parentheses_override_precedence() {
        close("(2+3)*4", 20.0);
        close("((1))", 1.0);
        close("2*(3+(4-1))", 12.0);
    }

    #[test]
    fn unary_minus_binds_to_its_operand() {
        close("-5+2", -3.0);
        close("3*-2", -6.0);
        close("-(2+3)", -5.0);
        close("--4", 4.0);
    }

    #[test]
    fn decimals_accept_a_point_or_a_comma() {
        close("1.5*2", 3.0);
        close("1,5*2", 3.0);
        close(".5+.5", 1.0);
        assert_eq!(eval("1.2.3"), None, "two separators are not a number");
    }

    #[test]
    fn percent_is_a_percentage_after_an_operand_and_modulo_between_two() {
        close("50%", 0.5);
        close("200*15%", 30.0);
        close("10 % 3", 1.0);
        close("(10)%(4)", 2.0);
    }

    #[test]
    fn a_percent_before_a_sign_is_a_percentage_and_never_a_modulo() {
        close("100*50%-10", 40.0);
        close("50%+1", 1.5);
        close("100+50%-20", 80.5);
        close("200*15% - 5", 25.0);
        close("50%%", 0.005);
        close("10 % (3)", 1.0);
        close("10 %3", 1.0);
    }

    #[test]
    fn whitespace_and_typographic_operators_are_tolerated() {
        close(" 6 \u{d7} 7 ", 42.0);
        close("8 \u{f7} 2", 4.0);
        close("5 \u{2212} 2", 3.0);
    }

    #[test]
    fn malformed_input_evaluates_to_nothing() {
        for input in ["", "   ", "2+", "*2", "(1+2", "1+2)", "abc", "2 3", "1/0", "5%0", "()"] {
            assert_eq!(eval(input), None, "{input:?}");
        }
    }

    #[test]
    fn a_bare_number_is_not_offered_as_a_calculation() {
        assert_eq!(calculation("42"), None);
        assert_eq!(calculation("-42"), None);
        assert_eq!(calculation("firefox"), None);
        assert_eq!(calculation("6*7").as_deref(), Some("42"));
        assert_eq!(calculation("(42)").as_deref(), Some("42"));
    }

    #[test]
    fn results_are_formatted_without_float_noise() {
        assert_eq!(calculation("0.1+0.2").as_deref(), Some("0.3"));
        assert_eq!(calculation("1/3").as_deref(), Some("0.3333333333"));
        assert_eq!(calculation("10/4").as_deref(), Some("2.5"));
        assert_eq!(calculation("0*-1").as_deref(), Some("0"));
    }

    #[test]
    fn a_comma_query_gets_a_comma_answer() {
        assert_eq!(calculation("1,5+1").as_deref(), Some("2,5"));
    }
}
