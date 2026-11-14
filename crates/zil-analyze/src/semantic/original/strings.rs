//! Literal spelling is an original operation, not a claim that a type's
//! pointer is static. Each emitted literal is checked against these bytes.
use super::*;
pub(in crate::semantic) fn byte_type() -> Type {
    Type::Address {
        mutable: false,
        inner: Box::new(Type::Slice {
            element: Box::new(Type::U8),
        }),
    }
}
pub(in crate::semantic) fn is_byte_type(ty: &Type) -> bool {
    matches!(ty,Type::Address{mutable:false,inner} if matches!(inner.as_ref(),Type::Slice{element} if element.as_ref()==&Type::U8))
}
fn decode(text: &str, b: &mut Budget) -> CompileResult<Vec<u8>> {
    b.spend(text.len().saturating_add(1), WORK)?;
    let text = text
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .ok_or_else(|| fail("original string literal has invalid delimiters"))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve(text.len())
        .map_err(|_| resource("original string literal allocation failed"))?;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        let value = if ch == '\\' {
            match chars.next() {
                Some('n') => '\n',
                Some('r') => '\r',
                Some('t') => '\t',
                Some('0') => '\0',
                Some('\\') => '\\',
                Some('\'') => '\'',
                Some('"') => '"',
                Some('u') => {
                    if chars.next() != Some('{') {
                        return Err(fail(
                            "original string Unicode escape needs its scalar braces",
                        ));
                    }
                    let mut value = 0u32;
                    let mut digits = 0;
                    let mut closed = false;
                    for digit in chars.by_ref() {
                        if digit == '}' {
                            closed = true;
                            break;
                        }
                        let digit = digit
                            .to_digit(16)
                            .filter(|_| digit.is_ascii_hexdigit())
                            .ok_or_else(|| {
                                fail("original string Unicode escape contains a nonhex digit")
                            })?;
                        digits += 1;
                        if digits > 6 {
                            return Err(fail(
                                "original string Unicode escape exceeds scalar width",
                            ));
                        }
                        value = value * 16 + digit;
                    }
                    if digits == 0 || !closed {
                        return Err(fail("original string Unicode escape is incomplete"));
                    }
                    char::from_u32(value)
                        .ok_or_else(|| fail("original string escape is not a Unicode scalar"))?
                }
                _ => {
                    return Err(fail(
                        "original string literal contains an unsupported escape",
                    ));
                }
            }
        } else {
            if matches!(ch, '\n' | '\r' | '"') {
                return Err(fail(
                    "original string literal contains an unescaped delimiter",
                ));
            }
            ch
        };
        let mut encoded = [0u8; 4];
        bytes.extend_from_slice(value.encode_utf8(&mut encoded).as_bytes());
    }
    Ok(bytes)
}
impl Solver<'_, '_> {
    fn static_plain_result(&self, term: Term, b: &mut Budget, depth: usize) -> CompileResult<bool> {
        self.step(depth, b)?;
        match &self.state.arena[self.type_root(term, b)?] {
            Kind::Atom(ty) => Ok(ty.is_b0_scalar()
                || crate::dyn_dispatch::is_create_error(ty)
                || *ty == crate::generic_mutex::error_type()),
            Kind::Tuple(fields)
            | Kind::Nominal {
                arguments: _,
                fields,
                ..
            } => {
                // Inspect nominal actuals too: a physical empty phantom body
                // does not establish an empty persistent result domain.
                if let Kind::Nominal { arguments, .. } =
                    &self.state.arena[self.type_root(term, b)?]
                {
                    for argument in arguments {
                        if !self.static_plain_result(*argument, b, depth + 1)? {
                            return Ok(false);
                        }
                    }
                }
                for field in fields {
                    if !self.static_plain_result(*field, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Kind::ErrorUnion { error, result } => {
                Ok(self.static_plain_result(*error, b, depth + 1)?
                    && self.static_plain_result(*result, b, depth + 1)?)
            }
            Kind::Array { element, .. } => self.static_plain_result(*element, b, depth + 1),
            _ => Ok(false),
        }
    }
    pub(super) fn static_call_roles(
        &mut self,
        frame: &Frame,
        node: NodeId,
        arguments: &[SemanticOriginalExpr],
        result: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        if !self.static_plain_result(result, b, depth + 1)? {
            return Ok(());
        }
        b.lookup(self.state.calls.len(), WORK)?;
        let Some(call) = self.state.calls.get(&(frame.definition, node)) else {
            return Ok(());
        };
        b.spend(call.parameters.len(), WORK)?;
        let parameters = call.parameters.clone();
        let mut replacements = Vec::new();
        for (index, (argument, expected)) in arguments.iter().zip(parameters).enumerate() {
            b.lookup(frame.expressions.len(), WORK)?;
            let Some(actual) = frame.expressions.get(&argument.id).copied() else {
                continue;
            };
            if !self.static_byte_role(actual, expected, b, depth + 1)? {
                continue;
            }
            let Kind::Address { life, .. } = self.state.arena[self.type_root(expected, b)?] else {
                continue;
            };
            if matches!(
                self.state.arena[self.root(life, b)?],
                Kind::LifeVariable(None)
            ) {
                Self::push(&mut replacements, (index, actual), b)?;
            }
        }
        if let Some(call) = self.state.calls.get_mut(&(frame.definition, node)) {
            for (index, actual) in replacements {
                call.parameters[index] = actual;
            }
        }
        Ok(())
    }
    pub(super) fn string_literal(
        &mut self,
        frame: &Frame,
        node: NodeId,
        text: &str,
        b: &mut Budget,
    ) -> CompileResult<Term> {
        let bytes = decode(text, b)?;
        b.lookup(self.state.string_literals.len(), WORK)?;
        if self
            .state
            .string_literals
            .insert((frame.definition, node), bytes)
            .is_some()
        {
            return Err(fail("original literal node is repeated"));
        }
        self.term(Kind::Atom(byte_type()), b)
    }
    pub(super) fn static_byte_role(
        &mut self,
        actual: Term,
        expected: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        let actual = self.type_root(actual, b)?;
        let expected = self.type_root(expected, b)?;
        if !matches!(&self.state.arena[actual],Kind::Atom(ty) if is_byte_type(ty)) {
            return Ok(false);
        }
        let Kind::Address {
            mutable: false,
            inner,
            ..
        } = self.state.arena[expected]
        else {
            return Ok(false);
        };
        let Kind::Slice(element) = self.state.arena[self.type_root(inner, b)?] else {
            return Ok(false);
        };
        if !matches!(
            self.state.arena[self.type_root(element, b)?],
            Kind::Atom(Type::U8)
        ) {
            return Ok(false);
        }
        self.step(depth, b)?;
        // Static can satisfy this use's shorter outer role. Neither the
        // literal's type nor the original role's lifetime is rewritten.
        Ok(true)
    }
}
impl Checked {
    pub(in crate::semantic) fn string_literal_at<'a>(
        &'a self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<&'a [u8]>> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail("original literal has no body node directory"));
        };
        let mut found = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site != site {
                continue;
            }
            if let SemanticOriginalNode::Expression(node) = link.node {
                b.lookup(self.string_literals.len(), WORK)?;
                if let Some(bytes) = self.string_literals.get(&(context.definition, node)) {
                    if found.replace(bytes.as_slice()).is_some() {
                        return Err(fail("original literal has multiple node links"));
                    }
                }
            }
        }
        Ok(found)
    }
}
