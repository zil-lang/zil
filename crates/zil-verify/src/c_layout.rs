//! Bounded, independent by-address C payload admission and natural x64 layout.
//! Layout facts do not establish the truth of an external declaration.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Layout {
    pub size: u64,
    pub alignment: u64,
}

pub(super) fn payload(
    ty: &Type,
    mutable: bool,
    callbacks: bool,
    target: &TargetProfile,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<Option<Layout>> {
    enum Work<'a> {
        Visit(&'a Type),
        Array(u32),
        Record(usize),
    }
    let mut work = vec![Work::Visit(ty)];
    let mut layouts = Vec::<Layout>::new();
    let overflow = || {
        reject(
            "E-VERIFY-FOREIGN",
            span,
            "C payload natural layout exceeds the addressable target size",
        )
    };
    while let Some(item) = work.pop() {
        budget.spend(1)?;
        match item {
            Work::Visit(ty) => {
                let scalar = match ty {
                    Type::U8 => Some((1, 1)),
                    Type::U16 => Some((2, 2)),
                    Type::I32 => Some((4, 4)),
                    Type::I64 => Some((8, 8)),
                    Type::ForeignHandle => {
                        native_values::target(target, span)?;
                        Some((8, 8))
                    }
                    Type::CWideString if !mutable => {
                        native_values::target(target, span)?;
                        Some((8, 8))
                    }
                    Type::ForeignCallback { parameters, result } if !mutable && callbacks => {
                        native_values::target(target, span)?;
                        budget.spend(parameters.len().saturating_add(1))?;
                        native_values::callback_signature(parameters, result, span)?;
                        Some((8, 8))
                    }
                    Type::Array { element, length } if *length != 0 => {
                        work.push(Work::Array(*length));
                        work.push(Work::Visit(element));
                        None
                    }
                    Type::Record { fields, .. } if !fields.is_empty() => {
                        native_values::target(target, span)?;
                        budget.spend(fields.len())?;
                        work.push(Work::Record(fields.len()));
                        work.extend(fields.iter().rev().map(|(_, ty)| Work::Visit(ty)));
                        None
                    }
                    _ => return Ok(None),
                };
                if let Some((size, alignment)) = scalar {
                    layouts.push(Layout { size, alignment });
                }
            }
            Work::Array(length) => {
                let item = layouts.last_mut().expect("visited C array element");
                item.size = item
                    .size
                    .checked_mul(u64::from(length))
                    .ok_or_else(overflow)?;
            }
            Work::Record(count) => {
                let first = layouts.len() - count;
                let mut size = 0u64;
                let mut alignment = 1u64;
                for field in &layouts[first..] {
                    budget.spend(1)?;
                    alignment = alignment.max(field.alignment);
                    size = align(size, field.alignment)
                        .and_then(|offset| offset.checked_add(field.size))
                        .ok_or_else(overflow)?;
                }
                size = align(size, alignment).ok_or_else(overflow)?;
                layouts.truncate(first);
                layouts.push(Layout { size, alignment });
            }
        }
        if target.pointer_width == 32
            && layouts
                .last()
                .is_some_and(|layout| layout.size > u64::from(u32::MAX))
        {
            return Err(overflow());
        }
    }
    Ok(layouts.pop())
}

fn align(offset: u64, alignment: u64) -> Option<u64> {
    offset
        .checked_add(alignment - 1)
        .map(|n| n & !(alignment - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(fields: Vec<Type>) -> Type {
        Type::Record {
            name: "LayoutFixture".into(),
            fields: fields
                .into_iter()
                .enumerate()
                .map(|(i, ty)| (format!("f{i}"), ty))
                .collect(),
        }
    }

    #[test]
    fn x64_native_record_sizes_include_internal_and_tail_padding() {
        let point = record(vec![Type::I32, Type::I32]);
        let rect = record(vec![Type::I32; 4]);
        let msg = record(vec![
            Type::ForeignHandle,
            Type::I32,
            Type::I64,
            Type::I64,
            Type::I32,
            point,
            Type::I32,
        ]);
        let paint = record(vec![
            Type::ForeignHandle,
            Type::I32,
            rect,
            Type::I32,
            Type::I32,
            Type::Array {
                element: Box::new(Type::U8),
                length: 32,
            },
        ]);
        let callback = Type::ForeignCallback {
            parameters: vec![Type::ForeignHandle, Type::I32, Type::I64, Type::I64],
            result: Box::new(Type::I64),
        };
        let window_class = record(vec![
            Type::I32,
            Type::I32,
            callback,
            Type::I32,
            Type::I32,
            Type::ForeignHandle,
            Type::ForeignHandle,
            Type::ForeignHandle,
            Type::ForeignHandle,
            Type::CWideString,
            Type::CWideString,
            Type::ForeignHandle,
        ]);
        for target in [
            TargetProfile::host_llvm(),
            TargetProfile::windows_x64_llvm(),
        ] {
            for (ty, size, alignment) in [
                (msg.clone(), 48, 8),
                (paint.clone(), 72, 8),
                (window_class.clone(), 80, 8),
                (record(vec![Type::U8, Type::I64, Type::U8]), 24, 8),
                (
                    Type::Array {
                        element: Box::new(record(vec![Type::U16, Type::U8])),
                        length: 3,
                    },
                    12,
                    2,
                ),
            ] {
                let mut budget = Budget::new(&Limits::default());
                let actual = payload(&ty, false, true, &target, None, &mut budget)
                    .unwrap()
                    .unwrap();
                assert_eq!(actual, Layout { size, alignment });
            }
        }
    }
}
