use super::*;

pub(super) fn scalar(name: &str) -> Option<CtScalarTy> {
    Some(match name {
        "void" => CtScalarTy::Unit,
        "bool" => CtScalarTy::Bool,
        "u8" => CtScalarTy::U8,
        "u16" => CtScalarTy::U16,
        "i32" => CtScalarTy::I32,
        "i64" => CtScalarTy::I64,
        "f32" => CtScalarTy::F32,
        "f64" => CtScalarTy::F64,
        "u64" => CtScalarTy::U64,
        "usize" => CtScalarTy::Usize,
        _ => return None,
    })
}
pub(super) fn category(name: &str) -> Option<CtSyntaxCategory> {
    Some(match name {
        "Expr" => CtSyntaxCategory::Expr,
        "Type" => CtSyntaxCategory::Type,
        "StmtList" => CtSyntaxCategory::StmtList,
        "ItemList" => CtSyntaxCategory::ItemList,
        _ => return None,
    })
}
impl<'a> Compiler<'a> {
    pub(super) fn ct_only_function(&mut self, module: usize, function: &s::Function) -> R<bool> {
        let names = self.classification_names(&function.generics)?;
        let old = std::mem::replace(&mut self.classification_binders, names);
        let result = (|| {
            let mut seen = Vec::new();
            for parameter in &function.params {
                if self.ct_only_type(module, &parameter.ty, 0, &mut seen)? {
                    return Ok(true);
                }
            }
            match &function.return_type {
                Some(ty) => self.ct_only_type(module, ty, 0, &mut seen),
                None => Ok(false),
            }
        })();
        self.classification_binders = old;
        result
    }
    fn classification_names(&mut self, parameters: &[s::GenericParam]) -> R<Vec<String>> {
        let mut names = Vec::new();
        for parameter in parameters {
            self.budget.text(&parameter.name, parameter.span)?;
            self.budget.node(parameter.span)?;
            names.push(parameter.name.clone());
        }
        Ok(names)
    }
    pub(super) fn ct_only_type(
        &mut self,
        module: usize,
        ty: &s::TypeRef,
        depth: usize,
        seen: &mut Vec<(usize, Span)>,
    ) -> R<bool> {
        self.budget.depth(depth, ty.span)?;
        match &ty.kind {
            s::TypeKind::Named { path, args } => {
                for name in &self.classification_binders {
                    self.budget
                        .work(name.len().min(path[0].len()) + 1, ty.span)?;
                    if name == &path[0] {
                        return Ok(false);
                    }
                }
                if path.len() == 1
                    && matches!(
                        path[0].as_str(),
                        "Type"
                            | "Syntax"
                            | "Ident"
                            | "TokenTree"
                            | "TokenCursor"
                            | "Text"
                            | "ShapeInfo"
                            | "FieldInfo"
                            | "ImplInfo"
                            | "LayoutInfo"
                    )
                {
                    return Ok(true);
                }
                for argument in args {
                    if let s::GenericArg::Type(ty) = argument
                        && self.ct_only_type(module, ty, depth + 1, seen)?
                    {
                        return Ok(true);
                    }
                }
                if path.len() == 1
                    && (scalar(&path[0]).is_some()
                        || matches!(
                            path[0].as_str(),
                            "ByteBuffer" | "ForeignHandle" | "CWideString"
                        ))
                {
                    return Ok(false);
                }
                let (owner, item) = match self.named_item(module, path, ty.span) {
                    Ok(found) => found,
                    Err(errors)
                        if errors
                            .iter()
                            .any(|d| d.category == zil_base::ResultCategory::ResourceLimit) =>
                    {
                        return Err(errors);
                    }
                    Err(_) => return Ok(false),
                };
                self.budget.work(seen.len(), ty.span)?;
                if seen.contains(&(owner, item.span)) {
                    return Ok(false);
                }
                self.budget.node(ty.span)?;
                seen.push((owner, item.span));
                let parameters = match &item.kind {
                    s::ItemKind::Aggregate(a) => a.generics.as_slice(),
                    s::ItemKind::Enum(e) => e.generics.as_slice(),
                    s::ItemKind::TypeAlias(a) => a.generics.as_slice(),
                    _ => &[],
                };
                let names = self.classification_names(parameters)?;
                let old = std::mem::replace(&mut self.classification_binders, names);
                let result = match &item.kind {
                    s::ItemKind::TypeAlias(alias) => match &alias.value {
                        Some(ty) => self.ct_only_type(owner, ty, depth + 1, seen)?,
                        None => false,
                    },
                    s::ItemKind::Aggregate(aggregate) => {
                        let mut found = false;
                        for field in &aggregate.fields {
                            if self.ct_only_type(owner, &field.ty, depth + 1, seen)? {
                                found = true;
                                break;
                            }
                        }
                        found
                    }
                    s::ItemKind::Enum(enumeration) => {
                        let mut found = false;
                        for variant in &enumeration.variants {
                            match &variant.fields {
                                s::VariantFields::Unit => {}
                                s::VariantFields::Tuple(fields) => {
                                    for field in fields {
                                        if self.ct_only_type(owner, &field.ty, depth + 1, seen)? {
                                            found = true;
                                            break;
                                        }
                                    }
                                }
                                s::VariantFields::Record(fields) => {
                                    for field in fields {
                                        if self.ct_only_type(owner, &field.ty, depth + 1, seen)? {
                                            found = true;
                                            break;
                                        }
                                    }
                                }
                            }
                            if found {
                                break;
                            }
                        }
                        found
                    }
                    _ => false,
                };
                self.classification_binders = old;
                seen.pop();
                Ok(result)
            }
            s::TypeKind::Tuple(fields) | s::TypeKind::Union(fields) | s::TypeKind::Impl(fields) => {
                for field in fields {
                    if self.ct_only_type(module, field, depth + 1, seen)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            s::TypeKind::Reference { inner, .. }
            | s::TypeKind::Group(inner)
            | s::TypeKind::Optional(inner)
            | s::TypeKind::Array { element: inner, .. }
            | s::TypeKind::ForLifetime { inner, .. } => {
                self.ct_only_type(module, inner, depth + 1, seen)
            }
            s::TypeKind::Function { params, result }
            | s::TypeKind::ForeignFunction { params, result, .. } => {
                for ty in params {
                    if self.ct_only_type(module, ty, depth + 1, seen)? {
                        return Ok(true);
                    }
                }
                self.ct_only_type(module, result, depth + 1, seen)
            }
            s::TypeKind::Error { error, result } => {
                Ok(self.ct_only_type(module, error, depth + 1, seen)?
                    || self.ct_only_type(module, result, depth + 1, seen)?)
            }
            _ => Ok(false),
        }
    }

    pub(super) fn named_item(
        &mut self,
        current: usize,
        path: &[String],
        span: Span,
    ) -> R<(usize, &'a s::Item)> {
        let (module, name) = self.resolve_path(current, path, span)?;
        let mut found = None;
        for item in &self.modules[module].syntax.items {
            self.budget.work(1, span)?;
            let actual = match &item.kind {
                s::ItemKind::Aggregate(a) => Some(&a.name),
                s::ItemKind::Enum(e) => Some(&e.name),
                s::ItemKind::TypeAlias(a) => Some(&a.name),
                _ => None,
            };
            if actual == Some(&name) {
                if found.is_some() {
                    return Err(error(span, "duplicate CT type declaration"));
                }
                if current != module && !item.public {
                    return Err(error(span, "CT type is private to its defining module"));
                }
                found = Some(item);
            }
        }
        found
            .map(|item| (module, item))
            .ok_or_else(|| error(span, "unknown CT type name"))
    }
    pub(super) fn ty(&mut self, module: usize, ty: &s::TypeRef, depth: usize) -> R<CtTy> {
        self.budget.depth(depth, ty.span)?;
        Ok(match &ty.kind {
            s::TypeKind::Group(inner) => self.ty(module, inner, depth + 1)?,
            s::TypeKind::Tuple(fields) if fields.is_empty() => CtTy::Scalar(CtScalarTy::Unit),
            s::TypeKind::Tuple(fields) => {
                let mut out = Vec::new();
                for t in fields {
                    self.budget.node(t.span)?;
                    out.push(self.ty(module, t, depth + 1)?);
                }
                CtTy::Tuple(out)
            }
            s::TypeKind::Array { element, length } => {
                let element = Box::new(self.ty(module, element, depth + 1)?);
                match length {
                    None => CtTy::List(element),
                    Some(expr) => CtTy::Array {
                        element,
                        length: self.fixed_length(expr)?,
                    },
                }
            }
            s::TypeKind::Named { path, args } => {
                if path.len() == 1 {
                    if let Some(actual) = self.binding(&path[0], ty.span)? {
                        if !args.is_empty() {
                            return Err(error(ty.span, "CT type binder takes no type arguments"));
                        }
                        return Ok(actual);
                    }
                    if let Some(scalar) = scalar(&path[0]) {
                        if !args.is_empty() {
                            return Err(error(ty.span, "scalar CT type takes no arguments"));
                        }
                        return Ok(CtTy::Scalar(scalar));
                    }
                    let simple = match path[0].as_str() {
                        "Type" => Some(CtTy::Type),
                        "Ident" => Some(CtTy::Ident),
                        "TokenTree" => Some(CtTy::TokenTree),
                        "TokenCursor" => Some(CtTy::TokenCursor),
                        "Text" => Some(CtTy::Text),
                        "ShapeInfo" => Some(CtTy::ShapeInfo),
                        "FieldInfo" => Some(CtTy::FieldInfo),
                        "ImplInfo" => Some(CtTy::ImplInfo),
                        "LayoutInfo" => Some(CtTy::LayoutInfo),
                        _ => None,
                    };
                    if let Some(simple) = simple {
                        if !args.is_empty() {
                            return Err(error(ty.span, "atomic CT type takes no arguments"));
                        }
                        return Ok(simple);
                    }
                    if path[0] == "Syntax" {
                        if let [
                            s::GenericArg::Type(s::TypeRef {
                                kind: s::TypeKind::Named { path, args },
                                ..
                            }),
                        ] = args.as_slice()
                            && path.len() == 1
                            && args.is_empty()
                            && let Some(category) = category(&path[0])
                        {
                            return Ok(CtTy::Syntax(category));
                        }
                        return Err(error(
                            ty.span,
                            "Syntax needs exactly one fixed syntax category",
                        ));
                    }
                    if path[0] == "List" {
                        if let [s::GenericArg::Type(inner)] = args.as_slice() {
                            return Ok(CtTy::List(Box::new(self.ty(module, inner, depth + 1)?)));
                        }
                        return Err(error(ty.span, "CT List requires one element type"));
                    }
                }
                let (owner, item) = self.named_item(module, path, ty.span)?;
                match &item.kind {
                    s::ItemKind::TypeAlias(alias) => {
                        if !args.is_empty() || !alias.generics.is_empty() {
                            return Err(error(
                                ty.span,
                                "CT data alias requires a concrete nongeneric definition",
                            ));
                        }
                        let actual = alias.value.as_ref().ok_or_else(|| {
                            error(ty.span, "CT alias has no actual type definition")
                        })?;
                        self.enter_alias(owner, item.span, ty.span)?;
                        let old = std::mem::take(&mut self.bindings);
                        let result = self.ty(owner, actual, depth + 1);
                        self.bindings = old;
                        self.active_aliases.pop();
                        result?
                    }
                    s::ItemKind::Aggregate(a) => {
                        if a.kind != s::AggregateKind::Struct {
                            return Err(forbidden(
                                ty.span,
                                "CT data cannot contain runtime resources, packages or unresolved type parameters",
                            ));
                        }
                        let mut arguments = Vec::new();
                        let mut bindings = Vec::new();
                        if !a.generics.is_empty() {
                            let names = self.generic_names(&a.generics, &[], ty.span)?;
                            for arg in args {
                                let s::GenericArg::Type(actual) = arg else {
                                    return Err(error(
                                        ty.span,
                                        "CT records require ordinary Copy type arguments",
                                    ));
                                };
                                self.budget.node(actual.span)?;
                                arguments.push(self.ty(module, actual, depth + 1)?);
                            }
                            bindings = self.bind_arguments(names, &arguments, ty.span)?;
                        } else if !args.is_empty() {
                            return Err(error(
                                ty.span,
                                "nongeneric CT record takes no type arguments",
                            ));
                        }
                        for (id, (m, actual)) in self.record_sources.iter().enumerate() {
                            self.budget.work(1, ty.span)?;
                            if *m == owner && std::ptr::eq(*actual, a) {
                                for t in self.record_arguments[id].iter().chain(&arguments) {
                                    self.budget.ty(t, 0, ty.span)?;
                                }
                                if self.record_arguments[id] == arguments {
                                    return Ok(CtTy::Record(CtRecordSchemaId(id as u32)));
                                }
                            }
                        }
                        self.budget.node(ty.span)?;
                        let id = CtRecordSchemaId(self.records.len() as u32);
                        let name = self.symbol(owner, item.span)?;
                        let mut schema_arguments = Vec::new();
                        for argument in &arguments {
                            self.budget.copy_ty(argument, 0, ty.span)?;
                            schema_arguments.push(CtSchemaArgument::Type(argument.clone()));
                        }
                        self.record_sources.push((owner, a));
                        self.record_arguments.push(arguments);
                        self.records.push(CtRecordSchema {
                            name,
                            arguments: schema_arguments,
                            fields: Vec::new(),
                            origin: CtSchemaOrigin::UserPure,
                        });
                        let mut fields = Vec::new();
                        let old = std::mem::replace(&mut self.bindings, bindings);
                        for field in &a.fields {
                            self.budget.node(field.span)?;
                            if field.manages {
                                return Err(error(
                                    field.span,
                                    "CT data cannot hold a managed runtime resource",
                                ));
                            }
                            fields.push(self.ty(owner, &field.ty, depth + 1)?);
                        }
                        self.bindings = old;
                        self.records[id.0 as usize].fields = fields;
                        CtTy::Record(id)
                    }
                    s::ItemKind::Enum(e) => {
                        if !e.generics.is_empty() || !args.is_empty() {
                            return Err(error(ty.span, "CT enum requires a concrete instance"));
                        }
                        for (id, (m, actual)) in self.enum_sources.iter().enumerate() {
                            self.budget.work(1, ty.span)?;
                            if *m == owner && std::ptr::eq(*actual, e) {
                                return Ok(CtTy::Enum(CtEnumSchemaId(id as u32)));
                            }
                        }
                        self.budget.node(ty.span)?;
                        let id = CtEnumSchemaId(self.enums.len() as u32);
                        let name = self.symbol(owner, item.span)?;
                        self.enum_sources.push((owner, e));
                        self.enums.push(CtEnumSchema {
                            name,
                            arguments: Vec::new(),
                            variants: Vec::new(),
                            origin: CtSchemaOrigin::UserPure,
                        });
                        let mut variants = Vec::new();
                        for variant in &e.variants {
                            self.budget.node(variant.span)?;
                            let mut fields = Vec::new();
                            match &variant.fields {
                                s::VariantFields::Unit => {}
                                s::VariantFields::Tuple(fs) => {
                                    for field in fs {
                                        self.budget.node(field.span)?;
                                        if field.manages {
                                            return Err(error(
                                                field.span,
                                                "CT enum field cannot manage runtime resources",
                                            ));
                                        }
                                        fields.push(self.ty(owner, &field.ty, depth + 1)?);
                                    }
                                }
                                s::VariantFields::Record(fs) => {
                                    for field in fs {
                                        self.budget.node(field.span)?;
                                        if field.manages {
                                            return Err(error(
                                                field.span,
                                                "CT enum field cannot manage runtime resources",
                                            ));
                                        }
                                        fields.push(self.ty(owner, &field.ty, depth + 1)?);
                                    }
                                }
                            }
                            variants.push(fields);
                        }
                        self.enums[id.0 as usize].variants = variants;
                        CtTy::Enum(id)
                    }
                    _ => unreachable!(),
                }
            }
            _ => {
                return Err(error(
                    ty.span,
                    "CT-only data has no runtime address, resource, closure or foreign ABI",
                ));
            }
        })
    }
    fn fixed_length(&mut self, expression: &s::Expr) -> R<u32> {
        self.budget.work(1, expression.span)?;
        if let s::ExprKind::Literal(s::Literal::Integer(text)) = &expression.kind {
            self.budget.text(text, expression.span)?;
            let value = zil_types::parse_integer_literal(
                text,
                Some(&zil_ir::Type::I32),
                false,
                expression.span,
            )?;
            if let Constant::I32(n) = value
                && n >= 0
            {
                return Ok(n as u32);
            }
        }
        Err(error(
            expression.span,
            "CT array type requires a nonnegative fixed integer length",
        ))
    }
}
