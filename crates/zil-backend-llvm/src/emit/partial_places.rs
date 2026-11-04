//! Actual inline addresses and per-local remaining leaf flags. These recipes
//! are rebuilt from concrete verified types, never supplied by a candidate.
use super::*;
use zil_ir::PlaceProjection;
#[derive(Clone)]
pub(super) struct Leaf {
    path: Vec<PlaceProjection>,
    symbol: Option<String>,
}
pub(super) fn leaves(ty: &Type, resources: &[ResourceDefinition]) -> CompileResult<Vec<Leaf>> {
    fn visit(
        ty: &Type,
        path: &mut Vec<PlaceProjection>,
        resources: &[ResourceDefinition],
        out: &mut Vec<Leaf>,
    ) -> CompileResult<()> {
        match ty {
            Type::Managed(_) => {
                let drops = packages::teardown_paths(ty, resources)?;
                if drops.len() > 1 || drops.first().is_some_and(|(p, _)| !p.is_empty()) {
                    return invalid(
                        "E-CODEGEN-PARTIAL",
                        "resource leaf recipe is not a direct head",
                        None,
                    );
                }
                out.push(Leaf {
                    path: path.clone(),
                    symbol: drops.into_iter().next().map(|(_, s)| s),
                });
            }
            Type::Tuple(fs) => {
                for (i, t) in fs.iter().enumerate() {
                    path.push(PlaceProjection::Field(i as u32));
                    visit(t, path, resources, out)?;
                    path.pop();
                }
            }
            Type::Package { fields, .. } => {
                for (i, (_, t)) in fields.iter().enumerate() {
                    path.push(PlaceProjection::Field(i as u32));
                    visit(t, path, resources, out)?;
                    path.pop();
                }
            }
            Type::Enum { variants, .. } => {
                for (v, (_, fs)) in variants.iter().enumerate() {
                    for (i, t) in fs.iter().enumerate() {
                        path.push(PlaceProjection::Variant(zil_ir::VariantProjection {
                            variant: v as u32,
                            index: i as u32,
                        }));
                        visit(t, path, resources, out)?;
                        path.pop();
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                for (e, t) in [(true, error.as_ref()), (false, result.as_ref())] {
                    path.push(PlaceProjection::ErrorArm(e));
                    visit(t, path, resources, out)?;
                    path.pop();
                }
            }
            t if t.is_copy_value() => {}
            _ => {
                return unsupported(
                    "partial ordinary cleanup contains a non-inline owning leaf",
                    None,
                );
            }
        }
        Ok(())
    }
    let mut result = Vec::new();
    visit(ty, &mut Vec::new(), resources, &mut result)?;
    Ok(result)
}
/// Reserve path recipes, leaf flags, repeated type/GEP construction and actual
/// drop-site expansion before FunctionEmitter allocates any recipe vectors.
pub(super) fn prepay(
    ty: &Type,
    f: &TypedFunction,
    resources: &[ResourceDefinition],
    b: &mut drop_plan::Budget<'_>,
) -> CompileResult<()> {
    fn count(
        ty: &Type,
        depth: usize,
        b: &mut drop_plan::Budget<'_>,
    ) -> CompileResult<(usize, usize)> {
        b.pay(depth, 1)?;
        let mut leaves = 0usize;
        let mut routes = 0usize;
        let mut child = |t: &Type| -> CompileResult<()> {
            let (n, p) = count(t, depth + 1, b)?;
            leaves = leaves.checked_add(n).ok_or_else(exhausted)?;
            routes = routes.checked_add(p).ok_or_else(exhausted)?;
            Ok(())
        };
        match ty {
            Type::Managed(_) => return Ok((1, depth)),
            Type::Tuple(fs) => {
                for t in fs {
                    child(t)?;
                }
            }
            Type::Package { fields, .. } => {
                for (_, t) in fields {
                    child(t)?;
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fs) in variants {
                    for t in fs {
                        child(t)?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                child(error)?;
                child(result)?;
            }
            _ => {}
        }
        Ok((leaves, routes))
    }
    let before = b.spent();
    b.tree(ty, 0)?;
    let structural_work = b.spent() - before;
    let (n, paths) = count(ty, 0, b)?;
    b.pay(0, f.blocks.len())?;
    let sites = f.blocks.iter().try_fold(1usize, |n, b| {
        n.checked_add(b.instructions.len()).ok_or_else(exhausted)
    })?;
    let weight = b
        .machine_weight(ty, 0)?
        .checked_add(structural_work)
        .ok_or_else(exhausted)?;
    let mut metadata = 1usize;
    for resource in resources {
        let before = b.spent();
        b.tree(&resource.payload, 0)?;
        metadata = metadata
            .checked_add(resource.name.len())
            .and_then(|v| v.checked_add(b.spent() - before))
            .ok_or_else(exhausted)?;
    }
    let bound = n
        .checked_add(paths)
        .and_then(|v| v.checked_add(1))
        .and_then(|v| v.checked_mul(weight.saturating_add(metadata)))
        .and_then(|v| v.checked_mul(sites))
        .and_then(|v| v.checked_mul(4))
        .ok_or_else(exhausted)?;
    b.pay(0, bound)
}
fn exhausted() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Codegen,
        "partial inline recipe/emission budget exceeded",
        None,
    )]
}
impl FunctionEmitter<'_> {
    fn place_inst(&mut self, out: Option<String>, op: m::Op) {
        self.event(m::Event::Instruction(m::Inst::new(out.map(m::name), op)));
    }
    fn field_pointer(&mut self, ty: &Type, ptr: &str, index: u32) -> CompileResult<(Type, String)> {
        let next = match ty {
            Type::Tuple(fs) => fs.get(index as usize),
            Type::Package { fields, .. } => fields.get(index as usize).map(|(_, t)| t),
            _ => None,
        }
        .ok_or_else(|| {
            vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-PARTIAL",
                None,
                "field path differs from actual inline storage",
            )]
        })?
        .clone();
        let p = self.temp();
        self.place_inst(
            Some(p.clone()),
            m::Op::Gep {
                ty: llvm_type(ty)?,
                ptr: m::val(ptr),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), index),
                ],
            },
        );
        Ok((next, p))
    }
    fn error_tag_at(&mut self, ty: &Type, ptr: &str) -> CompileResult<String> {
        if !matches!(ty, Type::ErrorUnion { .. }) {
            return invalid(
                "E-CODEGEN-PARTIAL",
                "error tag requires real ErrorUnion",
                None,
            );
        }
        let p = self.temp();
        self.place_inst(
            Some(p.clone()),
            m::Op::Gep {
                ty: llvm_type(ty)?,
                ptr: m::val(ptr),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), 0),
                ],
            },
        );
        let tag = self.temp();
        self.place_inst(
            Some(tag.clone()),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::val(p),
                align: 1,
            },
        );
        Ok(tag)
    }
    fn error_field_pointer(
        &mut self,
        ty: &Type,
        ptr: &str,
        is_error: bool,
    ) -> CompileResult<(Type, String)> {
        let Type::ErrorUnion { error, result } = ty else {
            return invalid("E-CODEGEN-PARTIAL", "error field requires ErrorUnion", None);
        };
        let next = if is_error {
            error.as_ref()
        } else {
            result.as_ref()
        }
        .clone();
        let p = self.temp();
        self.place_inst(
            Some(p.clone()),
            m::Op::Gep {
                ty: llvm_type(ty)?,
                ptr: m::val(ptr),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), if is_error { 1 } else { 2 }),
                ],
            },
        );
        Ok((next, p))
    }
    pub(super) fn actual_place(
        &mut self,
        root: &Type,
        ptr: &str,
        path: &[PlaceProjection],
    ) -> CompileResult<(Type, String)> {
        let mut ty = root.clone();
        let mut pointer = ptr.to_owned();
        for step in path {
            match step {
                PlaceProjection::Field(i) => {
                    (ty, pointer) = self.field_pointer(&ty, &pointer, *i)?;
                }
                PlaceProjection::Variant(p) => {
                    self.guard_enum_variant(&ty, &pointer, p.variant)?;
                    (ty, pointer) =
                        self.variant_field_pointer(&ty, &pointer, p.variant, p.index)?;
                }
                PlaceProjection::ErrorArm(e) => {
                    let tag = self.error_tag_at(&ty, &pointer)?;
                    let bad = self.temp();
                    self.place_inst(
                        Some(bad.clone()),
                        m::Op::Compare {
                            predicate: "ne".into(),
                            ty: m::Ty::Int(1),
                            left: m::val(tag),
                            right: m::val(*e),
                        },
                    );
                    self.trap_if(&bad);
                    (ty, pointer) = self.error_field_pointer(&ty, &pointer, *e)?;
                }
            }
        }
        Ok((ty, pointer))
    }
    pub(super) fn borrow_actual_place(
        &mut self,
        i: &Instruction,
        local: LocalId,
        path: &[PlaceProjection],
    ) -> CompileResult<()> {
        let root = self.local_type(local)?.clone();
        let (_, p) = self.actual_place(&root, &format!("%local.{}", local.0), path)?;
        self.place_inst(
            Some(value(result(i)?)),
            m::Op::Gep {
                ty: m::Ty::Int(8),
                ptr: m::val(p),
                indices: vec![m::Arg::new(m::Ty::Int(32), 0)],
            },
        );
        Ok(())
    }
    pub(super) fn tag_actual_place(
        &mut self,
        i: &Instruction,
        local: LocalId,
        path: &[PlaceProjection],
    ) -> CompileResult<()> {
        let root = self.local_type(local)?.clone();
        let (ty, p) = self.actual_place(&root, &format!("%local.{}", local.0), path)?;
        match ty {
            Type::Enum { .. } => self.enum_tag_at(&ty, &p, value(result(i)?)),
            Type::ErrorUnion { .. } => {
                let tag = self.error_tag_at(&ty, &p)?;
                self.place_inst(
                    Some(value(result(i)?)),
                    m::Op::Cast {
                        opcode: "zext".into(),
                        arg: m::Arg::new(m::Ty::Int(1), m::val(tag)),
                        to: m::Ty::Int(32),
                    },
                );
                Ok(())
            }
            _ => invalid(
                "E-CODEGEN-PARTIAL",
                "tag endpoint is not a Sum",
                Some(i.span),
            ),
        }
    }
    pub(super) fn borrow_address_place(
        &mut self,
        i: &Instruction,
        address: ValueId,
        path: &[PlaceProjection],
    ) -> CompileResult<()> {
        let Type::Address { inner, .. } = self.value_type(address)?.clone() else {
            return invalid(
                "E-CODEGEN-PARTIAL",
                "mixed input is not Address",
                Some(i.span),
            );
        };
        let (_, p) = self.actual_place(&inner, &value(address), path)?;
        self.place_inst(
            Some(value(result(i)?)),
            m::Op::Gep {
                ty: m::Ty::Int(8),
                ptr: m::val(p),
                indices: vec![m::Arg::new(m::Ty::Int(32), 0)],
            },
        );
        Ok(())
    }
    pub(super) fn tag_address_place(
        &mut self,
        i: &Instruction,
        address: ValueId,
        path: &[PlaceProjection],
    ) -> CompileResult<()> {
        let Type::Address { inner, .. } = self.value_type(address)?.clone() else {
            return invalid(
                "E-CODEGEN-PARTIAL",
                "mixed input is not Address",
                Some(i.span),
            );
        };
        let (ty, p) = self.actual_place(&inner, &value(address), path)?;
        match ty {
            Type::Enum { .. } => self.enum_tag_at(&ty, &p, value(result(i)?)),
            Type::ErrorUnion { .. } => {
                let tag = self.error_tag_at(&ty, &p)?;
                self.place_inst(
                    Some(value(result(i)?)),
                    m::Op::Cast {
                        opcode: "zext".into(),
                        arg: m::Arg::new(m::Ty::Int(1), m::val(tag)),
                        to: m::Ty::Int(32),
                    },
                );
                Ok(())
            }
            _ => invalid(
                "E-CODEGEN-PARTIAL",
                "mixed tag endpoint is not Sum",
                Some(i.span),
            ),
        }
    }
    fn leaf_flag(local: LocalId, index: usize) -> String {
        format!("partial.live.{}.{}", local.0, index)
    }
    pub(super) fn partial_flags(
        &mut self,
        local: LocalId,
        allocate: bool,
        live: bool,
    ) -> CompileResult<()> {
        let count = self.partial_sums.get(&local).map_or(0, Vec::len);
        for index in 0..count {
            let flag = Self::leaf_flag(local, index);
            if allocate {
                self.place_inst(
                    Some(flag.clone()),
                    m::Op::Alloca {
                        ty: m::Ty::Int(1),
                        align: 1,
                    },
                );
            }
            self.place_inst(
                None,
                m::Op::Store {
                    arg: m::Arg::new(m::Ty::Int(1), live),
                    ptr: m::local(flag),
                    align: 1,
                },
            );
        }
        Ok(())
    }
    pub(super) fn take_actual_place(
        &mut self,
        i: &Instruction,
        local: LocalId,
        path: &[PlaceProjection],
    ) -> CompileResult<()> {
        let root = self.local_type(local)?.clone();
        let (ty, p) = self.actual_place(&root, &format!("%local.{}", local.0), path)?;
        if ty != Type::Unit {
            self.place_inst(
                Some(value(result(i)?)),
                m::Op::Load {
                    ty: llvm_type(&ty)?,
                    ptr: m::val(p),
                    align: alignment(&ty, self.closures)?,
                },
            );
        }
        let count = self.partial_sums.get(&local).map_or(0, Vec::len);
        for index in 0..count {
            if self.partial_sums[&local][index].path.starts_with(path) {
                self.place_inst(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(m::Ty::Int(1), false),
                        ptr: m::local(Self::leaf_flag(local, index)),
                        align: 1,
                    },
                );
            }
        }
        Ok(())
    }
    /// Return true only when this local has actual selective-transfer recipes.
    pub(super) fn drop_partial_place(&mut self, local: LocalId) -> CompileResult<bool> {
        let Some(leaves) = self.partial_sums.get(&local).cloned() else {
            return Ok(false);
        };
        let ty = self.local_type(local)?.clone();
        let id = self.continuation;
        self.continuation += 1;
        let active = self.temp();
        self.place_inst(
            Some(active.clone()),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        );
        let start = format!("partial.drop.{id}");
        let done = format!("partial.done.{id}");
        self.place_inst(
            None,
            m::Op::CondBranch {
                condition: m::val(active),
                yes: start.clone(),
                no: done.clone(),
            },
        );
        self.event(m::Event::Label(start));
        self.place_inst(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), false),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        );
        for (index, leaf) in leaves.iter().enumerate() {
            let skip = format!("partial.leaf.done.{id}.{index}");
            let mut at = ty.clone();
            let mut pointer = format!("%local.{}", local.0);
            for (step_index, step) in leaf.path.iter().enumerate() {
                match step {
                    PlaceProjection::Field(field) => {
                        (at, pointer) = self.field_pointer(&at, &pointer, *field)?;
                    }
                    PlaceProjection::Variant(p) => {
                        let tag = self.temp();
                        self.enum_tag_at(&at, &pointer, tag.clone())?;
                        let equal = self.temp();
                        self.place_inst(
                            Some(equal.clone()),
                            m::Op::Compare {
                                predicate: "eq".into(),
                                ty: m::Ty::Int(32),
                                left: m::val(tag),
                                right: m::val(p.variant),
                            },
                        );
                        let yes = format!("partial.leaf.path.{id}.{index}.{step_index}");
                        self.place_inst(
                            None,
                            m::Op::CondBranch {
                                condition: m::val(equal),
                                yes: yes.clone(),
                                no: skip.clone(),
                            },
                        );
                        self.event(m::Event::Label(yes));
                        (at, pointer) =
                            self.variant_field_pointer(&at, &pointer, p.variant, p.index)?;
                    }
                    PlaceProjection::ErrorArm(e) => {
                        let tag = self.error_tag_at(&at, &pointer)?;
                        let equal = self.temp();
                        self.place_inst(
                            Some(equal.clone()),
                            m::Op::Compare {
                                predicate: "eq".into(),
                                ty: m::Ty::Int(1),
                                left: m::val(tag),
                                right: m::val(*e),
                            },
                        );
                        let yes = format!("partial.leaf.path.{id}.{index}.{step_index}");
                        self.place_inst(
                            None,
                            m::Op::CondBranch {
                                condition: m::val(equal),
                                yes: yes.clone(),
                                no: skip.clone(),
                            },
                        );
                        self.event(m::Event::Label(yes));
                        (at, pointer) = self.error_field_pointer(&at, &pointer, *e)?;
                    }
                }
            }
            let flag = self.temp();
            self.place_inst(
                Some(flag.clone()),
                m::Op::Load {
                    ty: m::Ty::Int(1),
                    ptr: m::local(Self::leaf_flag(local, index)),
                    align: 1,
                },
            );
            let run = format!("partial.leaf.run.{id}.{index}");
            self.place_inst(
                None,
                m::Op::CondBranch {
                    condition: m::val(flag),
                    yes: run.clone(),
                    no: skip.clone(),
                },
            );
            self.event(m::Event::Label(run));
            self.place_inst(
                None,
                m::Op::Store {
                    arg: m::Arg::new(m::Ty::Int(1), false),
                    ptr: m::local(Self::leaf_flag(local, index)),
                    align: 1,
                },
            );
            if let Some(symbol) = &leaf.symbol {
                self.place_inst(
                    None,
                    m::Op::Call {
                        result: m::Ty::Void,
                        result_attrs: vec![],
                        callee: m::global(symbol.clone()),
                        args: vec![m::Arg::new(m::Ty::Ptr, m::val(pointer))],
                        vararg: None,
                    },
                );
            }
            self.place_inst(None, m::Op::Branch(skip.clone()));
            self.event(m::Event::Label(skip));
        }
        self.place_inst(None, m::Op::Branch(done.clone()));
        self.event(m::Event::Label(done));
        Ok(true)
    }
}
