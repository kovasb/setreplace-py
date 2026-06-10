//! Substitution rules and a small parser for Wolfram-Language-like rule syntax.

use crate::error::Error;

/// An atom (hypergraph vertex). Following libSetReplace's convention:
/// positive values are concrete atoms, negative values are pattern variables
/// (only meaningful inside rules). Zero is invalid.
pub type Atom = i64;

/// A substitution rule: a list of input pattern hyperedges and a list of
/// output hyperedges. Negative atoms are pattern variables; a variable that
/// appears only in the outputs creates a fresh atom each time the rule fires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub inputs: Vec<Vec<Atom>>,
    pub outputs: Vec<Vec<Atom>>,
}

impl Rule {
    /// Creates a rule, validating the libSetReplace conventions:
    /// at least one input edge, and no zero atoms anywhere.
    pub fn new(inputs: Vec<Vec<Atom>>, outputs: Vec<Vec<Atom>>) -> Result<Rule, Error> {
        if inputs.is_empty() {
            return Err(Error::InvalidRule(
                "rule must have at least one input edge".into(),
            ));
        }
        for edge in inputs.iter().chain(outputs.iter()) {
            if edge.contains(&0) {
                return Err(Error::InvalidRule(
                    "atom 0 is invalid; use positive atoms (concrete) or negative (variables)"
                        .into(),
                ));
            }
        }
        Ok(Rule { inputs, outputs })
    }

    /// Parses a rule written in Wolfram-Language-like syntax, e.g.
    ///
    /// ```text
    /// {{x, y}} -> {{x, y}, {y, z}}
    /// {{a_, b_}, {b_, c_}} :> {{a, c}}
    /// {{1}, {2}} -> {{3}}
    /// ```
    ///
    /// Identifiers (with or without a trailing `_`) are pattern variables and
    /// are assigned negative atom ids in order of first appearance. Integer
    /// literals are concrete atoms (the semantics of SetReplace's
    /// `"PatternRules"`; to make every vertex a variable, write letters).
    pub fn parse(s: &str) -> Result<Rule, Error> {
        let tokens = lex(s)?;
        let mut p = Parser::new(&tokens);
        let inputs = p.edge_list()?;
        p.arrow()?;
        let outputs = p.edge_list()?;
        p.end()?;

        let mut vars: Vec<String> = Vec::new();
        let mut resolve = |atom: &RawAtom| -> Atom {
            match atom {
                RawAtom::Int(n) => *n,
                RawAtom::Var(name) => {
                    let idx = vars.iter().position(|v| v == name).unwrap_or_else(|| {
                        vars.push(name.clone());
                        vars.len() - 1
                    });
                    -((idx + 1) as Atom)
                }
            }
        };
        let inputs = inputs
            .iter()
            .map(|e| e.iter().map(&mut resolve).collect())
            .collect();
        let outputs = outputs
            .iter()
            .map(|e| e.iter().map(&mut resolve).collect())
            .collect();
        Rule::new(inputs, outputs)
    }
}

/// Parses an initial state written as a list of hyperedges of positive
/// integers, e.g. `{{1, 2}, {2, 3}}`.
pub fn parse_state(s: &str) -> Result<Vec<Vec<Atom>>, Error> {
    let tokens = lex(s)?;
    let mut p = Parser::new(&tokens);
    let edges = p.edge_list()?;
    p.end()?;
    edges
        .iter()
        .map(|e| {
            e.iter()
                .map(|a| match a {
                    RawAtom::Int(n) => Ok(*n),
                    RawAtom::Var(name) => Err(Error::Parse(format!(
                        "initial states must contain integer atoms only, found `{name}`"
                    ))),
                })
                .collect()
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
enum RawAtom {
    Int(i64),
    Var(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Open,
    Close,
    Comma,
    Arrow,
    Atom(RawAtom),
}

fn lex(s: &str) -> Result<Vec<Tok>, Error> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '{' => {
                out.push(Tok::Open);
                i += 1;
            }
            '}' => {
                out.push(Tok::Close);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '-' | ':' => {
                if i + 1 < bytes.len() && bytes[i + 1] as char == '>' {
                    out.push(Tok::Arrow);
                    i += 2;
                } else {
                    return Err(Error::Parse(format!("unexpected character `{c}` at {i}")));
                }
            }
            '0'..='9' => {
                let start = i;
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
                let n: i64 = s[start..i]
                    .parse()
                    .map_err(|_| Error::Parse(format!("invalid integer at {start}")))?;
                out.push(Tok::Atom(RawAtom::Int(n)));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < bytes.len() {
                    let c = bytes[i] as char;
                    if c.is_ascii_alphanumeric() || c == '_' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let mut name = &s[start..i];
                // `a_` pattern syntax: the trailing blank is part of the name.
                if let Some(stripped) = name.strip_suffix('_') {
                    name = stripped;
                }
                if name.is_empty() {
                    return Err(Error::Parse(format!("invalid identifier at {start}")));
                }
                out.push(Tok::Atom(RawAtom::Var(name.to_string())));
            }
            _ => return Err(Error::Parse(format!("unexpected character `{c}` at {i}"))),
        }
    }
    Ok(out)
}

struct Parser<'a> {
    tokens: &'a [Tok],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Tok]) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn expect(&mut self, t: Tok, what: &str) -> Result<(), Error> {
        if self.peek() == Some(&t) {
            self.pos += 1;
            Ok(())
        } else {
            Err(Error::Parse(format!(
                "expected {what} at token {}",
                self.pos
            )))
        }
    }

    /// `{ edge, edge, ... }` (possibly empty)
    fn edge_list(&mut self) -> Result<Vec<Vec<RawAtom>>, Error> {
        self.expect(Tok::Open, "`{`")?;
        let mut edges = Vec::new();
        if self.peek() == Some(&Tok::Close) {
            self.pos += 1;
            return Ok(edges);
        }
        loop {
            edges.push(self.edge()?);
            match self.peek() {
                Some(Tok::Comma) => self.pos += 1,
                Some(Tok::Close) => {
                    self.pos += 1;
                    return Ok(edges);
                }
                _ => return Err(Error::Parse("expected `,` or `}`".into())),
            }
        }
    }

    /// `{ atom, atom, ... }` (possibly empty)
    fn edge(&mut self) -> Result<Vec<RawAtom>, Error> {
        self.expect(Tok::Open, "`{`")?;
        let mut atoms = Vec::new();
        if self.peek() == Some(&Tok::Close) {
            self.pos += 1;
            return Ok(atoms);
        }
        loop {
            match self.peek() {
                Some(Tok::Atom(a)) => {
                    atoms.push(a.clone());
                    self.pos += 1;
                }
                _ => return Err(Error::Parse("expected atom".into())),
            }
            match self.peek() {
                Some(Tok::Comma) => self.pos += 1,
                Some(Tok::Close) => {
                    self.pos += 1;
                    return Ok(atoms);
                }
                _ => return Err(Error::Parse("expected `,` or `}`".into())),
            }
        }
    }

    fn arrow(&mut self) -> Result<(), Error> {
        self.expect(Tok::Arrow, "`->` or `:>`")
    }

    fn end(&mut self) -> Result<(), Error> {
        if self.pos == self.tokens.len() {
            Ok(())
        } else {
            Err(Error::Parse("unexpected trailing input".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_growth_rule() {
        let r = Rule::parse("{{x, y}} -> {{x, y}, {y, z}}").unwrap();
        assert_eq!(r.inputs, vec![vec![-1, -2]]);
        assert_eq!(r.outputs, vec![vec![-1, -2], vec![-2, -3]]);
    }

    #[test]
    fn parses_blank_pattern_syntax() {
        let r = Rule::parse("{{a_, b_}, {b_, c_}} :> {{a, c}}").unwrap();
        assert_eq!(r.inputs, vec![vec![-1, -2], vec![-2, -3]]);
        assert_eq!(r.outputs, vec![vec![-1, -3]]);
    }

    #[test]
    fn integers_are_concrete() {
        let r = Rule::parse("{{1}, {2}} -> {{3}}").unwrap();
        assert_eq!(r.inputs, vec![vec![1], vec![2]]);
        assert_eq!(r.outputs, vec![vec![3]]);
    }

    #[test]
    fn parses_empty_output() {
        let r = Rule::parse("{{x, y}} -> {}").unwrap();
        assert_eq!(r.inputs, vec![vec![-1, -2]]);
        assert!(r.outputs.is_empty());
    }

    #[test]
    fn parses_state() {
        assert_eq!(
            parse_state("{{1, 2}, {2, 3}}").unwrap(),
            vec![vec![1, 2], vec![2, 3]]
        );
    }

    #[test]
    fn rejects_empty_inputs() {
        assert!(Rule::parse("{} -> {{1, 2}}").is_err());
    }
}
