// Frozen verbatim encoder suffix from ee72fd6f701efa5f7a75ea3f13d4effc20d5c4b8.
// Original crates/zil-ir/src/lib.rs suffix SHA-256: 51ca6f134a7af5e0abbd18a1f0320201c724240ad1faf64fd309b1b1f435526a
// The suffix below the marker is an independent baseline oracle; do not update it.
#![allow(dead_code)]
use zil_base::{Fingerprint, Span};
use zil_ir::*;
pub fn bytes(module: &TypedModule) -> Vec<u8> { encode_module(module) }
fn passing_mode_tag(mode: PassingMode) -> u8 {
    match mode {
        PassingMode::Copy => 0,
        PassingMode::Move => 1,
        PassingMode::Describe => 2,
        PassingMode::Create => 3,
        PassingMode::Give => 4,
        PassingMode::Borrow => 5,
        PassingMode::BorrowMut => 6,
        PassingMode::Infer => 7,
    }
}
fn cleanup_exit_tag(kind: CleanupExitKind) -> u8 {
    match kind {
        CleanupExitKind::Fallthrough => 0,
        CleanupExitKind::Return => 1,
        CleanupExitKind::Error => 2,
        CleanupExitKind::Break => 3,
        CleanupExitKind::Continue => 4,
    }
}
// BEGIN FROZEN BASELINE ENCODER
fn encode_module(module: &TypedModule) -> Vec<u8> {
    let mut bytes = b"ZIL-TYPED-MODULE-V27\0".to_vec();
    encode_u32(&mut bytes, module.schema_version);
    encode_u32(&mut bytes, module.module_id.0);
    bytes.extend_from_slice(&module.source_fingerprint.0);
    encode_text(&mut bytes, &module.target.to_json());
    encode_len(&mut bytes, module.functions.len());
    for f in &module.functions {
        encode_function(&mut bytes, f);
    }
    encode_len(&mut bytes, module.closures.len());
    for closure in &module.closures {
        encode_u32(&mut bytes, closure.id.0);
        encode_u32(&mut bytes, closure.function.0);
        encode_len(&mut bytes, closure.captures.len());
        for ty in &closure.captures {
            encode_type(&mut bytes, ty);
        }
        encode_len(&mut bytes, closure.parameters.len());
        for ty in &closure.parameters {
            encode_type(&mut bytes, ty);
        }
        encode_type(&mut bytes, &closure.result);
        bytes.push(match closure.kind {
            CallableKind::FunctionPointer => 0,
            CallableKind::Fn => 1,
            CallableKind::FnMut => 2,
            CallableKind::FnOnce => 3,
        });
        encode_len(&mut bytes, closure.lending_captures.len());
        for index in &closure.lending_captures {
            encode_u32(&mut bytes, *index);
        }
    }
    encode_len(&mut bytes, module.resources.len());
    for resource in &module.resources {
        encode_text(&mut bytes, &resource.name);
        encode_type(&mut bytes, &resource.payload);
        bytes.push(u8::from(resource.teardown.is_some()));
        if let Some(teardown) = resource.teardown {
            encode_u32(&mut bytes, teardown.0);
        }
    }
    encode_len(&mut bytes, module.unsupported.len());
    for feature in &module.unsupported {
        encode_text(&mut bytes, &feature.feature);
        encode_optional_span(&mut bytes, feature.span);
    }
    bytes
}
fn encode_u32(b: &mut Vec<u8>, n: u32) {
    b.extend_from_slice(&n.to_le_bytes());
}
fn encode_len(b: &mut Vec<u8>, n: usize) {
    b.extend_from_slice(&(n as u64).to_le_bytes());
}
fn encode_text(b: &mut Vec<u8>, s: &str) {
    encode_len(b, s.len());
    b.extend_from_slice(s.as_bytes());
}
fn encode_span(b: &mut Vec<u8>, s: Span) {
    encode_u32(b, s.file.0);
    encode_len(b, s.start);
    encode_len(b, s.end);
}
fn encode_optional_span(b: &mut Vec<u8>, s: Option<Span>) {
    b.push(u8::from(s.is_some()));
    if let Some(s) = s {
        encode_span(b, s);
    }
}
fn encode_type(b: &mut Vec<u8>, t: &Type) {
    match t {
        Type::Unit => b.push(0),
        Type::Bool => b.push(1),
        Type::I32 => b.push(2),
        Type::I64 => b.push(3),
        Type::U8 => b.push(18),
        Type::U16 => b.push(19),
        Type::ByteBuffer => b.push(24),
        Type::OwnedStore { element } => {
            b.push(25);
            encode_type(b, element);
        }
        Type::ForeignHandle => b.push(20),
        Type::CWideString => b.push(21),
        Type::ForeignCallback { parameters, result } => {
            b.push(22);
            encode_len(b, parameters.len());
            for ty in parameters {
                encode_type(b, ty);
            }
            encode_type(b, result);
        }
        Type::Tuple(ts) => {
            b.push(4);
            encode_len(b, ts.len());
            for t in ts {
                encode_type(b, t);
            }
        }
        Type::Array { element, length } => {
            b.push(14);
            encode_type(b, element);
            encode_u32(b, *length);
        }
        Type::Slice { element } => {
            b.push(23);
            encode_type(b, element);
        }
        Type::Record { name, fields } => {
            b.push(13);
            encode_text(b, name);
            encode_len(b, fields.len());
            for (name, ty) in fields {
                encode_text(b, name);
                encode_type(b, ty);
            }
        }
        Type::Package { name, fields } => {
            b.push(17);
            encode_text(b, name);
            encode_len(b, fields.len());
            for (name, ty) in fields {
                encode_text(b, name);
                encode_type(b, ty);
            }
        }
        Type::Enum { name, variants } => {
            b.push(15);
            encode_text(b, name);
            encode_len(b, variants.len());
            for (name, fields) in variants {
                encode_text(b, name);
                encode_len(b, fields.len());
                for ty in fields {
                    encode_type(b, ty);
                }
            }
        }
        Type::ErrorUnion { error, result } => {
            b.push(16);
            encode_type(b, error);
            encode_type(b, result);
        }
        Type::Named(s) => {
            b.push(5);
            encode_text(b, s);
        }
        Type::Address { mutable, inner } => {
            b.extend_from_slice(&[6, u8::from(*mutable)]);
            encode_type(b, inner);
        }
        Type::Managed(t) => {
            b.push(7);
            encode_type(b, t);
        }
        Type::Desc(t) => {
            b.push(8);
            encode_type(b, t);
        }
        Type::FunctionPointer { parameters, result } => {
            b.push(9);
            encode_len(b, parameters.len());
            for t in parameters {
                encode_type(b, t);
            }
            encode_type(b, result);
        }
        Type::Closure(id) => {
            b.push(10);
            encode_u32(b, id.0);
        }
        Type::Opaque { identity, hidden } => {
            b.push(11);
            encode_u32(b, identity.0);
            encode_type(b, hidden);
        }
        Type::Unsupported(s) => {
            b.push(12);
            encode_text(b, s);
        }
    }
}
fn encode_function(b: &mut Vec<u8>, f: &TypedFunction) {
    b.push(u8::from(f.callable_return_contract.is_some()));
    if let Some(contract) = &f.callable_return_contract {
        encode_callable_return_contract(b, contract);
    }
    b.push(u8::from(f.callable_contract.is_some()));
    if let Some(contract) = &f.callable_contract {
        encode_callable_contract(b, contract);
    }
    encode_u32(b, f.id.0);
    encode_text(b, &f.name);
    b.push(u8::from(f.foreign.is_some()));
    if let Some(foreign) = &f.foreign {
        encode_text(b, &foreign.abi);
        encode_text(b, &foreign.symbol);
        encode_len(b, foreign.noescape_parameters.len());
        for parameter in &foreign.noescape_parameters {
            encode_u32(b, *parameter);
        }
        b.push(u8::from(foreign.static_callbacks));
    }
    b.push(u8::from(f.environment.is_some()));
    if let Some(environment) = f.environment {
        encode_u32(b, environment.0);
    }
    encode_len(b, f.parameters.len());
    for p in &f.parameters {
        encode_u32(b, p.0);
    }
    encode_type(b, &f.return_type);
    b.push(passing_mode_tag(f.result_mode));
    encode_len(b, f.locals.len());
    for l in &f.locals {
        encode_u32(b, l.id.0);
        encode_text(b, &l.name);
        encode_type(b, &l.ty);
        b.push(u8::from(l.mutable));
        encode_span(b, l.span);
    }
    encode_len(b, f.values.len());
    for t in &f.values {
        encode_type(b, t);
    }
    encode_len(b, f.blocks.len());
    for block in &f.blocks {
        encode_u32(b, block.id.0);
        encode_len(b, block.instructions.len());
        for i in &block.instructions {
            b.push(u8::from(i.result.is_some()));
            if let Some(v) = i.result {
                encode_u32(b, v.0);
            }
            encode_instruction(b, &i.kind);
            encode_span(b, i.span);
        }
        match &block.terminator {
            Terminator::Return(v) => {
                b.push(0);
                b.push(u8::from(v.is_some()));
                if let Some(v) = v {
                    encode_u32(b, v.0);
                }
            }
            Terminator::Jump(t) => {
                b.push(1);
                encode_u32(b, t.0);
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                b.push(2);
                encode_u32(b, condition.0);
                encode_u32(b, then_block.0);
                encode_u32(b, else_block.0);
            }
            Terminator::Unreachable => b.push(3),
            Terminator::Trap => b.push(4),
        }
    }
    encode_u32(b, f.entry.0);
    encode_span(b, f.span);
}
fn encode_constant(b: &mut Vec<u8>, c: &Constant) {
    match c {
        Constant::Unit => b.push(0),
        Constant::Bool(v) => b.extend_from_slice(&[1, u8::from(*v)]),
        Constant::U8(v) => b.extend_from_slice(&[4, *v]),
        Constant::U16(v) => {
            b.push(5);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::NullHandle => b.push(6),
        Constant::NullWideString => b.push(7),
        Constant::WideString(units) => {
            b.push(8);
            encode_len(b, units.len());
            for unit in units {
                b.extend_from_slice(&unit.to_le_bytes());
            }
        }
        Constant::I32(v) => {
            b.push(2);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::I64(v) => {
            b.push(3);
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
}
fn encode_instruction(b: &mut Vec<u8>, i: &InstructionKind) {
    match i {
        InstructionKind::ConvertInteger { value, target } => {
            b.push(59);
            encode_u32(b, value.0);
            encode_type(b, target);
        }
        InstructionKind::MakePackage { fields } => {
            b.push(49);
            encode_len(b, fields.len());
            for field in fields {
                encode_u32(b, field.0);
            }
        }
        InstructionKind::MakeOwnedArray { elements } => {
            b.push(72);
            encode_len(b, elements.len());
            for element in elements {
                encode_u32(b, element.0);
            }
        }
        InstructionKind::ProjectOwnedArrayField {
            local,
            index,
            field,
        } => {
            b.push(73);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            encode_u32(b, *field);
        }
        InstructionKind::TakeOwnedArrayElement { local, index } => {
            b.push(74);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::DropOwnedArrayElement { local, index } => {
            b.push(75);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::StoreOwnedArrayElement {
            local,
            index,
            value,
        } => {
            b.push(76);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::DropOwnedArray { local } => {
            b.push(77);
            encode_u32(b, local.0);
        }
        InstructionKind::BorrowOwnedArrayElement {
            local,
            index,
            mutable,
        } => {
            b.push(79);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            b.push(u8::from(*mutable));
        }
        InstructionKind::BorrowPackageField {
            local,
            index,
            mutable,
        } => {
            b.push(87);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            b.push(u8::from(*mutable));
        }
        InstructionKind::ReadPackageField { local, index } => {
            b.push(50);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::DescribePackageField { local, index } => {
            b.push(51);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::DropPackage { local } => {
            b.push(52);
            encode_u32(b, local.0);
        }
        InstructionKind::TakePackageField { local, index } => {
            b.push(54);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::DropPackageField { local, index } => {
            b.push(55);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::StorePackageField {
            local,
            index,
            value,
        } => {
            b.push(56);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::BindPackageCapture { local, index } => {
            b.push(57);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::EndPackageCapture { local, index } => {
            b.push(58);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::ScopeEnter { scope } => {
            b.push(44);
            encode_u32(b, scope.0);
        }
        InstructionKind::ScopeExit { scope, kind } => {
            b.push(45);
            encode_u32(b, scope.0);
            b.push(cleanup_exit_tag(*kind));
        }
        InstructionKind::MakeErrorUnion { is_error, value } => {
            b.push(46);
            b.push(u8::from(*is_error));
            encode_u32(b, value.0);
        }
        InstructionKind::ErrorTag { value } => {
            b.push(47);
            encode_u32(b, value.0);
        }
        InstructionKind::ReadErrorUnion { value, is_error } => {
            b.push(48);
            encode_u32(b, value.0);
            b.push(u8::from(*is_error));
        }
        InstructionKind::TakeErrorUnion { value, is_error } => {
            b.push(70);
            encode_u32(b, value.0);
            b.push(u8::from(*is_error));
        }
        InstructionKind::DropErrorUnion { local } => {
            b.push(71);
            encode_u32(b, local.0);
        }
        InstructionKind::MakeManaged { fields } => {
            b.push(34);
            encode_len(b, fields.len());
            for field in fields {
                encode_u32(b, field.0);
            }
        }
        InstructionKind::DescribeLocal { local } => {
            b.push(35);
            encode_u32(b, local.0);
        }
        InstructionKind::ReadDesc { desc, index } => {
            b.push(36);
            encode_u32(b, desc.0);
            encode_u32(b, *index);
        }
        InstructionKind::WriteDesc { desc, index, value } => {
            b.push(37);
            encode_u32(b, desc.0);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::DropManaged { local } => {
            b.push(38);
            encode_u32(b, local.0);
        }
        InstructionKind::DescribeCapture { index } => {
            b.push(39);
            encode_u32(b, *index);
        }
        InstructionKind::TakeCapture { index } => {
            b.push(40);
            encode_u32(b, *index);
        }
        InstructionKind::BorrowCapture { index, mutable } => {
            b.push(78);
            encode_u32(b, *index);
            b.push(u8::from(*mutable));
        }
        InstructionKind::CallClosureBorrowed {
            callee,
            kind,
            arguments,
        } => {
            b.push(53);
            encode_u32(b, callee.0);
            b.push(callable_kind_tag(*kind));
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::CallClosureShared { callee, arguments } => {
            b.push(41);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::CallClosureOnce { callee, arguments } => {
            b.push(42);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::DropClosure { local } => {
            b.push(43);
            encode_u32(b, local.0);
        }
        InstructionKind::UnsafeEnter { region } => {
            b.push(31);
            encode_u32(b, *region);
        }
        InstructionKind::UnsafeExit { region } => {
            b.push(32);
            encode_u32(b, *region);
        }
        InstructionKind::ForeignCall {
            function,
            arguments,
            region,
        } => {
            b.push(33);
            encode_u32(b, function.0);
            encode_u32(b, *region);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::Constant(c) => {
            b.push(0);
            encode_constant(b, c);
        }
        InstructionKind::Load(l) => {
            b.push(1);
            encode_u32(b, l.0);
        }
        InstructionKind::Store { local, value } => {
            b.push(2);
            encode_u32(b, local.0);
            encode_u32(b, value.0);
        }
        InstructionKind::Unary { op, operand } => {
            b.push(3);
            b.push(*op as u8);
            encode_u32(b, operand.0);
        }
        InstructionKind::Binary { op, left, right } => {
            b.push(4);
            b.push(*op as u8);
            encode_u32(b, left.0);
            encode_u32(b, right.0);
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            b.push(5);
            encode_u32(b, function.0);
            encode_len(b, arguments.len());
            for a in arguments {
                encode_u32(b, a.0);
            }
        }
        InstructionKind::ForeignFunctionRef { function } => {
            b.push(60);
            encode_u32(b, function.0);
        }
        InstructionKind::FunctionRef { function } => {
            b.push(25);
            encode_u32(b, function.0);
        }
        InstructionKind::CallIndirect { callee, arguments } => {
            b.push(26);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::ExtractIndex { aggregate, index } => {
            b.push(16);
            encode_u32(b, aggregate.0);
            encode_u32(b, index.0);
        }
        InstructionKind::UpdateIndex {
            aggregate,
            index,
            value,
        } => {
            b.push(17);
            encode_u32(b, aggregate.0);
            encode_u32(b, index.0);
            encode_u32(b, value.0);
        }
        InstructionKind::MakeAggregate { fields } => {
            b.push(13);
            encode_len(b, fields.len());
            for value in fields {
                encode_u32(b, value.0);
            }
        }
        InstructionKind::MakeVariant { variant, fields } => {
            b.push(18);
            encode_u32(b, *variant);
            encode_len(b, fields.len());
            for value in fields {
                encode_u32(b, value.0);
            }
        }
        InstructionKind::VariantTag { value } => {
            b.push(19);
            encode_u32(b, value.0);
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            b.push(20);
            encode_u32(b, value.0);
            encode_u32(b, *variant);
            encode_u32(b, *index);
        }
        InstructionKind::ExtractField { aggregate, index } => {
            b.push(14);
            encode_u32(b, aggregate.0);
            encode_u32(b, *index);
        }
        InstructionKind::UpdateField {
            aggregate,
            index,
            value,
        } => {
            b.push(15);
            encode_u32(b, aggregate.0);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::MakeClosure { closure, captures } => {
            b.push(6);
            encode_u32(b, closure.0);
            encode_len(b, captures.len());
            for capture in captures {
                encode_u32(b, capture.0);
            }
        }
        InstructionKind::ReadCapture { index } => {
            b.push(7);
            encode_u32(b, *index);
        }
        InstructionKind::WriteCapture { index, value } => {
            b.push(22);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::CallClosureMut { callee, arguments } => {
            b.push(21);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::CallClosure { callee, arguments } => {
            b.push(8);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::BorrowLocal { local } => {
            b.push(9);
            encode_u32(b, local.0);
        }
        InstructionKind::BorrowLocalMut { local } => {
            b.push(27);
            encode_u32(b, local.0);
        }
        InstructionKind::ReborrowAddress { address, mutable } => {
            b.push(28);
            encode_u32(b, address.0);
            b.push(u8::from(*mutable));
        }
        InstructionKind::WriteAddress { address, value } => {
            b.push(29);
            encode_u32(b, address.0);
            encode_u32(b, value.0);
        }
        InstructionKind::TakeLocal { local } => {
            b.push(30);
            encode_u32(b, local.0);
        }
        InstructionKind::ReadAddress { address } => {
            b.push(10);
            encode_u32(b, address.0);
        }
        InstructionKind::AddressField { address, index } => {
            b.push(23);
            encode_u32(b, address.0);
            encode_u32(b, *index);
        }
        InstructionKind::AddressIndex { address, index } => {
            b.push(24);
            encode_u32(b, address.0);
            encode_u32(b, index.0);
        }
        InstructionKind::AddressSlice {
            address,
            start,
            end,
        } => {
            b.push(61);
            encode_u32(b, address.0);
            for endpoint in [start, end] {
                b.push(u8::from(endpoint.is_some()));
                if let Some(value) = endpoint {
                    encode_u32(b, value.0);
                }
            }
        }
        InstructionKind::SliceLength { address } => {
            b.push(62);
            encode_u32(b, address.0);
        }
        InstructionKind::ByteBufferNew => b.push(63),
        InstructionKind::OwnedStoreNew { element } => {
            b.push(80);
            encode_type(b, element);
        }
        InstructionKind::OwnedStoreReserve { store, capacity } => {
            b.push(81);
            encode_u32(b, store.0);
            encode_u32(b, capacity.0);
        }
        InstructionKind::OwnedStorePush { store, value } => {
            b.push(82);
            encode_u32(b, store.0);
            encode_u32(b, value.0);
        }
        InstructionKind::OwnedStorePop { store } => {
            b.push(83);
            encode_u32(b, store.0);
        }
        InstructionKind::OwnedStoreLen { store } => {
            b.push(84);
            encode_u32(b, store.0);
        }
        InstructionKind::OwnedStoreCapacity { store } => {
            b.push(85);
            encode_u32(b, store.0);
        }
        InstructionKind::OwnedStoreBorrow { store, index } => {
            b.push(86);
            encode_u32(b, store.0);
            encode_u32(b, index.0);
        }
        InstructionKind::OwnedStoreBorrowMut { store, index } => {
            b.push(88);
            encode_u32(b, store.0);
            encode_u32(b, index.0);
        }
        InstructionKind::ByteBufferReserve { buffer, capacity } => {
            b.push(64);
            encode_u32(b, buffer.0);
            encode_u32(b, capacity.0);
        }
        InstructionKind::ByteBufferPush { buffer, value } => {
            b.push(65);
            encode_u32(b, buffer.0);
            encode_u32(b, value.0);
        }
        InstructionKind::ByteBufferTruncate { buffer, length } => {
            b.push(69);
            encode_u32(b, buffer.0);
            encode_u32(b, length.0);
        }
        InstructionKind::ByteBufferLen { buffer } => {
            b.push(66);
            encode_u32(b, buffer.0);
        }
        InstructionKind::ByteBufferCapacity { buffer } => {
            b.push(67);
            encode_u32(b, buffer.0);
        }
        InstructionKind::ByteBufferBorrow { buffer, mutable } => {
            b.push(68);
            encode_u32(b, buffer.0);
            b.push(u8::from(*mutable));
        }
        InstructionKind::StorageLive(local) => {
            b.push(11);
            encode_u32(b, local.0);
        }
        InstructionKind::StorageDead(local) => {
            b.push(12);
            encode_u32(b, local.0);
        }
        InstructionKind::Unsupported(s) => {
            b.push(255);
            encode_text(b, s);
        }
    }
}

/// Deterministic binding for a raw callable relation, not a verification token.
pub fn callable_contract_fingerprint(contract: &CallableParameterContract) -> Fingerprint {
    let mut bytes = b"ZIL-CALLABLE-CONTRACT-3\0".to_vec();
    encode_callable_contract(&mut bytes, contract);
    Fingerprint::of(&bytes)
}
/// Deterministic identity only; this hash never grants verification authority.
pub fn callable_return_contract_fingerprint(contract: &CallableReturnContract) -> Fingerprint {
    let mut bytes = b"ZIL-CALLABLE-RETURN-CONTRACT-2\0".to_vec();
    encode_callable_return_contract(&mut bytes, contract);
    Fingerprint::of(&bytes)
}
fn encode_callable_return_contract(b: &mut Vec<u8>, c: &CallableReturnContract) {
    encode_u32(b, c.closure.0);
    b.push(callable_kind_tag(c.kind));
    for indices in [&c.hold, &c.call, &c.writes, &c.cleanup, &c.lending_captures] {
        encode_len(b, indices.len());
        for index in indices {
            encode_u32(b, *index);
        }
    }
}
fn callable_kind_tag(kind: CallableKind) -> u8 {
    match kind {
        CallableKind::FunctionPointer => 0,
        CallableKind::Fn => 1,
        CallableKind::FnMut => 2,
        CallableKind::FnOnce => 3,
    }
}
fn encode_callable_contract(b: &mut Vec<u8>, c: &CallableParameterContract) {
    encode_len(b, c.roles.len());
    for role in &c.roles {
        encode_u32(b, role.parameter);
        encode_u32(b, role.closure.0);
        b.push(callable_kind_tag(role.kind));
        for indices in [&role.hold, &role.call, &role.writes] {
            encode_len(b, indices.len());
            for index in indices {
                encode_u32(b, *index);
            }
        }
        b.push(match role.cleanup {
            CallableCleanup::Owned => 0,
            CallableCleanup::Borrowed => 1,
        });
    }
    match &c.traces {
        CallableTraces::Finite(traces) => {
            b.push(0);
            encode_len(b, traces.len());
            for trace in traces {
                encode_callable_calls(b, &trace.calls);
                encode_callable_exit(b, trace.exit);
            }
        }
        CallableTraces::Flow(flow) => {
            b.push(1);
            encode_u32(b, flow.entry);
            encode_len(b, flow.nodes.len());
            for node in &flow.nodes {
                encode_callable_calls(b, &node.calls);
                match &node.next {
                    CallableFlowNext::Edges(edges) => {
                        b.push(0);
                        encode_len(b, edges.len());
                        for edge in edges {
                            encode_u32(b, *edge);
                        }
                    }
                    CallableFlowNext::Exit(exit) => {
                        b.push(1);
                        encode_callable_exit(b, *exit);
                    }
                }
            }
        }
    }
}
fn encode_callable_calls(b: &mut Vec<u8>, calls: &[CallEvent]) {
    encode_len(b, calls.len());
    for call in calls {
        encode_u32(b, call.parameter);
        b.push(callable_kind_tag(call.kind));
    }
}
fn encode_callable_exit(b: &mut Vec<u8>, exit: CallableExit) {
    b.push(match exit {
        CallableExit::Return => 0,
        CallableExit::Trap => 1,
    });
}
