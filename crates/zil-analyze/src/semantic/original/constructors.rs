//! Complete original resource constructors. Their type equations do not create
//! an initialized manager, a heap entry, a borrow or a cleanup certificate.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Wrapper {
    Managed,
    Store,
    Mutex,
    Registration,
    AsyncRegistration,
}
impl Wrapper {
    pub(in crate::semantic) fn ty(self, inner: Type) -> Type {
        match self {
            Self::Managed => Type::Managed(Box::new(inner)),
            Self::Store => Type::OwnedStore {
                element: Box::new(inner),
            },
            Self::Mutex => Type::Mutex {
                payload: Box::new(inner),
            },
            Self::Registration => Type::Registration {
                environment: Box::new(inner),
            },
            Self::AsyncRegistration => Type::AsyncRegistration {
                environment: Box::new(inner),
            },
        }
    }
}
impl Solver<'_, '_> {
    pub(super) fn wrapper(
        &mut self,
        frame: &Frame,
        wrapper: Wrapper,
        inner: &SemanticTypeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let inner = self.recipe(frame, inner, b, depth + 1)?;
        match wrapper {
            Wrapper::Store => Self::push(
                &mut self.needs,
                Need {
                    term: inner,
                    predicate: Predicate::StoreElement,
                },
                b,
            )?,
            Wrapper::Registration | Wrapper::AsyncRegistration => {
                Self::push(
                    &mut self.needs,
                    Need {
                        term: inner,
                        predicate: Predicate::Capability(SemanticBuiltinTrait::ClosedEnvironment),
                    },
                    b,
                )?;
                let signature = SemanticSignatureRecipe {
                    bound_lifetimes: 0,
                    invocation_lifetimes: 0,
                    parameters: vec![SemanticParameterRecipe {
                        ty: SemanticTypeRecipe::Atom(Type::I32),
                        mode: PassingMode::Infer,
                    }],
                    result: SemanticTypeRecipe::Atom(Type::I32),
                    result_mode: PassingMode::Infer,
                    call_kind: CallKind::Direct,
                };
                if let Kind::Rigid(owner, ordinal) = self.state.arena[self.type_root(inner, b)?] {
                    let (kind, available) = self.callable_premise(owner, ordinal, b)?;
                    crate::dyn_metadata::recipe(&available, b)?;
                    if !matches!(kind, CallableKind::Fn | CallableKind::FnMut)
                        || available != signature
                    {
                        return Err(fail(
                            "original registration environment lacks its complete scalar FnMut role",
                        ));
                    }
                } else {
                    self.callable_need(
                        frame,
                        inner,
                        CallableKind::FnMut,
                        &signature,
                        b,
                        depth + 1,
                    )?;
                }
            }
            Wrapper::Mutex => {
                if !matches!(
                    self.state.arena[self.type_root(inner, b)?],
                    Kind::Atom(Type::I32)
                ) {
                    return Err(unknown(
                        "original Mutex payload needs its complete sealed payload and synchronization scheme",
                    ));
                }
            }
            Wrapper::Managed => match &self.state.arena[self.type_root(inner, b)?] {
                Kind::Atom(Type::ByteBuffer)
                | Kind::Wrapped(
                    Wrapper::Store
                    | Wrapper::Mutex
                    | Wrapper::Registration
                    | Wrapper::AsyncRegistration,
                    _,
                ) => {}
                _ => {
                    return Err(unknown(
                        "original Managed payload needs its exact resource definition and Drop theorem",
                    ));
                }
            },
        }
        self.term(Kind::Wrapped(wrapper, inner), b)
    }
    pub(super) fn owned_sync(
        &self,
        payload: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        match &self.state.arena[self.type_root(payload, b)?] {
            Kind::Atom(Type::ByteBuffer) => Ok(true),
            Kind::Wrapped(Wrapper::Store, element) => {
                Ok(
                    self.satisfies(*element, Predicate::StoreElement, b, depth + 1)?
                        && self.satisfies(*element, Predicate::SyncDrop, b, depth + 1)?,
                )
            }
            Kind::Wrapped(Wrapper::Mutex, _) => Ok(true),
            Kind::Wrapped(Wrapper::Registration, environment) => self.satisfies(
                *environment,
                Predicate::Capability(SemanticBuiltinTrait::ClosedEnvironment),
                b,
                depth + 1,
            ),
            Kind::Wrapped(Wrapper::AsyncRegistration, _) => Ok(false),
            _ => Err(unknown(
                "original resource cleanup needs its actual original Drop and state relation",
            )),
        }
    }
}
