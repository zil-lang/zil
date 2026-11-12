//! Joint nominal Type actuals. Physical names locate a produced item; actual
//! source lifetime classes come from this specialization, never from its name.
use super::*;
use crate::stored_types::{self, LexicalLife};

impl Engine<'_> {
    fn resource_payload_hint(&mut self,ty:&TypeRef)->R<bool>{
        let TypeKind::Named{path,..}=&grouped(ty).kind else{return Ok(false)};
        let name=path.join(".");self.budget.text(&name,ty.span)?;
        if matches!(name.as_str(),"Store"|"ByteBuffer"){return Ok(true)}
        let original=self.instance_types.get(&name).unwrap_or(ty);
        let TypeKind::Named{path,args}=&grouped(original).kind else{return Ok(false)};
        let name=path.join(".");self.budget.text(&name,ty.span)?;
        Ok(matches!(self.nominals.get(&name),Some(Item{kind:ItemKind::Aggregate(aggregate),..}) if aggregate.kind==AggregateKind::Resource && aggregate.generics.len()==args.len()))
    }
    pub(super) fn constructed_resource_hint(&mut self,ty:Option<TypeRef>)->R<Option<TypeRef>>{
        let Some(ty)=ty else{return Ok(None)};
        if !self.resource_payload_hint(&ty)?{return Ok(Some(ty))}
        self.budget.ty(&ty)?;
        Ok(Some(TypeRef{span:ty.span,kind:TypeKind::Named{path:vec!["Managed".into()],args:vec![GenericArg::Type(ty)]}}))
    }
    pub(super) fn given_resource_hint(&mut self,operand:&Expr,ty:TypeRef,env:&Env)->R<Option<TypeRef>>{
        // This is a proposal for an explicit transfer from an actual owning
        // lexical position. A description parameter cannot manufacture it.
        let owns=self.resource_place_hint(operand,env,0)?;
        if owns {self.constructed_resource_hint(Some(ty))}else{Ok(Some(ty))}
    }
    fn resource_place_hint(&mut self,place:&Expr,env:&Env,depth:usize)->R<bool>{
        self.budget.node(place.span,depth)?;
        match &place.kind {
            ExprKind::Name(name)=>{
                let Some(binding)=env.bindings.get(name) else{return Ok(false)};
                if binding.resource_owner{return Ok(true)}
                let Some(ty)=env.get(name).and_then(Option::as_ref) else{return Ok(false)};
                let TypeKind::Named{path,..}=&grouped(ty).kind else{return Ok(matches!(grouped(ty).kind,TypeKind::Tuple(_)|TypeKind::Array{..}))};
                let key=path.join(".");self.budget.text(&key,place.span)?;
                let original=self.instance_types.get(&key).unwrap_or(ty);
                let TypeKind::Named{path,..}=&grouped(original).kind else{return Ok(false)};
                Ok(matches!(self.nominals.get(&path.join(".")),Some(Item{kind:ItemKind::Aggregate(aggregate),..}) if aggregate.kind==AggregateKind::Package))
            },
            ExprKind::Member{object,..}=>{
                if !self.resource_place_hint(object,env,depth+1)?{return Ok(false)}
                Ok(!self.value_hint(object,env,depth+1,false)?.as_ref().is_some_and(|ty|matches!(grouped(ty).kind,TypeKind::Reference{..})))
            },
            ExprKind::Group(inner)=>self.resource_place_hint(inner,env,depth+1),
            ExprKind::Record{..}=>Ok(true),
            _=>Ok(false),
        }
    }
    pub(super) fn bind_resource_access(&mut self,pattern:&Pattern,value:&Expr,annotation:Option<&TypeRef>,env:&mut Env)->R<()> {
        let PatternKind::Name(name)=&pattern.kind else{return Ok(())};
        let fresh=matches!(value.kind,ExprKind::Record{..}|ExprKind::Unary{op:UnaryOp::Give,..});
        let explicit_value=annotation.is_some_and(|ty|matches!(&grouped(ty).kind,TypeKind::Named{path,args} if path.as_slice()==["Managed"] && args.len()==1));
        let resource_annotation=annotation.map(|ty|self.resource_payload_hint(ty)).transpose()?.unwrap_or(false);
        let binding=env.bindings.get_mut(name).expect("new lexical binding");
        if fresh && !explicit_value {
            binding.resource_owner|=resource_annotation;
            if let Some(TypeRef{kind:TypeKind::Named{path,args},..})=&binding.ty
                && path.as_slice()==["Managed"] && let [GenericArg::Type(payload)]=args.as_slice(){
                self.budget.ty(payload)?;let payload=payload.clone();binding.ty=Some(payload);binding.resource_owner=true;
            }
        }else if let ExprKind::Call{callee,..}=&value.kind && let ExprKind::Name(callee)=&callee.kind {
            binding.resource_owner|=callee=="store_new" || self.signatures.get(callee).is_some_and(|function|stores::result_mode(function).is_ok_and(|mode|matches!(mode,zil_ir::PassingMode::Create|zil_ir::PassingMode::Give)));
        }
        Ok(())
    }
    pub(super) fn stored_application(
        &mut self,
        name: &str,
        arguments: &[GenericArg],
        span: Span,
        depth: usize,
    ) -> R<bool> {
        if matches!(name,"Task"|"Managed") || name==options::TASK_THREAD_SCOPE {
            // The retained Task envelope is a builtin qualified carrier.
            // Its Context is checked by the Task header/recipe consumer.
            return Ok(false);
        }
        if self.stored_declaration(name, span)? {
            return Ok(true);
        }
        for argument in arguments {
            self.budget.node(span, depth)?;
            if let GenericArg::Type(ty) = argument
                && self.qualified_actual_syntax(ty, depth + 1)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn qualified_actual_syntax(&mut self, ty: &TypeRef, depth: usize) -> R<bool> {
        self.budget.node(ty.span, depth)?;
        match &ty.kind {
            TypeKind::Reference {
                lifetime: Some(_), ..
            } => Ok(true),
            TypeKind::Named { path, args } => {
                if path.len() == 1 {
                    for instance in &self.products.storage.instances {
                        self.budget.node(ty.span, depth)?;
                        if let zil_ir::Type::BorrowedStorage { storage, .. } = &instance.instance.ty
                            && let zil_ir::Type::Record { name, .. }
                            | zil_ir::Type::Enum { name, .. }
                            | zil_ir::Type::Package { name, .. } = storage.as_ref()
                        {
                            self.budget.text(name, ty.span)?;
                            self.budget.text(&path[0], ty.span)?;
                            if name == &path[0] {
                                return Ok(true);
                            }
                        }
                    }
                }
                for argument in args {
                    self.budget.node(ty.span, depth)?;
                    match argument {
                        GenericArg::Lifetime(_) => return Ok(true),
                        GenericArg::Type(inner)
                            if self.qualified_actual_syntax(inner, depth + 1)? =>
                        {
                            return Ok(true);
                        }
                        _ => {}
                    }
                }
                Ok(false)
            }
            TypeKind::Group(inner) | TypeKind::Optional(inner) => {
                self.qualified_actual_syntax(inner, depth + 1)
            }
            TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
                for field in fields {
                    if self.qualified_actual_syntax(field, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            TypeKind::Array { element, .. } => self.qualified_actual_syntax(element, depth + 1),
            TypeKind::Error { error, result } => Ok(self
                .qualified_actual_syntax(error, depth + 1)?
                || self.qualified_actual_syntax(result, depth + 1)?),
            _ => Ok(false),
        }
    }

    pub(super) fn qualified_type_actual(
        &mut self,
        ty: &mut TypeRef,
        depth: usize,
    ) -> R<Option<stored_types::QualifiedActual>> {
        if !self.qualified_actual_syntax(ty, depth)? {
            return Ok(None);
        }
        if let TypeKind::Group(inner) = &mut ty.kind {
            return self.qualified_type_actual(inner, depth + 1);
        }
        if let TypeKind::Reference {
            lifetime: Some(name),
            mutable,
            inner,
        } = &mut ty.kind
        {
            let class = self.source_life(name, ty.span)?;
            self.closed_stored_actual(inner, depth + 1)?;
            self.ty(inner, depth + 1)?;
            self.closed_stored_actual(inner, depth + 1)?;
            self.budget.text(name, ty.span)?;
            self.budget.ty(inner)?;
            self.budget
                .lineage_bytes(std::mem::size_of::<LexicalLife>(), ty.span)?;
            return Ok(Some(stored_types::QualifiedActual {
                physical: TypeRef { span:ty.span, kind:TypeKind::Reference { lifetime:Some(name.clone()),mutable:*mutable,inner:inner.clone() } },
                ty: zil_ir::Type::Address {
                    mutable: *mutable,
                    inner: Box::new(super::super::type_ref(inner)),
                },
                classes: vec![class],
            }));
        }
        if let TypeKind::Tuple(fields) = &mut ty.kind {
            let mut direct=BTreeMap::new();
            for (index,field) in fields.iter_mut().enumerate() {
                let path=vec![zil_ir::LifeTypeProjection::Place(zil_ir::PlaceProjection::Field(u32::try_from(index).map_err(|_|error(field.span,"tuple ordinal overflow"))?))];
                if let Some(actual)=self.qualified_type_actual(field,depth+1)? {
                    self.budget.lineage_bytes(std::mem::size_of::<stored_types::QualifiedActual>().saturating_add(std::mem::size_of::<zil_ir::LifeTypeProjection>()),field.span)?;
                    direct.insert(path,actual);
                } else {self.ty(field,depth+1)?;}
            }
            return Ok(Some(stored_types::compound_actual(ty,&direct,&mut self.budget)?));
        }
        let TypeKind::Named { path, args } = &mut ty.kind else {
            return Err(unsupported(
                ty.span,
                "compound qualified Type actual needs its full joint metadata mapping",
            ));
        };
        if path.len() != 1 || !self.templates.contains_key(&path[0]) || args.is_empty() {
            return Err(unsupported(
                ty.span,
                "qualified Type actual lost its source lifetime application",
            ));
        }
        let (name, classes) = self.stored_nominal(&path[0], args, ty.span, depth + 1)?;
        let mut actual = None;
        for instance in &self.products.storage.instances {
            self.budget.node(ty.span, depth)?;
            if let zil_ir::Type::BorrowedStorage { storage, .. } = &instance.instance.ty
                && let zil_ir::Type::Record { name: physical, .. }
                | zil_ir::Type::Enum { name: physical, .. }
                | zil_ir::Type::Package { name: physical, .. } = storage.as_ref()
            {
                self.budget.text(physical, ty.span)?;
                self.budget.text(&name, ty.span)?;
                if physical == &name {
                    stored_types::pay_type(
                        &instance.instance.ty,
                        &mut self.budget,
                        ty.span,
                        depth + 1,
                    )?;
                    actual = Some(instance.instance.ty.clone());
                }
            }
        }
        let actual =
            actual.ok_or_else(|| error(ty.span, "actual qualified Type instance absent"))?;
        self.budget.text(&name, ty.span)?;
        *path = vec![name.clone()];
        args.clear();
        self.budget.ty(ty)?;
        Ok(Some(stored_types::QualifiedActual {
            physical:ty.clone(),
            ty: actual,
            classes,
        }))
    }

    /// Match only fragments for which the parameter already carries an actual
    /// produced physical instance. Source generic variables remain semantic.
    /// This changes layout hints, never lifetime classes or origin evidence.
    pub(super) fn materialize_unification_actual(
        &mut self,
        pattern: &TypeRef,
        actual: &mut TypeRef,
        depth: usize,
    ) -> R<()> {
        self.budget.node(actual.span, depth)?;
        let pattern = grouped(pattern);
        if let TypeKind::Group(inner) = &mut actual.kind {
            return self.materialize_unification_actual(pattern, inner, depth + 1);
        }
        if matches!(&pattern.kind, TypeKind::Named { path, args } if path.len() == 1 && args.is_empty())
            && self.qualified_actual_syntax(pattern, depth + 1)?
        {
            return self.ty(actual, depth + 1);
        }
        match (&pattern.kind, &mut actual.kind) {
            (TypeKind::Reference { inner: p, .. }, TypeKind::Reference { inner: a, .. })
            | (TypeKind::Array { element: p, .. }, TypeKind::Array { element: a, .. })
            | (TypeKind::Optional(p), TypeKind::Optional(a)) => {
                self.materialize_unification_actual(p, a, depth + 1)?;
            }
            (
                TypeKind::Named { path: p, args: ps },
                TypeKind::Named {
                    path: a,
                    args: actuals,
                },
            ) if p == a && ps.len() == actuals.len() => {
                for (p, a) in ps.iter().zip(actuals) {
                    if let (GenericArg::Type(p), GenericArg::Type(a)) = (p, a) {
                        self.materialize_unification_actual(p, a, depth + 1)?;
                    }
                }
            }
            (TypeKind::Tuple(ps), TypeKind::Tuple(actuals))
            | (TypeKind::Union(ps), TypeKind::Union(actuals))
                if ps.len() == actuals.len() =>
            {
                for (p, a) in ps.iter().zip(actuals) {
                    self.materialize_unification_actual(p, a, depth + 1)?;
                }
            }
            (
                TypeKind::Error {
                    error: pe,
                    result: pr,
                },
                TypeKind::Error {
                    error: ae,
                    result: ar,
                },
            ) => {
                self.materialize_unification_actual(pe, ae, depth + 1)?;
                self.materialize_unification_actual(pr, ar, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn joint_class_key(
        &mut self,
        classes: &[LexicalLife],
        joint: &mut Vec<LexicalLife>,
        span: Span,
    ) -> R<String> {
        let mut key = String::new();
        for class in classes {
            self.budget
                .identity_work(joint.len().saturating_add(24), span)?;
            let index = if let Some(index) = joint.iter().position(|prior| prior == class) {
                index
            } else {
                self.budget
                    .lineage_bytes(std::mem::size_of::<LexicalLife>(), span)?;
                joint.push(*class);
                joint.len() - 1
            };
            key.push_str(&format!("class:{index};"));
        }
        Ok(key)
    }
}

impl Engine<'_> {
    pub(super) fn semantic_source_actual(
        &mut self,
        ty: &TypeRef,
        domain: &str,
    ) -> R<Option<SemanticActual>> {
        if self.life_scope.is_empty() || !self.qualified_actual_syntax(ty, 0)? {
            return Ok(None);
        }
        let mut classes = Vec::new();
        self.semantic_free_classes(ty, &mut Vec::new(), &mut classes, 0)?;
        self.budget.ty(ty)?;
        self.budget.text(domain, ty.span)?;
        Ok(Some(SemanticActual {
            application: Some(ty.clone()),
            classes,
            domain: domain.to_owned(),
        }))
    }
    fn semantic_free_classes(
        &mut self,
        ty: &TypeRef,
        bound: &mut Vec<String>,
        classes: &mut Vec<LexicalLife>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(ty.span, depth)?;
        match &ty.kind {
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                if let Some(name) = lifetime {
                    self.semantic_free_life(name, ty.span, bound, classes)?;
                }
                self.semantic_free_classes(inner, bound, classes, depth + 1)?;
            }
            TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
                for arg in args {
                    match arg {
                        GenericArg::Lifetime(name) => {
                            self.semantic_free_life(name, ty.span, bound, classes)?
                        }
                        GenericArg::Type(inner) => {
                            self.semantic_free_classes(inner, bound, classes, depth + 1)?
                        }
                        GenericArg::Const(_) => {}
                    }
                }
            }
            TypeKind::Group(inner)
            | TypeKind::Optional(inner)
            | TypeKind::Array { element: inner, .. } => {
                self.semantic_free_classes(inner, bound, classes, depth + 1)?
            }
            TypeKind::ForLifetime { lifetime, inner } => {
                let mark = bound.len();
                self.budget.text(lifetime, ty.span)?;
                bound.push(lifetime.clone());
                self.semantic_free_classes(inner, bound, classes, depth + 1)?;
                bound.truncate(mark);
            }
            TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
                for field in fields {
                    self.semantic_free_classes(field, bound, classes, depth + 1)?;
                }
            }
            TypeKind::Error { error, result } => {
                self.semantic_free_classes(error, bound, classes, depth + 1)?;
                self.semantic_free_classes(result, bound, classes, depth + 1)?;
            }
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for param in params {
                    self.semantic_free_classes(param, bound, classes, depth + 1)?;
                }
                self.semantic_free_classes(result, bound, classes, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn semantic_free_life(
        &mut self,
        name: &str,
        span: Span,
        bound: &[String],
        classes: &mut Vec<LexicalLife>,
    ) -> R<()> {
        for local in bound {
            self.budget.text(local, span)?;
            self.budget.text(name, span)?;
        }
        if !bound.iter().any(|local| local == name) {
            let class = self.source_life(name, span)?;
            self.budget
                .lineage_bytes(std::mem::size_of::<LexicalLife>(), span)?;
            classes.push(class);
        }
        Ok(())
    }
    pub(super) fn bind_semantic_source(&mut self, pattern: &Pattern, env: &mut Env) -> R<()> {
        match &pattern.kind {
            PatternKind::Name(name) => {
                let Some(source) = env
                    .bindings
                    .get(name)
                    .and_then(|binding| binding.ty.as_ref())
                else {
                    return Ok(());
                };
                let Some(actual) = self.semantic_source_actual(source, &env.domain)? else {
                    return Ok(());
                };
                self.budget.ty(source)?;
                let mut physical = source.clone();
                self.ty(&mut physical, 0)?;
                let binding = env.bindings.get_mut(name).expect("actual binding");
                binding.ty = Some(physical);
                binding.semantic = Some(actual);
            }
            PatternKind::Tuple(fields) => {
                for field in fields {
                    self.bind_semantic_source(field, env)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn alpha_value_actuals(
        &mut self,
        source: &BTreeMap<String, TypeRef>,
        parameters: &[&GenericParam],
        item: &Item,
        span: Span,
    ) -> R<(BTreeMap<String, TypeRef>, Vec<GenericParam>)> {
        let mut occupied = occupied_value_lives(item, &mut self.budget)?;
        let mut rebound = BTreeMap::new();
        let mut joint = Vec::new();
        let mut formals = Vec::new();
        for parameter in parameters {
            let Some(actual) = source.get(&parameter.name) else {
                return Err(error(
                    span,
                    "qualified actual lacks its declared type binder",
                ));
            };
            self.budget.ty(actual)?;
            let mut actual = actual.clone();
            let mut captured = Vec::new();
            self.capture_actual_type(&mut actual, &mut captured, 0)?;
            let mut names = BTreeMap::new();
            for (token, class) in captured {
                self.budget
                    .identity_work(joint.len().saturating_add(1), span)?;
                let ordinal = if let Some(ordinal) = joint.iter().position(|prior| prior == &class)
                {
                    ordinal
                } else {
                    self.budget.identity_work(64, span)?;
                    self.budget.lineage_bytes(
                        std::mem::size_of::<GenericParam>()
                            .saturating_add(std::mem::size_of::<LexicalLife>()),
                        span,
                    )?;
                    joint.push(class);
                    let mut nonce = joint.len() - 1;
                    let name = loop {
                        self.budget.identity_work(64, span)?;
                        let candidate = format!("$value_life_{nonce}");
                        self.budget.identity_work(
                            candidate.len().saturating_add(1).saturating_mul(
                                occupied.len().saturating_add(1).ilog2() as usize + 2,
                            ),
                            span,
                        )?;
                        if !occupied.contains(&candidate) {
                            break candidate;
                        }
                        nonce = nonce
                            .checked_add(1)
                            .ok_or_else(|| error(span, "Value life binder namespace exhausted"))?;
                    };
                    self.budget.text(&name, span)?;
                    occupied.insert(name.clone());
                    formals.push(GenericParam {
                        name,
                        kind: GenericParamKind::Lifetime,
                        span,
                    });
                    joint.len() - 1
                };
                self.budget.text(&token, span)?;
                self.budget.text(&formals[ordinal].name, span)?;
                names.insert(token, formals[ordinal].name.clone());
            }
            enums::source_field_lives(&mut actual, &names, &mut self.budget, 0)?;
            self.budget.text(&parameter.name, span)?;
            rebound.insert(parameter.name.clone(), actual);
        }
        Ok((rebound, formals))
    }
}

impl Engine<'_> {
    /// A generated helper's hidden life names belong to its own declaration.
    /// Rebuild this call's semantic result from its actual source substitution;
    /// the independently checked Core header still carries the alpha-renamed
    /// joint contract and establishes all real call permissions.
    pub(super) fn anonymous_call_result(
        &mut self,function:&Function,actuals:&[TypeRef],span:Span,
    )->R<Option<TypeRef>>{
        fn anonymous(ty:&TypeRef,in_tuple:bool)->bool{
            match &ty.kind{
                TypeKind::Group(inner)=>anonymous(inner,in_tuple),
                TypeKind::Tuple(fields)=>fields.iter().any(|field|anonymous(field,true)),
                TypeKind::Reference{lifetime:Some(_),..}=>in_tuple,
                TypeKind::Named{args,..}=>args.iter().any(|arg|matches!(arg,GenericArg::Type(inner) if anonymous(inner,false))),
                _=>false,
            }
        }
        if !self.stored_nominals{return Ok(None)}
        let mut selected=false;
        for actual in actuals{self.budget.ty(actual)?;selected|=anonymous(actual,false);}
        if !selected{return Ok(None)}
        let Some(result)=&function.return_type else{return Ok(None)};
        let mut substitution=BTreeMap::new();
        let mut actuals=actuals.iter();
        for parameter in &function.generics{
            if matches!(parameter.kind,GenericParamKind::Lifetime){continue}
            let actual=actuals.next().ok_or_else(||error(span,"anonymous call type argument arity differs"))?;
            self.budget.text(&parameter.name,span)?;
            self.budget.ty(actual)?;
            substitution.insert(parameter.name.clone(),actual.clone());
        }
        if actuals.next().is_some(){return Err(error(span,"anonymous call has excess type arguments"))}
        Ok(Some(enums::substituted(result,&substitution,&mut self.budget)?))
    }

    pub(super) fn source_unification_scope(
        &mut self,
        env: &Env,
        span: Span,
    ) -> R<Option<Function>> {
        if !self.stored_nominals || self.life_scope.is_empty() {
            return Ok(None);
        }
        self.budget.text(&env.domain, span)?;
        let mut generics = Vec::new();
        for name in &self.life_scope {
            self.budget.text(name, span)?;
            self.budget
                .lineage_bytes(std::mem::size_of::<GenericParam>(), span)?;
            generics.push(GenericParam {
                name: name.clone(),
                kind: GenericParamKind::Lifetime,
                span,
            });
        }
        Ok(Some(Function {
            name: env.domain.clone(),
            generics,
            params: vec![],
            return_type: None,
            where_clause: vec![],
            contract: vec![],
            body: FunctionBody::Declaration,
            span,
        }))
    }
}

pub(super) fn source_unification_key(
    ty: &TypeRef,
    source: Option<&Function>,
    budget: &mut Budget<'_>,
) -> R<String> {
    let Some(source) = source else {
        return type_key_bounded(ty, budget);
    };
    let mut key = check::source_class_key(ty, source, budget)?;
    fn life(
        name: &str,
        source: &Function,
        budget: &mut Budget<'_>,
        span: Span,
        key: &mut String,
    ) -> R<()> {
        let mut actual = None;
        for (ordinal, formal) in source.generics.iter().enumerate() {
            budget.node(span, 0)?;
            budget.text(&formal.name, span)?;
            budget.text(name, span)?;
            if matches!(formal.kind, GenericParamKind::Lifetime) && formal.name == name {
                actual = Some(ordinal);
            }
        }
        let ordinal = actual.ok_or_else(|| {
            error(
                span,
                "qualified actual class is outside its lexical unification domain",
            )
        })?;
        budget.identity_work(32, span)?;
        key.push_str(&format!("|class:{ordinal}"));
        Ok(())
    }
    fn visit(
        ty: &TypeRef,
        source: &Function,
        budget: &mut Budget<'_>,
        key: &mut String,
        depth: usize,
    ) -> R<()> {
        budget.node(ty.span, depth)?;
        match &ty.kind {
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                if let Some(name) = lifetime {
                    life(name, source, budget, ty.span, key)?;
                }
                visit(inner, source, budget, key, depth + 1)?;
            }
            TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
                for arg in args {
                    match arg {
                        GenericArg::Lifetime(name) => life(name, source, budget, ty.span, key)?,
                        GenericArg::Type(inner) => visit(inner, source, budget, key, depth + 1)?,
                        GenericArg::Const(_) => {}
                    }
                }
            }
            TypeKind::Group(inner)
            | TypeKind::Optional(inner)
            | TypeKind::Array { element: inner, .. } => {
                visit(inner, source, budget, key, depth + 1)?
            }
            TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
                for field in fields {
                    visit(field, source, budget, key, depth + 1)?;
                }
            }
            TypeKind::Error { error, result } => {
                visit(error, source, budget, key, depth + 1)?;
                visit(result, source, budget, key, depth + 1)?;
            }
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for param in params {
                    visit(param, source, budget, key, depth + 1)?;
                }
                visit(result, source, budget, key, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    visit(ty, source, budget, &mut key, 0)?;
    Ok(key)
}

fn occupied_value_lives(item: &Item, budget: &mut Budget<'_>) -> R<BTreeSet<String>> {
    use crate::ct_selection::Visitor;
    struct Occupied<'b, 'l> {
        budget: &'b mut Budget<'l>,
        names: BTreeSet<String>,
    }
    impl Visitor for Occupied<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|error| vec![error])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.budget.text(text, span).map_err(|error| vec![error])?;
            if text.strip_prefix("$value_life_").is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
            }) {
                self.budget
                    .lineage_bytes(
                        text.len().saturating_add(std::mem::size_of::<String>()),
                        span,
                    )
                    .map_err(|error| vec![error])?;
                self.names.insert(text.to_owned());
            }
            Ok(())
        }
    }
    let mut scan = Occupied {
        budget,
        names: BTreeSet::new(),
    };
    scan.item(item, 0).map_err(|mut errors| errors.remove(0))?;
    Ok(scan.names)
}
