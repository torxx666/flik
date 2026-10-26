//! Boolean filter grammar: NOT > AND > OR; quoted atoms keep operators literal.
use crate::filter::{FilterKind, FilterRule};

#[derive(Debug, Clone)]
pub enum Expression {
    Atom(FilterRule),
    Not(Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}
impl Expression {
    pub fn matches(&self, line: &str) -> bool {
        match self {
            Self::Atom(rule) => rule.matches(line),
            Self::Not(expr) => !expr.matches(line),
            Self::And(a, b) => a.matches(line) && b.matches(line),
            Self::Or(a, b) => a.matches(line) || b.matches(line),
        }
    }
}
#[derive(Debug, PartialEq)]
enum Token {
    Atom(String),
    Left,
    Right,
    And,
    Or,
    Not,
}
fn tokens(input: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let mut atom = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut index = 0;
    let chars: Vec<char> = input.chars().collect();
    let flush = |atom: &mut String, out: &mut Vec<Token>| {
        if !atom.trim().is_empty() {
            out.push(Token::Atom(atom.trim().to_owned()));
        }
        atom.clear();
    };
    while index < chars.len() {
        let ch = chars[index];
        if let Some(q) = quote {
            atom.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            index += 1;
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            atom.push(ch);
            index += 1;
            continue;
        }
        if ch == '(' || ch == ')' {
            flush(&mut atom, &mut out);
            out.push(if ch == '(' { Token::Left } else { Token::Right });
            index += 1;
            continue;
        }
        let mut operator = None;
        if index == 0
            || chars[index - 1].is_whitespace()
            || chars[index - 1] == ')'
            || chars[index - 1] == '('
        {
            for word in ["AND", "OR", "NOT"] {
                let end = index + word.len();
                if end <= chars.len()
                    && chars[index..end].iter().copied().eq(word.chars())
                    && (end == chars.len()
                        || chars[end].is_whitespace()
                        || chars[end] == '('
                        || chars[end] == ')')
                {
                    operator = Some((word, end));
                    break;
                }
            }
        }
        if let Some((word, end)) = operator {
            flush(&mut atom, &mut out);
            out.push(match word {
                "AND" => Token::And,
                "OR" => Token::Or,
                _ => Token::Not,
            });
            index = end;
        } else {
            atom.push(ch);
            index += 1;
        }
    }
    if quote.is_some() {
        return Err("Unclosed quote in filter".into());
    }
    flush(&mut atom, &mut out);
    Ok(out)
}

pub fn parse(input: &str, case_sensitive: bool) -> Result<Option<Expression>, String> {
    let list = match tokens(input) {
        Ok(tokens) => tokens,
        Err(_)
            if ![" AND ", " OR ", "NOT ", "(", ")"]
                .iter()
                .any(|part| input.contains(part)) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    if !list.iter().any(|t| {
        matches!(
            t,
            Token::And | Token::Or | Token::Not | Token::Left | Token::Right
        )
    }) {
        return Ok(None);
    }
    if list.len() > 512 {
        return Err("Filter is too complex (maximum 512 tokens)".into());
    }
    struct Parser {
        tokens: Vec<Token>,
        pos: usize,
        case_sensitive: bool,
    }
    impl Parser {
        fn expr(&mut self, min: u8, depth: usize) -> Result<Expression, String> {
            if depth > 64 {
                return Err("Filter nesting is too deep".into());
            }
            let token = self.tokens.get(self.pos).ok_or("Missing filter operand")?;
            self.pos += 1;
            let mut left = match token {
                Token::Atom(value) => Expression::Atom(FilterRule::new(
                    0,
                    value.clone(),
                    FilterKind::Include,
                    false,
                    self.case_sensitive,
                )?),
                Token::Not => Expression::Not(Box::new(self.expr(3, depth + 1)?)),
                Token::Left => {
                    let expr = self.expr(0, depth + 1)?;
                    if self.tokens.get(self.pos) != Some(&Token::Right) {
                        return Err("Missing closing parenthesis".into());
                    }
                    self.pos += 1;
                    expr
                }
                _ => return Err("Expected a term, NOT or an opening parenthesis".into()),
            };
            loop {
                let power = match self.tokens.get(self.pos) {
                    Some(Token::Or) => 1,
                    Some(Token::And) => 2,
                    _ => break,
                };
                if power < min {
                    break;
                }
                self.pos += 1;
                let right = self.expr(power + 1, depth + 1)?;
                left = if power == 1 {
                    Expression::Or(Box::new(left), Box::new(right))
                } else {
                    Expression::And(Box::new(left), Box::new(right))
                };
            }
            Ok(left)
        }
    }
    let mut parser = Parser {
        tokens: list,
        pos: 0,
        case_sensitive,
    };
    let expr = parser.expr(0, 0)?;
    if parser.pos != parser.tokens.len() {
        return Err("Unexpected token; use AND or OR between terms".into());
    }
    Ok(Some(expr))
}
