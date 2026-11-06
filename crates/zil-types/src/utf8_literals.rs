//! General runtime UTF8 literals. No library name or source span grants authority.
use super::*;

pub(super) fn ty() -> Type {
    Type::Address {
        mutable: false,
        inner: Box::new(Type::Slice {
            element: Box::new(Type::U8),
        }),
    }
}

impl Builder<'_, '_> {
    pub(super) fn utf8_literal(&mut self, spelling: &str, span: Span) -> R<(ValueId, Type)> {
        // Two complete decoding passes are prepaid before visiting the spelling.
        // The first computes the exact backing capacity; the second fills it.
        self.closures
            .snapshot(spelling.len().saturating_mul(2), span)?;
        let invalid = || {
            error(
                "E-UTF8-LITERAL",
                span,
                "invalid runtime UTF8 string literal",
            )
        };
        let mut length = 0usize;
        decode(spelling, |ch| length += ch.len_utf8()).ok_or_else(invalid)?;
        self.closures.snapshot(length.saturating_add(3), span)?;
        let mut bytes = Vec::with_capacity(length);
        decode(spelling, |ch| {
            let mut encoded = [0; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
        })
        .ok_or_else(invalid)?;
        let ty = ty();
        let result = self.value(InstructionKind::StaticUtf8 { bytes }, ty.clone(), span)?;
        Ok((result, ty))
    }
}

/// Validate even caller-built Core spelling. Work and output capacity are charged
/// by the caller; this visitor allocates no temporary escape strings.
fn decode(spelling: &str, mut emit: impl FnMut(char)) -> Option<()> {
    let inner = spelling.strip_prefix('"')?.strip_suffix('"')?;
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        let ch = if ch == '\\' {
            match chars.next()? {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '0' => '\0',
                '\\' => '\\',
                '\'' => '\'',
                '"' => '"',
                'u' => {
                    if chars.next()? != '{' {
                        return None;
                    }
                    let mut value = 0u32;
                    let mut count = 0;
                    loop {
                        let digit = chars.next()?;
                        if digit == '}' {
                            if count == 0 {
                                return None;
                            }
                            break;
                        }
                        if count == 6 || !digit.is_ascii_hexdigit() {
                            return None;
                        }
                        value = value * 16 + digit.to_digit(16)?;
                        count += 1;
                    }
                    char::from_u32(value)?
                }
                _ => return None,
            }
        } else {
            // The lexer never admits an unescaped quote or physical line break.
            // A hand-built Core node must obey the same contract.
            if matches!(ch, '"' | '\n' | '\r') {
                return None;
            }
            ch
        };
        emit(ch);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_decoding_preserves_utf8_empty_zero_and_escapes() {
        for (spelling, expected) in [
            ("\"\"", ""),
            ("\"按钮\"", "按钮"),
            (r#""a\0\u{1F642}\n\\\"\'\t\r""#, "a\0🙂\n\\\"'\t\r"),
        ] {
            let mut result = String::new();
            assert_eq!(decode(spelling, |ch| result.push(ch)), Some(()));
            assert_eq!(result, expected);
        }
        for spelling in [
            "plain",
            "\"a\"b\"",
            "\"a\n\"",
            r#""\u{}""#,
            r#""\u{D800}""#,
            r#""\u{110000}""#,
            r#""\x41""#,
            r#""\u{0000000}""#,
        ] {
            assert_eq!(decode(spelling, |_| {}), None, "{spelling}");
        }
    }
}
