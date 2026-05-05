use std::str::Chars;

use anyhow::bail;

pub fn migrate() -> Result<(), anyhow::Error> {
	Ok(())
}

pub struct Lexer<'a> {
	source: &'a str,
	chars: Chars<'a>,
	state: LexerState,
}

enum LexerState {
	Default,
	CellMarker,
}

#[cfg_attr(test, derive(Debug))]
pub struct Token {
	pub kind: TokenKind,
	pub start: usize,
	pub end: usize,
}

#[cfg_attr(test, derive(Debug, PartialEq))]
pub enum TokenKind {
	Frontmatter,
	CellMarker,
	CellType,
	CellConfig,
	CellContent,
	BlockComment,
	LineComment,
	Eof,
}

impl<'a> Lexer<'a> {
	pub fn new(source: &'a str) -> Self {
		Lexer {
			source,
			chars: source.chars(),
			state: LexerState::Default,
		}
	}

	pub fn read_next_token(&mut self) -> Result<Token, anyhow::Error> {
		let start = self._offset();
		let kind = self.read_next_kind()?;
		let end = self._offset();

		Ok(Token { kind, start, end })
	}

	pub fn read_next_kind(&mut self) -> Result<TokenKind, anyhow::Error> {
		while let Some(c) = self.chars.next() {
			match self.state {
				LexerState::Default => match c {
					'/' if self._peek() == Some('*') => {
						self.chars.next();
						return Ok(self._frontmatter());
					}
					'-' if self._peek() == Some('-') => {
						self.chars.next();

						match self._cell_marker() {
							Some(marker) => return Ok(marker),
							None => {
								// consume the rest of the line if the comment starts with
								// non-whitespace characters
								while let Some(c) = self.chars.next() {
									if c == '\n' {
										return Ok(TokenKind::LineComment);
									}
								}

								// end of file reached before newline
								return Ok(TokenKind::LineComment);
							}
						}
					}
					_ => {
						if let Some(content) = self._cell_content() {
							return Ok(content);
						}
					}
				},
				LexerState::CellMarker => match c {
					'[' => {
						return self._cell_type();
					}
					'{' => {
						return self._cell_config();
					}
					'\n' => {
						self.state = LexerState::Default;
					}
					c if c.is_whitespace() => {}
					_ => bail!("unexpected characters in the cell marker"),
				},
			}
		}

		Ok(TokenKind::Eof)
	}
}

impl<'a> Lexer<'a> {
	fn _frontmatter(&mut self) -> TokenKind {
		let mut count = 0;
		let mut invalid = false;
		while let Some(c) = self.chars.next() {
			// not a frontmatter if there are non-whitespace characters before the first
			// "---" or after the last "---"
			match c {
				'-' => {
					if self._peek() == Some('-') {
						self.chars.next();

						if self._peek() == Some('-') {
							self.chars.next();
							count += 1;
						} else if count == 0 || count == 2 {
							invalid = true;
						}
					} else if count == 0 || count == 2 {
						invalid = true;
					}
				}
				'*' => {
					if self._peek() == Some('/') {
						self.chars.next();
						break;
					}
				}
				c if c.is_whitespace() => {}
				_ => {
					if count == 0 || count == 2 {
						invalid = true;
					}
				}
			}
		}

		if count == 2 && !invalid {
			TokenKind::Frontmatter
		} else {
			TokenKind::BlockComment
		}
	}

	fn _cell_marker(&mut self) -> Option<TokenKind> {
		while let Some(c) = self.chars.next() {
			// "--" and "%%" can be separated by whitespaces
			// not a cell marker if there are non-whitespace characters before "%%"
			match c {
				'%' => {
					if self._peek() == Some('%') {
						self.chars.next();
						self.state = LexerState::CellMarker;
						return Some(TokenKind::CellMarker);
					}
				}
				'\n' => return Some(TokenKind::LineComment),
				c if c.is_whitespace() => {}
				_ => break,
			}
		}

		None
	}

	fn _cell_type(&mut self) -> Result<TokenKind, anyhow::Error> {
		let mut kind: Option<TokenKind> = None;
		while let Some(c) = self.chars.next() {
			match c {
				']' => {
					kind = Some(TokenKind::CellType);
					break;
				}
				'\n' => break,
				_ => {}
			}
		}

		if let Some(k) = kind {
			Ok(k)
		} else {
			bail!("expected ']' but found newline or end of file");
		}
	}

	fn _cell_config(&mut self) -> Result<TokenKind, anyhow::Error> {
		let mut braces = 1;
		while let Some(c) = self.chars.next() {
			match c {
				'{' => braces += 1,
				'}' => {
					braces -= 1;
					if braces == 0 {
						break;
					}
				}
				'\n' => break,
				_ => {}
			}
		}

		if braces == 0 {
			Ok(TokenKind::CellConfig)
		} else {
			bail!("unbalanced braces in the cell config");
		}
	}

	fn _cell_content(&mut self) -> Option<TokenKind> {
		let mut chars = self.chars.clone();
		let first = chars.next();
		let second = chars.next();

		if first.is_none() {
			return Some(TokenKind::CellContent);
		}

		if first == Some('-') && second == Some('-') {
			return Some(TokenKind::CellContent);
		}

		None
	}

	fn _peek(&self) -> Option<char> {
		self.chars.clone().next()
	}

	fn _offset(&self) -> usize {
		self.source.len() - self.chars.as_str().len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_frontmatters() {
		let fm = "
/*
---
dialect: postgres
---
*/
		"
		.trim();
		let mut lexer = Lexer::new(&fm);

		let t = lexer.read_next_token().unwrap();
		assert_eq!(t.kind, TokenKind::Frontmatter);
		assert_eq!(t.start, 0);
		assert_eq!(t.end, 31);

		let fm = "
/*
---
name: my-app--test
dialect: postgres
---
*/
		"
		.trim();
		let mut lexer = Lexer::new(&fm);

		let t = lexer.read_next_token().unwrap();
		assert_eq!(t.kind, TokenKind::Frontmatter);
		assert_eq!(t.start, 0);
		assert_eq!(t.end, 50);

		let fm = "
/*
-
---
dialect: postgres
---
*/
		"
		.trim();
		let mut lexer = Lexer::new(&fm);

		let t = lexer.read_next_token().unwrap();
		assert_eq!(t.kind, TokenKind::BlockComment);
		assert_eq!(t.start, 0);
		assert_eq!(t.end, 33);

		let fm = "
/*
--
---
dialect: postgres
---
*/
		"
		.trim();
		let mut lexer = Lexer::new(&fm);

		let t = lexer.read_next_token().unwrap();
		assert_eq!(t.kind, TokenKind::BlockComment);
		assert_eq!(t.start, 0);
		assert_eq!(t.end, 34);

		let fm = "
/*
---
dialect: postgres
---
-
*/
		"
		.trim();
		let mut lexer = Lexer::new(&fm);

		let t = lexer.read_next_token().unwrap();
		assert_eq!(t.kind, TokenKind::BlockComment);
		assert_eq!(t.start, 0);
		assert_eq!(t.end, 33);

		let fm = "
/*
---
dialect: postgres
---
--
*/
		"
		.trim();
		let mut lexer = Lexer::new(&fm);

		let t = lexer.read_next_token().unwrap();
		assert_eq!(t.kind, TokenKind::BlockComment);
		assert_eq!(t.start, 0);
		assert_eq!(t.end, 34);

		let fm = "
/*
2**2
---
dialect: postgres
---
*/
		"
		.trim();
		let mut lexer = Lexer::new(&fm);

		let t = lexer.read_next_token().unwrap();
		assert_eq!(t.kind, TokenKind::BlockComment);
		assert_eq!(t.start, 0);
		assert_eq!(t.end, 36);

		let fm = "
/*
---
dialect: postgres
---
2**2
*/
		"
		.trim();
		let mut lexer = Lexer::new(&fm);

		let t = lexer.read_next_token().unwrap();
		assert_eq!(t.kind, TokenKind::BlockComment);
		assert_eq!(t.start, 0);
		assert_eq!(t.end, 36);
	}

	#[test]
	fn test_cell_markers() {
		let markers = vec![
			("--%%", 0, 4),
			("--%% \n", 0, 4),
			("-- %%", 0, 5),
			("-- %% \n", 0, 5),
			("--  \t%%", 0, 7),
			("--  \t%% \n", 0, 7),
		];

		for marker in markers {
			let mut lexer = Lexer::new(marker.0);
			let t = lexer.read_next_token().unwrap();

			assert_eq!(t.kind, TokenKind::CellMarker);
			assert_eq!(t.start, marker.1);
			assert_eq!(t.end, marker.2);
		}
	}

	#[test]
	fn test_line_comments() {
		let comments = vec![
			("--%", 0, 3),
			("--  \t% \n abc", 0, 8),
			("--  \tabc %% \n xyz", 0, 13),
		];

		for comment in comments {
			let mut lexer = Lexer::new(comment.0);
			let t = lexer.read_next_token().unwrap();

			assert_eq!(t.kind, TokenKind::LineComment);
			assert_eq!(t.start, comment.1);
			assert_eq!(t.end, comment.2);
		}
	}

	#[test]
	fn test_cell_types() {
		let types = vec![
			("-- %% [typescript]", 5, 18),
			("-- %% [typescript] \n", 5, 18),
			("-- %% [  python  ]", 5, 18),
			("-- %% [  python  ] \n", 5, 18),
		];

		for t in types {
			let mut lexer = Lexer::new(t.0);

			let t1 = lexer.read_next_token().unwrap();
			assert_eq!(t1.kind, TokenKind::CellMarker);
			assert_eq!(t1.start, 0);
			assert_eq!(t1.end, 5);

			let t2 = lexer.read_next_token().unwrap();
			assert_eq!(t2.kind, TokenKind::CellType);
			assert_eq!(t2.start, t.1);
			assert_eq!(t2.end, t.2);
		}

		let types_err = vec![
			("-- %% [typescript", "expected ']'"),
			("-- %% [typescript {\"key\": \"value\"} \n", "expected ']'"),
			("-- %% ]typescript", "unexpected characters"),
		];

		for t in types_err {
			let mut lexer = Lexer::new(t.0);

			let t1 = lexer.read_next_token().unwrap();
			assert_eq!(t1.kind, TokenKind::CellMarker);
			assert_eq!(t1.start, 0);
			assert_eq!(t1.end, 5);

			let t2 = lexer.read_next_token();
			assert!(t2.is_err());
			assert!(t2.unwrap_err().to_string().contains(t.1));
		}
	}

	#[test]
	fn test_cell_configs() {
		let configs = vec![
			("-- %% {\"key\": \"value\"}", 5, 22),
			("-- %% {\"nested\": {\"key\": \"value\"}}", 5, 34),
		];

		for config in configs {
			let mut lexer = Lexer::new(config.0);
			let t1 = lexer.read_next_token().unwrap();
			assert_eq!(t1.kind, TokenKind::CellMarker);
			assert_eq!(t1.start, 0);
			assert_eq!(t1.end, 5);

			let t2 = lexer.read_next_token().unwrap();
			assert_eq!(t2.kind, TokenKind::CellConfig);
			assert_eq!(t2.start, config.1);
			assert_eq!(t2.end, config.2);
		}

		let configs_err = vec![
			("-- %% {\"key\": \"value\"", "unbalanced braces"),
			("-- %% {\"key\": \"value\" [typescript] \n", "unbalanced braces"),
			("-- %% {\"nested\": {\"key\": \"value\"}", "unbalanced braces"),
		];

		for config in configs_err {
			let mut lexer = Lexer::new(config.0);

			let t1 = lexer.read_next_token().unwrap();
			assert_eq!(t1.kind, TokenKind::CellMarker);
			assert_eq!(t1.start, 0);
			assert_eq!(t1.end, 5);

			let t2 = lexer.read_next_token();
			assert!(t2.is_err());
			assert!(t2.unwrap_err().to_string().contains(config.1));
		}
	}

	#[test]
	fn test_cell_content() {
		let content = vec![
			("select columns from users;", 0, 26),
			("select columns from products;\nselect columns from orders;", 0, 57),
		];

		for c in content {
			let mut lexer = Lexer::new(c.0);

			let t = lexer.read_next_token().unwrap();
			assert_eq!(t.kind, TokenKind::CellContent);
			assert_eq!(t.start, c.1);
			assert_eq!(t.end, c.2);
		}
	}

	#[test]
	fn test_source() {
		let source = "
-- %%
select columns from users;

-- %% [python]
select columns from products;

-- %% {\"key\": \"value\"}
select columns from orders;

-- %% [python] {\"key\": \"value\"}
select 2 / 1;

-- %% {\"key\": \"value\"} [python]
select 2 - 1;
		"
		.trim();
		let mut lexer = Lexer::new(&source);

		let tokens = vec![
			(TokenKind::CellMarker, 0, 5),
			(TokenKind::CellContent, 5, 34),
			//
			(TokenKind::CellMarker, 34, 39),
			(TokenKind::CellType, 39, 48),
			(TokenKind::CellContent, 48, 80),
			//
			(TokenKind::CellMarker, 80, 85),
			(TokenKind::CellConfig, 85, 102),
			(TokenKind::CellContent, 102, 132),
			//
			(TokenKind::CellMarker, 132, 137),
			(TokenKind::CellType, 137, 146),
			(TokenKind::CellConfig, 146, 163),
			(TokenKind::CellContent, 163, 179),
			//
			(TokenKind::CellMarker, 179, 184),
			(TokenKind::CellConfig, 184, 201),
			(TokenKind::CellType, 201, 210),
			(TokenKind::CellContent, 210, 224),
		];

		for tk in tokens {
			let t = lexer.read_next_token().unwrap();
			assert_eq!(t.kind, tk.0);
			assert_eq!(t.start, tk.1);
			assert_eq!(t.end, tk.2);
		}

		let t = lexer.read_next_token().unwrap();
		assert_eq!(t.kind, TokenKind::Eof);
	}
}
