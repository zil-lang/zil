//! Original dynamic type domains. These terms retain the complete mixed
//! application and Context; an interface ID or vtable is never a premise.
use super::*;
use crate::semantic::value::{self, Class, Value};

#[derive(Clone, PartialEq, Eq)]
enum Sequence { One(usize), All(usize, Vec<LifeTypeProjection>) }
#[derive(Clone, PartialEq, Eq)]
enum Port { One(usize, StoredBorrowMode, usize), From(usize, Vec<Sequence>) }
#[derive(Clone, PartialEq, Eq)]
enum Retain { One(Vec<LifeTypeProjection>, u32, usize), All(Vec<LifeTypeProjection>, Vec<Sequence>) }
#[derive(Clone, PartialEq, Eq)]
enum Nominal { One(Vec<LifeTypeProjection>, LifeDeclarationId, u32, usize), From(Vec<LifeTypeProjection>, usize, Vec<Sequence>) }
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct ContextShape {
    domain: Vec<Sequence>, ports: Vec<Port>, retained: Vec<Retain>, nominal: Vec<Nominal>,
    // Slots are canonicalized in the original domain, before actual images can
    // coincide. They continue to distinguish phantom and repeated actuals.
    leaves: Vec<Term>,
}
#[derive(Clone)]
pub(super) enum Context { Closed, Type(Term), Shape(ContextShape) }
#[derive(Clone)]
pub(super) enum AssociatedValue { Type(Term), Const { carrier: Term, value: Term } }
#[derive(Clone)]
pub(super) struct Associated {
    pub(super) declaration: LifeDeclarationId,
    pub(super) arguments: Vec<Term>,
    pub(super) member: u32,
    pub(super) value: AssociatedValue,
}
#[derive(Clone)]
pub(super) struct DynType {
    pub(super) owned: bool,
    pub(super) declaration: LifeDeclarationId,
    pub(super) arguments: Vec<Term>,
    pub(super) associated: Vec<Associated>,
    pub(super) context: Context,
}
impl AssociatedValue {
    fn terms(&self) -> impl Iterator<Item=Term> + '_ {
        let (first, second) = match self { Self::Type(term) => (*term, None), Self::Const { carrier, value } => (*carrier, Some(*value)) };
        std::iter::once(first).chain(second)
    }
}
impl DynType {
    pub(super) fn terms(&self) -> impl Iterator<Item=Term> + '_ {
        self.arguments.iter().copied()
            .chain(self.associated.iter().flat_map(|row| row.arguments.iter().copied().chain(row.value.terms())))
            .chain(match &self.context { Context::Closed => &[][..], Context::Type(term) => std::slice::from_ref(term), Context::Shape(shape) => shape.leaves.as_slice() }.iter().copied())
    }
    pub(super) fn work(&self) -> usize {
        let mut work = self.arguments.len().saturating_add(1);
        for row in &self.associated { work = work.saturating_add(row.arguments.len().saturating_add(3)); }
        if let Context::Shape(shape) = &self.context {
            fn sequence(rows: &[Sequence]) -> usize { rows.iter().fold(0usize, |n, row| n.saturating_add(match row { Sequence::One(_) => 1, Sequence::All(_, path) => path.len().saturating_add(1) })) }
            work = work.saturating_add(shape.leaves.len()).saturating_add(sequence(&shape.domain));
            for row in &shape.ports { work = work.saturating_add(match row { Port::One(..) => 1, Port::From(_, rows) => sequence(rows).saturating_add(1) }); }
            for row in &shape.retained { work = work.saturating_add(match row { Retain::One(path, ..) => path.len().saturating_add(1), Retain::All(path, rows) => path.len().saturating_add(sequence(rows)).saturating_add(1) }); }
            for row in &shape.nominal { work = work.saturating_add(match row { Nominal::One(path, ..) => path.len().saturating_add(1), Nominal::From(path, _, rows) => path.len().saturating_add(sequence(rows)).saturating_add(1) }); }
        }
        work
    }
    fn map_terms(&mut self, mut map: impl FnMut(Term) -> CompileResult<Term>) -> CompileResult<()> {
        for term in &mut self.arguments { *term = map(*term)?; }
        for row in &mut self.associated {
            for term in &mut row.arguments { *term = map(*term)?; }
            match &mut row.value { AssociatedValue::Type(term) => *term = map(*term)?, AssociatedValue::Const { carrier, value } => { *carrier = map(*carrier)?; *value = map(*value)?; } }
        }
        match &mut self.context { Context::Closed => {}, Context::Type(term) => *term = map(*term)?, Context::Shape(shape) => for term in &mut shape.leaves { *term = map(*term)?; } }
        Ok(())
    }
    fn same_structure(&self, other: &Self) -> bool {
        self.owned == other.owned && self.declaration == other.declaration
            && self.arguments.len() == other.arguments.len() && self.associated.len() == other.associated.len()
            && self.associated.iter().zip(&other.associated).all(|(a,z)| a.declaration == z.declaration && a.member == z.member && a.arguments.len() == z.arguments.len() && matches!((&a.value,&z.value), (AssociatedValue::Type(_),AssociatedValue::Type(_)) | (AssociatedValue::Const{..},AssociatedValue::Const{..})))
            && match (&self.context, &other.context) {
                (Context::Closed, Context::Closed) | (Context::Type(_), Context::Type(_)) => true,
                (Context::Shape(a), Context::Shape(z)) => a.domain == z.domain && a.ports == z.ports && a.retained == z.retained && a.nominal == z.nominal && a.leaves.len() == z.leaves.len(),
                _ => false,
            }
    }
}
fn frame(definition: SemanticDefinitionId, formals: Vec<Term>) -> Frame {
    Frame { definition, formals, parameters: Vec::new(), result: 0, locals: Vec::new(), inherited: 0, captured: BTreeMap::new(), alive: true, environment: Vec::new(), expressions: BTreeMap::new(), bindings: BTreeMap::new() }
}
impl Solver<'_, '_> {
    fn dyn_shape_leaf(&self, leaves: &mut Vec<Term>, term: Term, b: &mut Budget, depth: usize) -> CompileResult<usize> {
        for (slot, previous) in leaves.iter().enumerate() {
            if self.same_original(*previous, term, b, depth + 1)? { return Ok(slot); }
        }
        let slot = leaves.len(); Self::push(leaves, term, b)?; Ok(slot)
    }
    fn dyn_sequence(&mut self, frame: &Frame, source: &SemanticClassSequenceRecipe, leaves: &mut Vec<Term>, b: &mut Budget, depth: usize) -> CompileResult<Vec<Sequence>> {
        self.step(depth,b)?; let mut rows = Vec::new();
        for row in &source.parts {
            let row = match row {
                SemanticClassSequencePart::One(life) => { let term = self.life(frame,life,b,depth+1)?; Sequence::One(self.dyn_shape_leaf(leaves,term,b,depth+1)?) },
                SemanticClassSequencePart::AllClasses { ty, path } => { let term = self.recipe(frame,ty,b,depth+1)?; b.spend(path.len(),WORK)?; Sequence::All(self.dyn_shape_leaf(leaves,term,b,depth+1)?,path.clone()) },
            }; Self::push(&mut rows,row,b)?;
        } Ok(rows)
    }
    fn dyn_shape(&mut self, frame: &Frame, source: &SemanticRetainedShapeRecipe, b: &mut Budget, depth: usize) -> CompileResult<ContextShape> {
        self.step(depth,b)?; let mut shape = ContextShape::default();
        shape.domain = self.dyn_sequence(frame,&source.domain,&mut shape.leaves,b,depth+1)?;
        for row in &source.ports {
            let row = match row {
                SemanticPortPart::One { leaf, mode, class } => { let leaf = self.recipe(frame,leaf,b,depth+1)?; let class = self.life(frame,class,b,depth+1)?; Port::One(self.dyn_shape_leaf(&mut shape.leaves,leaf,b,depth+1)?,*mode,self.dyn_shape_leaf(&mut shape.leaves,class,b,depth+1)?) },
                SemanticPortPart::FromType { ty, classes } => { let ty=self.recipe(frame,ty,b,depth+1)?; let classes=self.dyn_sequence(frame,classes,&mut shape.leaves,b,depth+1)?; Port::From(self.dyn_shape_leaf(&mut shape.leaves,ty,b,depth+1)?,classes) },
            }; Self::push(&mut shape.ports,row,b)?;
        }
        for row in &source.retained {
            let row=match row {
                SemanticRetainedPart::One { path, child_class, parent } => { b.spend(path.len(),WORK)?; let term=self.life(frame,parent,b,depth+1)?; Retain::One(path.clone(),*child_class,self.dyn_shape_leaf(&mut shape.leaves,term,b,depth+1)?) },
                SemanticRetainedPart::All { path, parents } => { b.spend(path.len(),WORK)?; Retain::All(path.clone(),self.dyn_sequence(frame,parents,&mut shape.leaves,b,depth+1)?) },
            }; Self::push(&mut shape.retained,row,b)?;
        }
        for row in &source.nominal {
            let row=match row {
                SemanticNominalPart::One { path, declaration, argument, class } => { b.spend(path.len(),WORK)?; let term=self.life(frame,class,b,depth+1)?; Nominal::One(path.clone(),*declaration,*argument,self.dyn_shape_leaf(&mut shape.leaves,term,b,depth+1)?) },
                SemanticNominalPart::FromType { path, ty, classes } => { b.spend(path.len(),WORK)?; let term=self.recipe(frame,ty,b,depth+1)?; let classes=self.dyn_sequence(frame,classes,&mut shape.leaves,b,depth+1)?; Nominal::From(path.clone(),self.dyn_shape_leaf(&mut shape.leaves,term,b,depth+1)?,classes) },
            }; Self::push(&mut shape.nominal,row,b)?;
        } Ok(shape)
    }
    pub(super) fn dyn_type(&mut self, original: &Frame, owned: bool, recipe: &SemanticDynInterfaceRecipe, b: &mut Budget, depth: usize) -> CompileResult<Term> {
        self.step(depth,b)?;
        let arguments=self.trait_arguments(original,&recipe.application,b,depth+1)?;
        let context=match &recipe.context { SemanticDynContextRecipe::Closed => Context::Closed, SemanticDynContextRecipe::Type(ty) => Context::Type(self.recipe(original,ty,b,depth+1)?), SemanticDynContextRecipe::Shape(shape) => Context::Shape(self.dyn_shape(original,shape,b,depth+1)?) };
        let mut associated=Vec::<Associated>::new();
        for binding in &recipe.associated {
            let actuals=self.trait_arguments(original,&binding.application,b,depth+1)?;
            if !self.dyn_super_application(recipe.application.declaration,&arguments,binding.application.declaration,&actuals,b,depth+1)? { return Err(fail("original Dyn associated binding selects no full inherited application")); }
            let raw=self.inventory.original_dyn(binding.application.declaration,b)?;
            let member=raw.members.get(binding.member as usize).ok_or_else(||fail("original Dyn associated binding selects no member"))?;
            let value=match (&member.kind,&binding.value) {
                (DynMemberKind::Type,SemanticAssociatedRecipe::Type(ty)) => AssociatedValue::Type(self.recipe(original,ty,b,depth+1)?),
                (DynMemberKind::Constant,SemanticAssociatedRecipe::Const { carrier, value }) => {
                    let carrier=self.recipe(original,carrier,b,depth+1)?;
                    let scope=declaration_scope(self.program,binding.application.declaration,self.inventory,b)?;
                    let target=frame(scope,actuals.clone());
                    let row=raw.associated_definitions.as_ref().and_then(|rows|rows.iter().find(|row|row.member==binding.member)).ok_or_else(||unknown("original Dyn associated constant omits its declaration carrier"))?;
                    let raw_carrier=row.carrier.as_ref().ok_or_else(||fail("original Dyn associated constant omits its carrier"))?;
                    let expected=self.raw_dyn_type(&target,raw_carrier,None,b,depth+1)?;
                    self.unify(carrier,expected,depth+1,b)?;
                    AssociatedValue::Const { carrier, value:self.constant(original,value,carrier,b,depth+1)? }
                },
                _ => return Err(fail("original Dyn associated binding changes its declared member kind")),
            };
            for old in &associated {
                if old.declaration==binding.application.declaration && old.member==binding.member && self.dyn_same_rows(&old.arguments,&actuals,b,depth+1)? { return Err(fail("original Dyn repeats an associated equation")); }
            }
            Self::push(&mut associated,Associated { declaration:binding.application.declaration,arguments:actuals,member:binding.member,value },b)?;
        }
        self.term(Kind::Dyn(DynType { owned,declaration:recipe.application.declaration,arguments,associated,context }),b)
    }
    pub(super) fn dyn_unify(&mut self,left:&DynType,right:&DynType,b:&mut Budget,depth:usize)->CompileResult<()> {
        self.step(depth,b)?; b.spend(left.work().saturating_add(right.work()),WORK)?;
        if !left.same_structure(right) { return Err(fail("original Dyn equation changes its full application or Context shape")); }
        for (a,z) in left.terms().zip(right.terms()) { self.unify(a,z,depth+1,b)?; } Ok(())
    }
    pub(super) fn dyn_same(&self,left:&DynType,right:&DynType,b:&mut Budget,depth:usize)->CompileResult<bool> {
        self.step(depth,b)?; b.spend(left.work().saturating_add(right.work()),WORK)?;
        if !left.same_structure(right) { return Ok(false); }
        for (a,z) in left.terms().zip(right.terms()) { if !self.same_original(a,z,b,depth+1)? { return Ok(false); } } Ok(true)
    }
    pub(super) fn dyn_instantiate(&mut self,mut ty:DynType,owner:SemanticDefinitionId,actuals:&[Term],b:&mut Budget,depth:usize)->CompileResult<Term> {
        self.step(depth,b)?; ty.map_terms(|term|self.instantiate(term,owner,actuals,b,depth+1))?; self.term(Kind::Dyn(ty),b)
    }
    pub(super) fn dyn_open_invocations(&mut self,mut ty:DynType,owner:SemanticDefinitionId,actuals:&[Term],b:&mut Budget,depth:usize)->CompileResult<Term> {
        self.step(depth,b)?; ty.map_terms(|term|self.open_invocations(term,owner,actuals,b,depth+1))?; self.term(Kind::Dyn(ty),b)
    }
    fn dyn_super_application(&mut self,source:LifeDeclarationId,arguments:&[Term],target:LifeDeclarationId,expected:&[Term],b:&mut Budget,depth:usize)->CompileResult<bool> {
        self.step(depth,b)?;
        if source==target && self.dyn_same_rows(arguments,expected,b,depth+1)? { return Ok(true); }
        let raw=self.inventory.original_dyn(source,b)?;
        let edges=raw.direct_supertraits.as_ref().ok_or_else(||unknown("original Dyn supertrait application inventory is unknown"))?;
        let scope=declaration_scope(self.program,source,self.inventory,b)?;
        b.spend(arguments.len(),WORK)?; let context=frame(scope,arguments.to_vec());
        for edge in edges {
            let actuals=self.raw_dyn_application(&context,edge,b,depth+1)?;
            if self.dyn_super_application(edge.declaration,&actuals,target,expected,b,depth+1)? { return Ok(true); }
        } Ok(false)
    }
}

impl Solver<'_, '_> {
    fn dyn_same_rows(&self, left:&[Term], right:&[Term], b:&mut Budget, depth:usize)->CompileResult<bool> {
        self.step(depth,b)?;
        if left.len()!=right.len() { return Ok(false); }
        for (a,z) in left.iter().zip(right) { if !self.same_original(*a,*z,b,depth+1)? { return Ok(false); } }
        Ok(true)
    }
    fn dyn_pattern_scope(&self, declaration:LifeDeclarationId, b:&mut Budget)->CompileResult<SemanticDefinitionId> {
        let mut found=None;
        for row in self.inventory.definitions.values() {
            b.spend(1,WORK)?;
            if matches!(row.body,SemanticDefinitionBody::Nominal { declaration:id,.. } if id==declaration) && found.replace(row.id).is_some() { return Err(fail("original raw nominal has competing complete declarations")); }
        }
        if let Some(scope)=found { return Ok(scope); }
        declaration_scope(self.program,declaration,self.inventory,b)
    }
    fn dyn_raw_formal(&self, context:&Frame, ordinal:u32, method:bool, kind:u8, b:&mut Budget)->CompileResult<Term> {
        let declaration=self.inventory.definition(context.definition,b)?;
        let target=if !method && declaration.kind!=SemanticDefinitionKind::Scope {
            let parent=declaration.parent.ok_or_else(||fail("original raw declaration parameter has no lexical declaration"))?;
            self.state.frames.get(&parent).ok_or_else(||fail("original raw declaration parameter has no checked parent frame"))?
        } else { context };
        let row=self.inventory.definition(target.definition,b)?;
        if method && !matches!(row.body,SemanticDefinitionBody::FunctionWithBody { .. }) { return Err(fail("original raw method parameter has no method declaration")); }
        if !matches!((kind,row.parameters.get(ordinal as usize)),(0,Some(SemanticFormal::Type { .. })) | (1,Some(SemanticFormal::Lifetime)) | (2,Some(SemanticFormal::Const { .. }))) { return Err(fail("original raw parameter changes its full ordinal or mixed kind")); }
        target.formals.get(ordinal as usize).copied().ok_or_else(||fail("original raw full-ordinal actual is absent"))
    }
    pub(super) fn raw_dyn_life(&mut self, context:&Frame, recipe:&DynLifePattern, b:&mut Budget, depth:usize)->CompileResult<Term> {
        self.step(depth,b)?;
        match recipe {
            DynLifePattern::DeclarationParameter(index)=>self.dyn_raw_formal(context,*index,false,1,b),
            DynLifePattern::MethodParameter(index)=>self.dyn_raw_formal(context,*index,true,1,b),
            DynLifePattern::Invocation(index)=>self.life(context,&SemanticLifeRecipe::Invocation(*index),b,depth+1),
            DynLifePattern::ImplicitDeclaration(index)=>self.life(context,&SemanticLifeRecipe::Implicit { scope:SemanticScopeRef::Ancestor(0),ordinal:*index },b,depth+1),
            DynLifePattern::Bound { .. }=>Err(unknown("original raw quantified lifetime requires its full bound role frame")),
        }
    }
    pub(super) fn raw_dyn_application(&mut self, context:&Frame, recipe:&DynSelfPattern, b:&mut Budget, depth:usize)->CompileResult<Vec<Term>> {
        self.step(depth,b)?;
        let scope=self.dyn_pattern_scope(recipe.declaration,b)?;
        let declaration=self.inventory.definition(scope,b)?;
        if declaration.parameters.len()!=recipe.arguments.len() { return Err(fail("original raw application changes its complete mixed arity")); }
        let mut arguments=Vec::new();
        for (formal,actual) in declaration.parameters.iter().zip(&recipe.arguments) {
            let term=match (formal,actual) {
                (SemanticFormal::Type { .. },DynPatternActual::Type(ty))=>self.raw_dyn_type(context,ty,None,b,depth+1)?,
                (SemanticFormal::Lifetime,DynPatternActual::Life(life))=>self.raw_dyn_life(context,life,b,depth+1)?,
                (SemanticFormal::Const { carrier },DynPatternActual::Const(constant))=>{
                    b.spend(arguments.len(),WORK)?;
                    let target=frame(scope,arguments.clone());
                    let carrier=self.recipe(&target,carrier,b,depth+1)?;
                    self.raw_dyn_constant(context,constant,carrier,b,depth+1)?
                },
                _=>return Err(fail("original raw application changes its full Type/Const/Life actual kind")),
            }; Self::push(&mut arguments,term,b)?;
        }
        Ok(arguments)
    }
    pub(super) fn raw_dyn_constant(&mut self, context:&Frame, recipe:&DynConstPattern, carrier:Term, b:&mut Budget, depth:usize)->CompileResult<Term> {
        self.step(depth,b)?;
        match recipe {
            DynConstPattern::Value(value)=>self.constant(context,&SemanticConstRecipe::Value(value.clone()),carrier,b,depth+1),
            DynConstPattern::IntegerLiteral(value)=>self.constant(context,&SemanticConstRecipe::IntegerLiteral(value.clone()),carrier,b,depth+1),
            DynConstPattern::FloatLiteral(value)=>self.constant(context,&SemanticConstRecipe::FloatLiteral(value.clone()),carrier,b,depth+1),
            DynConstPattern::Parameter(ordinal) | DynConstPattern::MethodParameter(ordinal)=>{
                let term=self.dyn_raw_formal(context,*ordinal,matches!(recipe,DynConstPattern::MethodParameter(_)),2,b)?;
                let expected=match &self.state.arena[self.root(term,b)?] { Kind::ConstRigid { carrier,.. }=>*carrier, Kind::ConstValue(value)=>self.atom(value.ty(),b)?, _=>return Err(fail("original raw constant parameter has no constant term")) };
                self.unify(carrier,expected,depth+1,b)?; Ok(term)
            },
            // Symbolic arithmetic/projection must first exist in the original
            // constant algebra. Raw structure and an actual folded result do
            // not prove this expression for every original actual.
            DynConstPattern::Unary { .. } | DynConstPattern::Binary { .. } | DynConstPattern::Convert { .. } | DynConstPattern::AssociatedProjection { .. }=>Err(unknown("original raw constant expression requires its complete symbolic carrier-aware algebra")),
        }
    }
    pub(super) fn raw_dyn_type(&mut self, context:&Frame, recipe:&DynTypePattern, receiver:Option<Term>, b:&mut Budget, depth:usize)->CompileResult<Term> {
        self.step(depth,b)?;
        match recipe {
            DynTypePattern::SelfType=>receiver.ok_or_else(||fail("original raw Self cannot establish its own receiver")),
            DynTypePattern::Parameter(ordinal)=>self.dyn_raw_formal(context,*ordinal,false,0,b),
            DynTypePattern::MethodParameter(ordinal)=>self.dyn_raw_formal(context,*ordinal,true,0,b),
            DynTypePattern::Concrete(ty)=>self.recipe(context,&SemanticTypeRecipe::Atom(ty.clone()),b,depth+1),
            DynTypePattern::Semantic { scope,ty }=>{
                if *scope==context.definition { return self.recipe(context,ty,b,depth+1); }
                let mut owner=self.inventory.definition(context.definition,b)?.parent;
                for _ in 0..=b.partial_depth {
                    b.spend(1,WORK)?;
                    let Some(id)=owner else { return Err(fail("original raw semantic type changes its exact lexical owner")); };
                    if id==*scope {
                        let parent=self.state.frames.get(scope).ok_or_else(||fail("original raw semantic type has no checked lexical frame"))?;
                        b.spend(parent.formals.len(),WORK)?;
                        let parent=frame(*scope,parent.formals.clone());
                        return self.recipe(&parent,ty,b,depth+1);
                    }
                    owner=self.inventory.definition(id,b)?.parent;
                }
                Err(resource("original raw semantic owner exceeds its lexical depth"))
            },
            DynTypePattern::Nominal(application)=>{
                let definition=self.dyn_pattern_scope(application.declaration,b)?;
                let arguments=self.raw_dyn_application(context,application,b,depth+1)?;
                self.application(definition,arguments,b,depth+1)
            },
            DynTypePattern::AssociatedProjection { receiver:subject,application,member }=>{
                let subject=self.raw_dyn_type(context,subject,receiver,b,depth+1)?;
                let arguments=self.raw_dyn_application(context,application,b,depth+1)?;
                self.associated_type(context,subject,application.declaration,arguments,*member,b,depth+1)
            },
            DynTypePattern::Address { mutable,lifetime,inner }=>{
                let life=self.raw_dyn_life(context,lifetime.as_ref().ok_or_else(||unknown("original raw address omits its actual lifetime occurrence"))?,b,depth+1)?;
                let inner=self.raw_dyn_type(context,inner,receiver,b,depth+1)?;
                self.term(Kind::Address { mutable:*mutable,life,inner },b)
            },
            DynTypePattern::Desc { lifetime,inner }=>{
                let life=self.raw_dyn_life(context,lifetime.as_ref().ok_or_else(||unknown("original raw Desc omits its actual lifetime occurrence"))?,b,depth+1)?;
                let inner=self.raw_dyn_type(context,inner,receiver,b,depth+1)?;
                self.term(Kind::Desc { life,inner },b)
            },
            DynTypePattern::Tuple(fields)=>{
                let mut terms=Vec::new(); for field in fields { let term=self.raw_dyn_type(context,field,receiver,b,depth+1)?; Self::push(&mut terms,term,b)?; }
                if terms.is_empty() { self.atom(Type::Unit,b) } else { self.term(Kind::Tuple(terms),b) }
            },
            DynTypePattern::Array { element,length }=>{
                let element=self.raw_dyn_type(context,element,receiver,b,depth+1)?; let carrier=self.atom(Type::Usize,b)?;
                let length=self.raw_dyn_constant(context,length,carrier,b,depth+1)?; self.term(Kind::Array { element,length },b)
            },
            DynTypePattern::Slice(inner)=>{ let inner=self.raw_dyn_type(context,inner,receiver,b,depth+1)?; self.term(Kind::Slice(inner),b) },
            DynTypePattern::Managed(inner)=>{ let inner=self.raw_dyn_type(context,inner,receiver,b,depth+1)?; self.term(Kind::Wrapped(Wrapper::Managed,inner),b) },
            DynTypePattern::OwnedStore(inner)=>{
                let inner=self.raw_dyn_type(context,inner,receiver,b,depth+1)?;
                Self::push(&mut self.needs,Need { term:inner,predicate:Predicate::StoreElement },b)?;
                let store=self.term(Kind::Wrapped(Wrapper::Store,inner),b)?; self.term(Kind::Wrapped(Wrapper::Managed,store),b)
            },
            DynTypePattern::ErrorUnion { error,result }=>{
                let error=self.raw_dyn_type(context,error,receiver,b,depth+1)?; let result=self.raw_dyn_type(context,result,receiver,b,depth+1)?;
                self.term(Kind::ErrorUnion { error,result },b)
            },
        }
    }
}
