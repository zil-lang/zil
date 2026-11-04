use super::*;

pub(super) fn boolean(expression: &Expr, context: &AttributeContext) -> Option<bool> {
    match value(expression, context)? {
        ConfigValue::Bool(value) => Some(value),
        _ => None,
    }
}
fn value(expression: &Expr, context: &AttributeContext) -> Option<ConfigValue> {
    if let Some(path) = path(expression) {
        return context.configuration.get(&path.join(".")).cloned();
    }
    match &expression.kind {
        ExprKind::Literal(Literal::Bool(v)) => Some(ConfigValue::Bool(*v)),
        ExprKind::Literal(Literal::String(v)) => string(v).map(ConfigValue::String),
        ExprKind::Literal(Literal::Integer(v)) => {
            let clean = v.replace('_', "");
            let (digits, radix) = if let Some(v) = clean.strip_prefix("0x") {
                (v, 16)
            } else if let Some(v) = clean.strip_prefix("0o") {
                (v, 8)
            } else if let Some(v) = clean.strip_prefix("0b") {
                (v, 2)
            } else {
                (clean.as_str(), 10)
            };
            i128::from_str_radix(digits, radix)
                .ok()
                .map(ConfigValue::Integer)
        }
        ExprKind::Group(e) => value(e, context),
        ExprKind::Unary {
            op: UnaryOp::Not,
            operand,
        } => boolean(operand, context).map(|v| ConfigValue::Bool(!v)),
        ExprKind::Unary {
            op: UnaryOp::Negate,
            operand,
        } => match value(operand, context)? {
            ConfigValue::Integer(v) => v.checked_neg().map(ConfigValue::Integer),
            _ => None,
        },
        ExprKind::Binary {
            op: BinaryOp::And,
            left,
            right,
        } => match (boolean(left, context), boolean(right, context)) {
            (Some(false), _) | (_, Some(false)) => Some(ConfigValue::Bool(false)),
            (Some(true), Some(true)) => Some(ConfigValue::Bool(true)),
            _ => None,
        },
        ExprKind::Binary {
            op: BinaryOp::Or,
            left,
            right,
        } => match (boolean(left, context), boolean(right, context)) {
            (Some(true), _) | (_, Some(true)) => Some(ConfigValue::Bool(true)),
            (Some(false), Some(false)) => Some(ConfigValue::Bool(false)),
            _ => None,
        },
        ExprKind::Binary { op, left, right } => {
            let left = value(left, context)?;
            let right = value(right, context)?;
            let ordering = match (&left, &right) {
                (ConfigValue::Integer(a), ConfigValue::Integer(b)) => a.partial_cmp(b),
                (ConfigValue::String(a), ConfigValue::String(b))
                    if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual) =>
                {
                    a.partial_cmp(b)
                }
                (ConfigValue::Bool(a), ConfigValue::Bool(b))
                    if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual) =>
                {
                    a.partial_cmp(b)
                }
                _ => return None,
            }?;
            let result = match op {
                BinaryOp::Equal => ordering.is_eq(),
                BinaryOp::NotEqual => !ordering.is_eq(),
                BinaryOp::Less => ordering.is_lt(),
                BinaryOp::LessEqual => !ordering.is_gt(),
                BinaryOp::Greater => ordering.is_gt(),
                BinaryOp::GreaterEqual => !ordering.is_lt(),
                _ => return None,
            };
            Some(ConfigValue::Bool(result))
        }
        // The parser does not invent the rest of the configuration language or
        // run arbitrary calls. Missing snapshots/operations are explicitly
        // deferred; they are never guessed true and never authorize execution.
        _ => None,
    }
}
fn string(spelling: &str) -> Option<String> {
    let inner = spelling.strip_prefix('"')?.strip_suffix('"')?;
    let mut chars = inner.chars();
    let mut result = String::new();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        result.push(match chars.next()? {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '0' => '\0',
            '\\' => '\\',
            '\'' => '\'',
            '"' => '"',
            'u' if chars.next()? == '{' => {
                let mut hex = String::new();
                loop {
                    let ch = chars.next()?;
                    if ch == '}' {
                        break;
                    }
                    hex.push(ch);
                }
                char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?
            }
            _ => return None,
        });
    }
    Some(result)
}
