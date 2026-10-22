// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The expression of a buffer's size (`"WIDTH": "$WIDTH/4"`), read into the tree that the engine works out for each call (see `docs/engine-phase-6-4-design.md`, K2).
//!
//! What it may have: numbers, `$WIDTH` and `$HEIGHT` (the size of the picture the effect works on), the name of one of the shader's inputs that is a float, a long or a bool (its
//! value in the call), `+ - * / %`, brackets, a minus or a plus in front of something, and the functions that `MathFunction` has. Anything else is told, with the place in the text.

use crate::engine::{MathFunction, Operator, SizeExpression};

/// The longest an expression of a size may be, in characters.
pub const LONGEST_EXPRESSION: usize = 512;

/// How deeply brackets, and functions in functions, may be nested in an expression.
pub const MOST_NESTING_OF_AN_EXPRESSION: usize = 32;

/// What the names of the inputs of a shader are, for the expressions: the name and the place of its number among the numbers of a call, for the inputs that are one number.
pub type NamesOfNumbers<'names> = &'names [(&'names str, usize)];

struct Parser<'names> {
    characters: Vec<char>,
    at: usize,
    names: NamesOfNumbers<'names>,
    depth: usize,
}

fn function_named(name: &str) -> Option<MathFunction> {
    Some(match name {
        "floor" => MathFunction::Floor,
        "ceil" => MathFunction::Ceil,
        "round" => MathFunction::Round,
        "abs" => MathFunction::Abs,
        "min" => MathFunction::Min,
        "max" => MathFunction::Max,
        "pow" => MathFunction::Pow,
        "sqrt" => MathFunction::Sqrt,
        "sin" => MathFunction::Sin,
        "cos" => MathFunction::Cos,
        "exp" => MathFunction::Exp,
        "log" => MathFunction::Log,
        "log2" => MathFunction::Log2,
        "sign" => MathFunction::Sign,
        _ => return None,
    })
}

impl Parser<'_> {
    fn skip_spaces(&mut self) {
        while self
            .characters
            .get(self.at)
            .is_some_and(|character| character.is_whitespace())
        {
            self.at += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_spaces();
        self.characters.get(self.at).copied()
    }

    fn fail<T>(&self, message: impl Into<String>) -> Result<T, String> {
        self.fail_at(self.at, message)
    }

    /// Fails at the character `position` (from 0), which is where a word or a number that is wrong begins and not where it ends.
    fn fail_at<T>(&self, position: usize, message: impl Into<String>) -> Result<T, String> {
        Err(format!(
            "{} (at character {} of the expression)",
            message.into(),
            position + 1
        ))
    }

    /// `a + b - c...`
    fn sum(&mut self) -> Result<SizeExpression, String> {
        let mut left = self.product()?;
        while let Some(character @ ('+' | '-')) = self.peek() {
            self.at += 1;
            let right = self.product()?;
            left = SizeExpression::Binary(
                if character == '+' {
                    Operator::Add
                } else {
                    Operator::Subtract
                },
                Box::new(left),
                Box::new(right),
            );
        }
        Ok(left)
    }

    /// `a * b / c % d...`
    fn product(&mut self) -> Result<SizeExpression, String> {
        let mut left = self.sign()?;
        while let Some(character @ ('*' | '/' | '%')) = self.peek() {
            self.at += 1;
            let right = self.sign()?;
            left = SizeExpression::Binary(
                match character {
                    '*' => Operator::Multiply,
                    '/' => Operator::Divide,
                    _ => Operator::Remainder,
                },
                Box::new(left),
                Box::new(right),
            );
        }
        Ok(left)
    }

    /// `-a`, `+a`, or just `a`.
    fn sign(&mut self) -> Result<SizeExpression, String> {
        match self.peek() {
            Some('-') => {
                self.at += 1;
                self.nested(|parser| Ok(SizeExpression::Negate(Box::new(parser.sign()?))))
            }
            Some('+') => {
                self.at += 1;
                self.nested(Self::sign)
            }
            _ => self.primary(),
        }
    }

    /// Does `inner` one level deeper, and refuses a level too deep.
    fn nested(
        &mut self,
        inner: impl FnOnce(&mut Self) -> Result<SizeExpression, String>,
    ) -> Result<SizeExpression, String> {
        if self.depth >= MOST_NESTING_OF_AN_EXPRESSION {
            return self.fail(format!("it is nested more than {MOST_NESTING_OF_AN_EXPRESSION} deep"));
        }
        self.depth += 1;
        let result = inner(self);
        self.depth -= 1;
        result
    }

    /// A number, `$WIDTH`, `$HEIGHT`, a name, a function and its arguments, or an expression in brackets.
    fn primary(&mut self) -> Result<SizeExpression, String> {
        let Some(character) = self.peek() else {
            return self.fail("the expression ends where something was wanted");
        };
        let start = self.at;
        if character == '(' {
            self.at += 1;
            let inner = self.nested(Self::sum)?;
            return match self.peek() {
                Some(')') => {
                    self.at += 1;
                    Ok(inner)
                }
                _ => self.fail("a bracket is not closed"),
            };
        }
        if character.is_ascii_digit() || character == '.' {
            return self.number();
        }
        if character == '$' {
            self.at += 1;
            let name = self.word();
            return match name.as_str() {
                "WIDTH" => Ok(SizeExpression::Width),
                "HEIGHT" => Ok(SizeExpression::Height),
                other => self.fail_at(
                    start,
                    format!("${other} is not something that a size can use ($WIDTH and $HEIGHT are)"),
                ),
            };
        }
        if character.is_ascii_alphabetic() || character == '_' {
            let name = self.word();
            if self.peek() == Some('(') {
                return self.call(&name, start);
            }
            return match self.names.iter().find(|(known, _)| *known == name) {
                Some((_, place)) => Ok(SizeExpression::Value(*place)),
                None => self.fail_at(
                    start,
                    format!("{name} is not an input of the shader that is a number (a float, a long or a bool), and not a function"),
                ),
            };
        }
        self.fail(format!("{character} cannot be in the expression of a size"))
    }

    /// The letters, digits and underscores that are next.
    fn word(&mut self) -> String {
        let start = self.at;
        while self
            .characters
            .get(self.at)
            .is_some_and(|character| character.is_ascii_alphanumeric() || *character == '_')
        {
            self.at += 1;
        }
        self.characters[start..self.at].iter().collect()
    }

    /// A number: digits, a point, digits, and an exponent.
    fn number(&mut self) -> Result<SizeExpression, String> {
        let start = self.at;
        let digits = |parser: &mut Self| {
            while parser.characters.get(parser.at).is_some_and(char::is_ascii_digit) {
                parser.at += 1;
            }
        };
        digits(self);
        if self.characters.get(self.at) == Some(&'.') {
            self.at += 1;
            digits(self);
        }
        if matches!(self.characters.get(self.at), Some('e' | 'E')) {
            let before_exponent = self.at;
            self.at += 1;
            if matches!(self.characters.get(self.at), Some('+' | '-')) {
                self.at += 1;
            }
            let first_digit = self.at;
            digits(self);
            if self.at == first_digit {
                // (An `e` that has no digits after it is not an exponent.)
                self.at = before_exponent;
            }
        }
        let text: String = self.characters[start..self.at].iter().collect();
        match text.parse::<f64>() {
            Ok(number) if number.is_finite() => Ok(SizeExpression::Number(number)),
            _ => self.fail_at(start, format!("{text} is not a number")),
        }
    }

    /// The arguments of the function `name`, in brackets; the name began at the character `start`.
    fn call(&mut self, name: &str, start: usize) -> Result<SizeExpression, String> {
        let Some(function) = function_named(name) else {
            return self.fail_at(start, format!("{name} is not a function that a size can use"));
        };
        self.at += 1; // the bracket
        let mut arguments = Vec::new();
        loop {
            arguments.push(self.nested(Self::sum)?);
            match self.peek() {
                Some(',') => self.at += 1,
                Some(')') => {
                    self.at += 1;
                    break;
                }
                _ => return self.fail(format!("a bracket of {name} is not closed")),
            }
        }
        if arguments.len() != function.arguments() {
            return self.fail_at(
                start,
                format!(
                    "{name} takes {} argument{} and is given {}",
                    function.arguments(),
                    if function.arguments() == 1 { "" } else { "s" },
                    arguments.len()
                ),
            );
        }
        Ok(SizeExpression::Function(function, arguments))
    }
}

/// Reads the expression `text` of a size. `names` are the inputs of the shader that are one number each, with the place of that number among the numbers of a call.
pub fn read_size_expression(text: &str, names: NamesOfNumbers<'_>) -> Result<SizeExpression, String> {
    if text.chars().count() > LONGEST_EXPRESSION {
        return Err(format!("the expression is longer than {LONGEST_EXPRESSION} characters"));
    }
    let mut parser = Parser {
        characters: text.chars().collect(),
        at: 0,
        names,
        depth: 0,
    };
    let expression = parser.sum()?;
    if parser.peek().is_some() {
        return parser.fail(format!(
            "{} cannot follow the end of the expression",
            parser.characters[parser.at]
        ));
    }
    Ok(expression)
}

#[cfg(test)]
mod tests;
