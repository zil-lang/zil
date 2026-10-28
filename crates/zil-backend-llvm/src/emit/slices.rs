//! Native borrowed windows. Permission is established independently by Verify;
//! this module preserves the checked window and guards every offset operation.
use super::*;

impl FunctionEmitter<'_> {
    /// The array object's address is already the address of its first element.
    /// Avoid even a zero-offset GEP here: range guards precede address math.
    fn slice_base(&mut self, address: ValueId) -> CompileResult<(Type, m::Val, m::Val)> {
        let Type::Address { inner, .. } = self.value_type(address)?.clone() else {
            return invalid(
                "E-CODEGEN-CFG",
                "slice projection requires a checked address",
                Some(self.function.span),
            );
        };
        match *inner {
            Type::Array { element, length } => {
                Ok((*element, m::val(value(address)), m::val(length)))
            }
            Type::Slice { element } => {
                let data = self.temp();
                let length = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((data).clone())),
                    m::Op::Extract {
                        arg: m::Arg::new(
                            m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64)]),
                            m::val((value(address)).clone()),
                        ),
                        index: 0,
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((length).clone())),
                    m::Op::Extract {
                        arg: m::Arg::new(
                            m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64)]),
                            m::val((value(address)).clone()),
                        ),
                        index: 1,
                    },
                )));
                Ok((*element, m::val(data), m::val(length)))
            }
            _ => invalid(
                "E-CODEGEN-CFG",
                "slice projection lost its array or slice pointee",
                Some(self.function.span),
            ),
        }
    }

    pub(super) fn slice_range(
        &mut self,
        instruction: &Instruction,
        address: ValueId,
        start: Option<ValueId>,
        end: Option<ValueId>,
    ) -> CompileResult<()> {
        let (element, data, length) = self.slice_base(address)?;
        let start = match start {
            Some(value) => m::val(self.widened_index(value)?),
            None => m::Val::Int(0),
        };
        let end = match end {
            Some(value) => m::val(self.widened_index(value)?),
            None => length.clone(),
        };
        // Every admitted backing length is nonnegative. Unsigned comparisons
        // reject sign-extended negative endpoints as well as reversed ranges.
        let reversed = self.temp();
        let outside = self.temp();
        let invalid = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((reversed).clone())),
            m::Op::Compare {
                predicate: "ugt".into(),
                ty: m::Ty::Int(64),
                left: m::val((start).clone()),
                right: m::val((end).clone()),
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((outside).clone())),
            m::Op::Compare {
                predicate: "ugt".into(),
                ty: m::Ty::Int(64),
                left: m::val((end).clone()),
                right: m::val((length).clone()),
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((invalid).clone())),
            m::Op::Binary {
                opcode: "or".into(),
                ty: m::Ty::Int(1),
                left: m::val((reversed).clone()),
                right: m::val((outside).clone()),
            },
        )));
        self.trap_if(&invalid);

        let offset = self.temp();
        let count = self.temp();
        let pair = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((offset).clone())),
            m::Op::Gep {
                ty: (aggregate_field_type(&element)?).clone(),
                ptr: m::val((data).clone()),
                indices: vec![m::Arg::new(m::Ty::Int(64), m::val((start).clone()))],
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((count).clone())),
            m::Op::Binary {
                opcode: "sub".into(),
                ty: m::Ty::Int(64),
                left: m::val((end).clone()),
                right: m::val((start).clone()),
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((pair).clone())),
            m::Op::Insert {
                arg: m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64)]),
                    m::Val::Zero,
                ),
                index: 0,
                field: m::Arg::new(m::Ty::Ptr, m::val((offset).clone())),
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", result(instruction)?.0)),
            m::Op::Insert {
                arg: m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64)]),
                    m::val((pair).clone()),
                ),
                index: 1,
                field: m::Arg::new(m::Ty::Int(64), m::val((count).clone())),
            },
        )));
        Ok(())
    }

    pub(super) fn slice_index(
        &mut self,
        instruction: &Instruction,
        address: ValueId,
        index: ValueId,
    ) -> CompileResult<()> {
        let (element, data, length) = self.slice_base(address)?;
        let index = self.widened_index(index)?;
        let outside = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((outside).clone())),
            m::Op::Compare {
                predicate: "uge".into(),
                ty: m::Ty::Int(64),
                left: m::val((index).clone()),
                right: m::val((length).clone()),
            },
        )));
        self.trap_if(&outside);
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", result(instruction)?.0)),
            m::Op::Gep {
                ty: (aggregate_field_type(&element)?).clone(),
                ptr: m::val((data).clone()),
                indices: vec![m::Arg::new(m::Ty::Int(64), m::val((index).clone()))],
            },
        )));
        Ok(())
    }

    pub(super) fn reborrow_slice(
        &mut self,
        instruction: &Instruction,
        address: ValueId,
    ) -> CompileResult<()> {
        let data = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((data).clone())),
            m::Op::Extract {
                arg: m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64)]),
                    m::val((value(address)).clone()),
                ),
                index: 0,
            },
        )));
        // Preserve both fields. No heap box, address erasure or invented length.
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", result(instruction)?.0)),
            m::Op::Insert {
                arg: m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64)]),
                    m::val((value(address)).clone()),
                ),
                index: 0,
                field: m::Arg::new(m::Ty::Ptr, m::val((data).clone())),
            },
        )));
        Ok(())
    }
}
