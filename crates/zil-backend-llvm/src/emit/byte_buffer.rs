//! Explicit sealed byte storage. Only independently verified primitives reach
//! these helpers; no public pointer/length/capacity construction is exposed.
use super::*;

pub(super) fn is_resource(ty: &Type, definitions: &[ResourceDefinition]) -> bool {
    matches!(ty, Type::Managed(payload) | Type::Desc(payload)
        if payload.as_ref() == &Type::ByteBuffer
        && definitions.iter().any(|definition|
            definition.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME
            && definition.payload == Type::ByteBuffer
            && definition.teardown.is_none()))
}

/// Emit allocator declarations once, checking rather than silently overriding
/// a user's foreign ABI. Buffer primitives never import a C source host.
pub(super) fn runtime(lir: &LirModule) -> CompileResult<String> {
    if !lir
        .resources
        .iter()
        .any(|r| r.payload == Type::ByteBuffer || matches!(r.payload, Type::OwnedStore { .. }))
    {
        return Ok(String::new());
    }
    let windows = lir.target.triple == "x86_64-pc-windows-msvc";
    let declarations = if windows {
        vec![
            ("GetProcessHeap", vec![], Type::ForeignHandle),
            (
                "HeapAlloc",
                vec![Type::ForeignHandle, Type::I32, Type::I64],
                Type::ForeignHandle,
            ),
            (
                "HeapReAlloc",
                vec![
                    Type::ForeignHandle,
                    Type::I32,
                    Type::ForeignHandle,
                    Type::I64,
                ],
                Type::ForeignHandle,
            ),
            (
                "HeapFree",
                vec![Type::ForeignHandle, Type::I32, Type::ForeignHandle],
                Type::I32,
            ),
        ]
    } else {
        vec![
            (
                "realloc",
                vec![Type::ForeignHandle, Type::I64],
                Type::ForeignHandle,
            ),
            ("free", vec![Type::ForeignHandle], Type::Unit),
        ]
    };
    for function in &lir.functions {
        if function
            .foreign
            .as_ref()
            .is_some_and(|f| f.symbol.starts_with("zil.bytes."))
        {
            return invalid(
                "E-CODEGEN-FOREIGN",
                "foreign symbol conflicts with byte-storage internal ABI",
                Some(function.span),
            );
        }
    }
    let mut output = String::new();
    for (name, parameters, result) in declarations {
        let existing = lir
            .functions
            .iter()
            .find(|function| function.foreign.as_ref().is_some_and(|f| f.symbol == name));
        if let Some(function) = existing {
            let actual = function
                .parameters
                .iter()
                .map(|id| function.locals[id.0 as usize].ty.clone())
                .collect::<Vec<_>>();
            if actual != parameters || function.return_type != result {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    format!("{name} conflicts with the byte-storage allocator ABI"),
                    Some(function.span),
                );
            }
        } else {
            let parameters = parameters
                .iter()
                .map(llvm_type)
                .collect::<CompileResult<Vec<_>>>()?
                .join(", ");
            output.push_str(&format!(
                "declare {} @{name}({parameters})\n",
                llvm_type(&result)?
            ));
        }
    }
    if windows {
        output.push_str(WINDOWS_ALLOCATION);
    } else {
        output.push_str(LINUX_ALLOCATION);
    }
    output.push_str(STORAGE_OPERATIONS);
    Ok(output)
}

const LINUX_ALLOCATION: &str = r#"
define internal ptr @zil.bytes.reallocate(ptr %old, i64 %size) {
entry:
  %result = call ptr @realloc(ptr %old, i64 %size)
  ret ptr %result
}
define internal void @zil.bytes.release(ptr %payload) {
entry:
  call void @free(ptr %payload)
  ret void
}
"#;

const WINDOWS_ALLOCATION: &str = r#"
define internal ptr @zil.bytes.reallocate(ptr %old, i64 %size) {
entry:
  %heap = call ptr @GetProcessHeap()
  %heap.ok = icmp ne ptr %heap, null
  br i1 %heap.ok, label %choose, label %failed
choose:
  %empty = icmp eq ptr %old, null
  br i1 %empty, label %allocate, label %reallocate
allocate:
  %fresh = call ptr @HeapAlloc(ptr %heap, i32 0, i64 %size)
  ret ptr %fresh
reallocate:
  %grown = call ptr @HeapReAlloc(ptr %heap, i32 0, ptr %old, i64 %size)
  ret ptr %grown
failed:
  ret ptr null
}
define internal void @zil.bytes.release(ptr %payload) {
entry:
  %empty = icmp eq ptr %payload, null
  br i1 %empty, label %done, label %release
release:
  %heap = call ptr @GetProcessHeap()
  %released = call i32 @HeapFree(ptr %heap, i32 0, ptr %payload)
  %ok = icmp ne i32 %released, 0
  br i1 %ok, label %done, label %failed
failed:
  call void @llvm.trap()
  unreachable
done:
  ret void
}
"#;

const STORAGE_OPERATIONS: &str = r#"
define internal i1 @zil.bytes.reserve(ptr %head, i64 %requested) {
entry:
  %negative = icmp slt i64 %requested, 0
  br i1 %negative, label %failed, label %capacity
capacity:
  %cap.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 2
  %cap = load i64, ptr %cap.field, align 8
  %fits = icmp ule i64 %requested, %cap
  br i1 %fits, label %done, label %prepare
prepare:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %old = load ptr, ptr %ptr.field, align 8
  %next = call ptr @zil.bytes.reallocate(ptr %old, i64 %requested)
  %ok = icmp ne ptr %next, null
  br i1 %ok, label %publish, label %failed
publish:
  store ptr %next, ptr %ptr.field, align 8
  store i64 %requested, ptr %cap.field, align 8
  br label %done
done:
  ret i1 true
failed:
  ret i1 false
}
define internal i1 @zil.bytes.truncate(ptr %head, i64 %requested) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %len = load i64, ptr %len.field, align 8
  %nonnegative = icmp sge i64 %requested, 0
  %within = icmp ule i64 %requested, %len
  %valid = and i1 %nonnegative, %within
  br i1 %valid, label %publish, label %failed
publish:
  store i64 %requested, ptr %len.field, align 8
  ret i1 true
failed:
  ret i1 false
}
define internal i1 @zil.bytes.push(ptr %head, i8 %byte) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %cap.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 2
  %len = load i64, ptr %len.field, align 8
  %cap = load i64, ptr %cap.field, align 8
  %room = icmp ult i64 %len, %cap
  br i1 %room, label %initialize, label %full
initialize:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %payload = load ptr, ptr %ptr.field, align 8
  %slot = getelementptr i8, ptr %payload, i64 %len
  store i8 %byte, ptr %slot, align 1
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret i1 true
full:
  ret i1 false
}
define internal void @zil.bytes.drop(ptr %head) {
entry:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %payload = load ptr, ptr %ptr.field, align 8
  call void @zil.bytes.release(ptr %payload)
  ret void
}
"#;

impl FunctionEmitter<'_> {
    pub(super) fn byte_buffer_instruction(
        &mut self,
        instruction: &Instruction,
    ) -> CompileResult<()> {
        let output = result(instruction)?;
        match instruction.kind {
            InstructionKind::ByteBufferNew => {
                self.line(format!(
                    "  {} = freeze {{ ptr, i64, i64 }} zeroinitializer",
                    value(output)
                ));
            }
            InstructionKind::ByteBufferReserve { buffer, capacity } => {
                self.line(format!(
                    "  {} = call i1 @zil.bytes.reserve(ptr {}, i64 {})",
                    value(output),
                    value(buffer),
                    value(capacity)
                ));
            }
            InstructionKind::ByteBufferTruncate { buffer, length } => {
                self.line(format!(
                    "  {} = call i1 @zil.bytes.truncate(ptr {}, i64 {})",
                    value(output),
                    value(buffer),
                    value(length)
                ));
            }
            InstructionKind::ByteBufferPush {
                buffer,
                value: byte,
            } => {
                self.line(format!(
                    "  {} = call i1 @zil.bytes.push(ptr {}, i8 {})",
                    value(output),
                    value(buffer),
                    value(byte)
                ));
            }
            InstructionKind::ByteBufferLen { buffer }
            | InstructionKind::ByteBufferCapacity { buffer } => {
                let index = if matches!(instruction.kind, InstructionKind::ByteBufferLen { .. }) {
                    1
                } else {
                    2
                };
                let field = self.temp();
                self.line(format!(
                    "  {field} = getelementptr {{ ptr, i64, i64 }}, ptr {}, i32 0, i32 {index}",
                    value(buffer)
                ));
                self.line(format!(
                    "  {} = load i64, ptr {field}, align 8",
                    value(output)
                ));
            }
            InstructionKind::ByteBufferBorrow { buffer, .. } => {
                let pointer = self.temp();
                let length_field = self.temp();
                let length = self.temp();
                let pair = self.temp();
                self.line(format!(
                    "  {pointer} = load ptr, ptr {}, align 8",
                    value(buffer)
                ));
                self.line(format!(
                    "  {length_field} = getelementptr {{ ptr, i64, i64 }}, ptr {}, i32 0, i32 1",
                    value(buffer)
                ));
                self.line(format!(
                    "  {length} = load i64, ptr {length_field}, align 8"
                ));
                self.line(format!(
                    "  {pair} = insertvalue {{ ptr, i64 }} poison, ptr {pointer}, 0"
                ));
                self.line(format!(
                    "  {} = insertvalue {{ ptr, i64 }} {pair}, i64 {length}, 1",
                    value(output)
                ));
            }
            _ => {
                return invalid(
                    "E-CODEGEN-CFG",
                    "expected byte-storage primitive",
                    Some(instruction.span),
                );
            }
        }
        Ok(())
    }
}
