#[derive(Clone, Copy, PartialEq)]
enum Token<'a> {
    Name(&'a str),
    Literal,
    Punctuation(char),
}

#[derive(Clone)]
struct JavascriptTokens<'a> {
    remaining: &'a str,
    previous: Option<Token<'a>>,
    literal_depth: usize,
}

impl<'a> JavascriptTokens<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            remaining: text,
            previous: None,
            literal_depth: 0,
        }
    }

    fn advance(&mut self) -> Option<char> {
        let character = self.remaining.chars().next()?;
        self.remaining = &self.remaining[character.len_utf8()..];
        Some(character)
    }

    fn skip_comment(&mut self) -> bool {
        if self.remaining.starts_with("//") {
            self.remaining = self
                .remaining
                .trim_start_matches(|c| c != '\n' && c != '\r');
        } else if self.remaining.starts_with("/*") {
            self.remaining = self.remaining.split_once("*/").map_or("", |(_, rest)| rest);
        } else {
            return false;
        }
        true
    }

    fn skip_quoted(&mut self, quote: char) {
        // Bound nesting in untrusted source; templates remain opaque evidence.
        if self.literal_depth >= 64 {
            self.remaining = "";
            return;
        }
        self.literal_depth += 1;
        while let Some(character) = self.advance() {
            match character {
                '\\' => {
                    self.advance();
                }
                c if c == quote => break,
                '$' if quote == '`' && self.remaining.starts_with('{') => {
                    self.advance();
                    self.previous = Some(Token::Punctuation('{'));
                    let mut braces = 1usize;
                    while let Some(token) = self.next() {
                        match token {
                            Token::Punctuation('{') => braces += 1,
                            Token::Punctuation('}') => {
                                braces -= 1;
                                if braces == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        self.literal_depth -= 1;
    }

    fn regex_allowed(&self) -> bool {
        match self.previous {
            None => true,
            Some(Token::Punctuation(c)) => "=([{,:;!?&|+-*%^~<>".contains(c),
            Some(Token::Name("return" | "throw" | "case" | "yield" | "typeof" | "void")) => true,
            _ => false,
        }
    }

    fn skip_regex(&mut self) {
        let mut character_class = false;
        while let Some(character) = self.advance() {
            match character {
                '\\' => {
                    self.advance();
                }
                '[' => character_class = true,
                ']' => character_class = false,
                '/' if !character_class => break,
                '\n' | '\r' => break,
                _ => {}
            }
        }
        self.remaining = self.remaining.trim_start_matches(char::is_alphabetic);
    }

    fn next(&mut self) -> Option<Token<'a>> {
        loop {
            self.remaining = self
                .remaining
                .trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
            if !self.skip_comment() {
                break;
            }
        }
        let source = self.remaining;
        let character = self.advance()?;
        let token = match character {
            '\'' | '"' | '`' => {
                self.skip_quoted(character);
                Token::Literal
            }
            '/' if self.regex_allowed() => {
                self.skip_regex();
                Token::Literal
            }
            c if identifier_character(c) => {
                self.remaining = self.remaining.trim_start_matches(identifier_character);
                Token::Name(&source[..source.len() - self.remaining.len()])
            }
            c => Token::Punctuation(c),
        };
        self.previous = Some(token);
        Some(token)
    }
}

fn identifier_character(character: char) -> bool {
    character.is_alphanumeric()
        || matches!(character, '_' | '$')
        || (!character.is_ascii() && !character.is_whitespace())
}

fn test_call(mut tokens: JavascriptTokens<'_>) -> bool {
    let mut token = tokens.next();
    while token == Some(Token::Punctuation('.')) {
        if !matches!(tokens.next(), Some(Token::Name("only" | "skip"))) {
            return false;
        }
        token = tokens.next();
    }
    if token != Some(Token::Punctuation('(')) {
        return false;
    }
    let mut depth = 0usize;
    let mut comma = false;
    let mut callback = false;
    while let Some(token) = tokens.next() {
        match token {
            Token::Punctuation(')') if depth == 0 => {
                return callback && tokens.next() != Some(Token::Punctuation('{'));
            }
            Token::Punctuation(',') if depth == 0 => comma = true,
            _ => {
                callback |= comma;
                match token {
                    Token::Punctuation('(' | '[' | '{') => depth += 1,
                    Token::Punctuation(')' | ']' | '}') => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
        }
    }
    false
}

pub(super) fn javascript_has_inline_tests(text: &str) -> bool {
    let mut tokens = JavascriptTokens::new(text);
    let mut previous = None;
    while let Some(token) = tokens.next() {
        if matches!(token, Token::Name("describe" | "test" | "it"))
            && !matches!(
                previous,
                Some(Token::Punctuation('.') | Token::Name("function" | "new"))
            )
            && test_call(tokens.clone())
        {
            return true;
        }
        previous = Some(token);
    }
    false
}

#[cfg(test)]
mod tests;
