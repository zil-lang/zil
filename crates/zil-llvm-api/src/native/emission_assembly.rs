//! Reconstitute omitted, independently addressed ELF leaf definitions.
//!
//! The caller proves equality of the CURRENT optimized IR and retains the
//! original emission fallback. This parser only transports a narrow LLVM
//! x86-64 AT&T assembly shape; it is not an IR equality or ABI proof.
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_FUNCTIONS: usize = 65_536;
const LOCAL_PREFIX: &str = ".Lzil_assembly_clone_";

struct Line<'a> {
    raw: &'a str,
    start: usize,
    end: usize,
    text: bool,
}
struct Function<'a> {
    name: &'a str,
    bytes: Range<usize>,
    text_at_start: bool,
    text_at_end: bool,
}
enum Replacement {
    Function,
    Local(usize),
}
struct Template<'a> {
    raw: &'a str,
    replacements: Vec<(Range<usize>, Replacement)>,
}

fn code(line: &str) -> &str {
    line.split('#').next().unwrap_or_default().trim()
}
fn directive(line: &str) -> Option<(&str, &str)> {
    let line = code(line);
    if !line.starts_with('.') || line.ends_with(':') {
        return None;
    }
    let end = line.find(char::is_whitespace).unwrap_or(line.len());
    Some((&line[..end], line[end..].trim()))
}
fn symbol(name: &str) -> bool {
    let mut chars = name.bytes();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'$'))
}
fn function_type(line: &str) -> Option<&str> {
    let (op, args) = directive(line)?;
    let (name, kind) = args.split_once(',')?;
    (op == ".type" && kind.trim() == "@function" && symbol(name.trim())).then_some(name.trim())
}
fn named_directive<'a>(line: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let (actual, args) = directive(line)?;
    if actual != op {
        return None;
    }
    let (name, rest) = args.split_once(',')?;
    Some((name.trim(), rest.trim()))
}
fn header(line: &str, name: &str) -> bool {
    match directive(line) {
        Some((".globl" | ".global" | ".local" | ".weak" | ".hidden" | ".protected", arg)) => {
            arg == name
        }
        Some((".p2align" | ".prefalign" | ".balign" | ".align", _)) => true,
        _ => false,
    }
}

fn parse(assembly: &str) -> Option<Vec<Function<'_>>> {
    let mut lines = Vec::new();
    let (mut offset, mut text) = (0, false);
    for raw in assembly.split_inclusive('\n') {
        lines.push(Line {
            raw,
            start: offset,
            end: offset + raw.len(),
            text,
        });
        offset += raw.len();
        if let Some((op, args)) = directive(raw) {
            // These can manufacture hidden definitions, change syntax, or
            // expand text which a line/symbol boundary parser cannot observe.
            if op.starts_with(".if")
                || matches!(
                    op,
                    ".else"
                        | ".elseif"
                        | ".endif"
                        | ".macro"
                        | ".endm"
                        | ".rept"
                        | ".endr"
                        | ".irp"
                        | ".irpc"
                        | ".include"
                        | ".incbin"
                        | ".altmacro"
                        | ".noaltmacro"
                        | ".set"
                        | ".equ"
                        | ".equiv"
                        | ".weakref"
                        | ".symver"
                        | ".intel_syntax"
                        | ".code16"
                        | ".code32"
                        | ".code64"
                        | ".arch"
                        | ".cpu"
                )
            {
                return None;
            }
            if op == ".att_syntax" && !args.is_empty() {
                return None;
            }
            match op {
                ".text" => text = args.is_empty(),
                ".section" | ".pushsection" | ".popsection" | ".previous" | ".subsection"
                | ".data" | ".bss" | ".rodata" => text = false,
                _ => {}
            }
        }
    }
    let mut functions = Vec::new();
    let mut names = BTreeSet::new();
    let (mut i, mut previous_end) = (0, 0);
    while i < lines.len() {
        let Some(name) = function_type(lines[i].raw) else {
            i += 1;
            continue;
        };
        if !names.insert(name) || functions.len() == MAX_FUNCTIONS {
            return None;
        }
        let mut start = i;
        while start > previous_end
            && (code(lines[start - 1].raw).is_empty() || header(lines[start - 1].raw, name))
        {
            start -= 1;
        }
        let mut label = i + 1;
        while label < lines.len() && code(lines[label].raw).strip_suffix(':') != Some(name) {
            if !code(lines[label].raw).is_empty() && !header(lines[label].raw, name) {
                return None;
            }
            label += 1;
        }
        if label == lines.len() {
            return None;
        }
        let (mut end, mut cfi) = (label + 1, false);
        loop {
            let line = lines.get(end)?;
            if function_type(line.raw).is_some() {
                return None;
            }
            if let Some((op, _)) = directive(line.raw) {
                match op {
                    ".cfi_startproc" if !cfi => cfi = true,
                    ".cfi_endproc" if cfi => cfi = false,
                    ".cfi_startproc" | ".cfi_endproc" => return None,
                    _ => {}
                }
            }
            if let Some((owner, _)) = named_directive(line.raw, ".size") {
                if owner != name {
                    return None;
                }
                break;
            }
            end += 1;
        }
        end += 1;
        // LLVM may put .cfi_endproc AFTER .size. Never insert a clone into
        // that still-open ordinary function's unwind region.
        if cfi {
            while lines.get(end).is_some_and(|line| code(line.raw).is_empty()) {
                end += 1;
            }
            if directive(lines.get(end)?.raw) != Some((".cfi_endproc", "")) {
                return None;
            }
            end += 1;
        }
        let text_at_end = lines.get(end).map_or(text, |line| line.text);
        functions.push(Function {
            name,
            bytes: lines[start].start..lines[end - 1].end,
            text_at_start: lines[start].text,
            text_at_end,
        });
        previous_end = end;
        i = end;
    }
    Some(functions)
}

fn bb(name: &str) -> bool {
    name.strip_prefix(".LBB")
        .and_then(|s| s.split_once('_'))
        .is_some_and(|(a, b)| {
            !a.is_empty()
                && !b.is_empty()
                && a.bytes().all(|c| c.is_ascii_digit())
                && b.bytes().all(|c| c.is_ascii_digit())
        })
}
fn end_label(name: &str) -> bool {
    name.strip_prefix(".Lfunc_end")
        .is_some_and(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
}
fn alignment(args: &str) -> bool {
    let fields = args.split(',').map(str::trim).collect::<Vec<_>>();
    !fields.is_empty()
        && fields.len() <= 3
        && fields.iter().enumerate().all(|(i, field)| {
            let n = if let Some(hex) = field.strip_prefix("0x") {
                u32::from_str_radix(hex, 16).ok()
            } else {
                field.parse::<u32>().ok()
            };
            n.is_some_and(|n| if i == 0 { n <= 6 } else { n <= 255 })
        })
}
fn register(name: &str) -> bool {
    matches!(
        name,
        "rax"
            | "eax"
            | "ax"
            | "al"
            | "ah"
            | "rbx"
            | "ebx"
            | "bx"
            | "bl"
            | "bh"
            | "rcx"
            | "ecx"
            | "cx"
            | "cl"
            | "ch"
            | "rdx"
            | "edx"
            | "dx"
            | "dl"
            | "dh"
            | "rsi"
            | "esi"
            | "si"
            | "sil"
            | "rdi"
            | "edi"
            | "di"
            | "dil"
            | "rsp"
            | "esp"
            | "sp"
            | "spl"
            | "rbp"
            | "ebp"
            | "bp"
            | "bpl"
    ) || name.strip_prefix('r').is_some_and(|s| {
        let n = s.strip_suffix(['d', 'w', 'b']).unwrap_or(s);
        matches!(n, "8" | "9" | "10" | "11" | "12" | "13" | "14" | "15")
    })
}
fn condition(s: &str) -> bool {
    matches!(
        s,
        "e" | "z"
            | "ne"
            | "nz"
            | "s"
            | "ns"
            | "g"
            | "ge"
            | "l"
            | "le"
            | "a"
            | "ae"
            | "b"
            | "be"
            | "o"
            | "no"
            | "p"
            | "np"
            | "pe"
            | "po"
            | "c"
            | "nc"
            | "na"
            | "nae"
            | "nb"
            | "nbe"
            | "ng"
            | "nge"
            | "nl"
            | "nle"
    )
}
fn ordinary(op: &str) -> bool {
    let base = op.strip_suffix(['b', 'w', 'l', 'q']).unwrap_or(op);
    matches!(
        base,
        "mov"
            | "movabs"
            | "lea"
            | "add"
            | "sub"
            | "imul"
            | "mul"
            | "idiv"
            | "div"
            | "and"
            | "or"
            | "xor"
            | "not"
            | "neg"
            | "inc"
            | "dec"
            | "shl"
            | "sal"
            | "shr"
            | "sar"
            | "rol"
            | "ror"
            | "shld"
            | "shrd"
            | "cmp"
            | "test"
            | "push"
            | "pop"
            | "bsf"
            | "bsr"
            | "bt"
            | "bts"
            | "btr"
            | "btc"
            | "nop"
            | "leave"
    ) || matches!(
        op,
        "movsbl"
            | "movsbw"
            | "movsbq"
            | "movswl"
            | "movswq"
            | "movslq"
            | "movzbl"
            | "movzbw"
            | "movzbq"
            | "movzwl"
            | "movzwq"
            | "cbtw"
            | "cwtl"
            | "cltd"
            | "cltq"
            | "cqto"
            | "ud2"
    ) || op.strip_prefix("set").is_some_and(condition)
        || op
            .strip_prefix("cmov")
            .is_some_and(|s| condition(s.strip_suffix(['w', 'l', 'q']).unwrap_or(s)))
}

// Complete assembler tokens, never substring substitution. Register tokens
// include their '%' so a function named 'eax' cannot rename a register.
fn tokens(code: &str) -> Vec<Range<usize>> {
    let bytes = code.as_bytes();
    let (mut i, mut out) = (0, Vec::new());
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_hexdigit() || bytes[i] == b'x') {
                i += 1;
            }
        } else if bytes[i].is_ascii_alphabetic() || matches!(bytes[i], b'_' | b'.' | b'%') {
            let start = i;
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'.' | b'$'))
            {
                i += 1;
            }
            out.push(start..i);
        } else {
            i += 1;
        }
    }
    out
}
fn instruction(code: &str, locals: &BTreeMap<&str, usize>) -> bool {
    if !code.is_ascii()
        || code.bytes().any(|c| {
            matches!(
                c,
                b';' | b'"' | b'\'' | b'@' | b':' | b'[' | b']' | b'{' | b'}' | b'\\'
            )
        })
    {
        return false;
    }
    let split = code.find(char::is_whitespace).unwrap_or(code.len());
    let (op, args) = (&code[..split], code[split..].trim());
    if op == "ret" || op == "retq" || op == "ud2" {
        return args.is_empty();
    }
    if op == "jmp" || op == "jmpq" || op.strip_prefix('j').is_some_and(condition) {
        return bb(args) && locals.contains_key(args);
    }
    if !ordinary(op) {
        return false;
    }
    for token in tokens(args) {
        let value = &args[token];
        if !value.strip_prefix('%').is_some_and(register) {
            return false;
        }
    }
    // Memory identities may only be the current stack frame. LEA/NOP do not
    // dereference memory; RIP, segment and symbolic references still reject.
    if !op.starts_with("lea") && !op.starts_with("nop") {
        let mut tail = args;
        while let Some((_, rest)) = tail.split_once('(') {
            let Some((address, after)) = rest.split_once(')') else {
                return false;
            };
            if address.trim() != "%rsp" && address.trim() != "%rbp" {
                return false;
            }
            tail = after;
        }
    }
    true
}

impl<'a> Template<'a> {
    fn new(assembly: &'a str, function: &Function<'_>) -> Option<Self> {
        if !function.text_at_start || !function.text_at_end {
            return None;
        }
        let raw = &assembly[function.bytes.clone()];
        let mut locals = BTreeMap::new();
        let mut end = None;
        for line in raw.lines() {
            if let Some(label) = code(line).strip_suffix(':') {
                if label == function.name {
                    continue;
                }
                if !bb(label) && !end_label(label) {
                    return None;
                }
                let index = locals.len();
                if locals.insert(label, index).is_some() {
                    return None;
                }
                if end_label(label) && end.replace(label).is_some() {
                    return None;
                }
            }
        }
        let end = end?;
        let (mut owner, mut typed, mut global, mut sized) = (false, false, false, false);
        let (mut ended, mut terminal, mut instruction_count) = (false, false, 0usize);
        let mut replacements = Vec::new();
        let mut offset = 0;
        for line in raw.split_inclusive('\n') {
            let text = code(line);
            if text.is_empty() {
                offset += line.len();
                continue;
            }
            if !text.is_ascii() || text.contains(['"', '\'', ';', '\\']) {
                return None;
            }
            let is_label = text.strip_suffix(':');
            let rewrite_owner;
            if let Some(label) = is_label {
                if sized || ended {
                    return None;
                }
                if label == function.name {
                    if owner || !typed || !global {
                        return None;
                    }
                    owner = true;
                } else if !owner || !locals.contains_key(label) {
                    return None;
                }
                if label == end {
                    ended = true;
                }
                rewrite_owner = label == function.name;
            } else if let Some((op, args)) = directive(text) {
                match op {
                    ".globl" | ".global" if !owner && !global && args == function.name => {
                        global = true;
                    }
                    ".type" if !owner && !typed && function_type(text) == Some(function.name) => {
                        typed = true;
                    }
                    ".p2align" if !ended && alignment(args) => {}
                    ".prefalign" if !owner => {
                        let fields = args.split(',').map(str::trim).collect::<Vec<_>>();
                        if fields.len() != 3
                            || !alignment(fields[0])
                            || fields[1] != end
                            || fields[2] != "nop"
                        {
                            return None;
                        }
                    }
                    ".size" if ended && !sized => {
                        let (name, expression) = named_directive(text, ".size")?;
                        let expression = expression
                            .chars()
                            .filter(|c| !c.is_whitespace())
                            .collect::<String>();
                        if name != function.name || expression != format!("{end}-{}", function.name)
                        {
                            return None;
                        }
                        sized = true;
                    }
                    _ => return None, // CFI/EH/data/sections/unknown directives.
                }
                rewrite_owner = matches!(op, ".globl" | ".global" | ".type" | ".size");
            } else {
                if !owner || ended || sized || !instruction(text, &locals) {
                    return None;
                }
                instruction_count += 1;
                let op = text.split_whitespace().next()?;
                terminal = matches!(op, "ret" | "retq" | "ud2" | "jmp" | "jmpq");
                rewrite_owner = false;
            }
            let start = line.find(text)?;
            for token in tokens(text) {
                let name = &text[token.clone()];
                let replacement = if rewrite_owner
                    && name == function.name
                    && !(token.start > 0 && text.as_bytes()[token.start - 1] == b'@')
                {
                    Some(Replacement::Function)
                } else {
                    locals.get(name).copied().map(Replacement::Local)
                };
                if let Some(replacement) = replacement {
                    replacements.push((
                        offset + start + token.start..offset + start + token.end,
                        replacement,
                    ));
                }
            }
            offset += line.len();
        }
        if !owner || !typed || !global || !sized || !terminal || instruction_count == 0 {
            return None;
        }
        Some(Self { raw, replacements })
    }
    fn append(&self, name: &str, ordinal: usize, output: &mut Vec<u8>) -> Option<()> {
        let mut cursor = 0;
        for (range, replacement) in &self.replacements {
            append(output, &self.raw.as_bytes()[cursor..range.start])?;
            match replacement {
                Replacement::Function => append(output, name.as_bytes())?,
                Replacement::Local(index) => append(
                    output,
                    format!("{LOCAL_PREFIX}{ordinal}_{index}").as_bytes(),
                )?,
            }
            cursor = range.end;
        }
        append(output, &self.raw.as_bytes()[cursor..])
    }
}
fn append(output: &mut Vec<u8>, bytes: &[u8]) -> Option<()> {
    if output.len().checked_add(bytes.len())? > MAX_BYTES {
        return None;
    }
    output.extend_from_slice(bytes);
    Some(())
}

// The empty module assembler emits its own source-file record. Remove ONLY
// the original unnumbered module record; numbered debug files stay untouched.
// Reject misplaced or repeated module records rather than change debug scope.
fn module_file(assembly: &str, first_function: usize) -> Option<Option<Range<usize>>> {
    let (mut offset, mut found) = (0, None);
    for raw in assembly.split_inclusive('\n') {
        let line = raw.trim();
        if line
            .strip_prefix(".file")
            .is_some_and(|tail| tail.starts_with(char::is_whitespace))
        {
            let args = line[5..].trim();
            if args.starts_with('"') {
                if found.is_some() || offset + raw.len() > first_function {
                    return None;
                }
                let mut escaped = false;
                let mut closing = None;
                for (i, ch) in args.char_indices().skip(1) {
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == '"' {
                        closing = Some(i);
                        break;
                    }
                }
                let trailing = args.get(closing? + 1..)?.trim();
                if !trailing.is_empty() && !trailing.starts_with('#') {
                    return None;
                }
                found = Some(offset..offset + raw.len());
            } else if !args.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                return None;
            }
        }
        offset += raw.len();
    }
    Some(found)
}

/// `aliases` maps omitted definitions to independently proved current owners.
/// All original functions must appear exactly once, in current IR order.
/// Unknown syntax or any incomplete binding declines the WHOLE reconstruction.
pub(super) fn reconstruct(
    assembly: &[u8],
    aliases: &BTreeMap<String, String>,
    definition_order: &[String],
) -> Option<Vec<u8>> {
    if assembly.len() > MAX_BYTES || definition_order.len() > MAX_FUNCTIONS {
        return None;
    }
    let text = std::str::from_utf8(assembly).ok()?;
    if aliases.is_empty() {
        let file = module_file(text, text.len())?;
        let mut output = Vec::with_capacity(assembly.len());
        if let Some(file) = file {
            append(&mut output, &assembly[..file.start])?;
            append(&mut output, &assembly[file.end..])?;
        } else {
            append(&mut output, assembly)?;
        }
        return Some(output);
    }
    if !text.ends_with('\n') || text.contains(LOCAL_PREFIX) || text.contains('\0') {
        return None;
    }
    let order = definition_order
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if order.len() != definition_order.len() || order.iter().any(|name| !symbol(name)) {
        return None;
    }
    for (name, canonical) in aliases {
        if name == canonical
            || !order.contains(name.as_str())
            || !order.contains(canonical.as_str())
            || aliases.contains_key(canonical)
        {
            return None;
        }
    }
    let functions = parse(text)?;
    let file = module_file(text, functions.first()?.bytes.start)?;
    let expected = definition_order
        .iter()
        .filter(|name| !aliases.contains_key(name.as_str()))
        .map(String::as_str)
        .collect::<Vec<_>>();
    if functions.iter().map(|f| f.name).collect::<Vec<_>>() != expected {
        return None;
    }
    // Reject duplicate definitions outside parsed function bodies, too.
    for line in text.lines() {
        if let Some(label) = code(line).strip_suffix(':')
            && aliases.contains_key(label)
        {
            return None;
        }
    }
    let mut templates = BTreeMap::new();
    for canonical in aliases.values() {
        if templates.contains_key(canonical.as_str()) {
            continue;
        }
        let function = functions.iter().find(|f| f.name == canonical)?;
        templates.insert(canonical.as_str(), Template::new(text, function)?);
    }
    let (mut cursor, mut ordinal) = (0, 0);
    let mut output = Vec::with_capacity(assembly.len());
    for function in &functions {
        if cursor == 0 {
            if let Some(file) = &file {
                append(&mut output, &assembly[..file.start])?;
                append(&mut output, &assembly[file.end..function.bytes.start])?;
            } else {
                append(&mut output, &assembly[..function.bytes.start])?;
            }
        } else {
            append(&mut output, &assembly[cursor..function.bytes.start])?;
        }
        while let Some(name) = definition_order
            .get(ordinal)
            .filter(|name| aliases.contains_key(name.as_str()))
        {
            if !function.text_at_start {
                return None;
            }
            templates
                .get(aliases.get(name)?.as_str())?
                .append(name, ordinal, &mut output)?;
            ordinal += 1;
        }
        if definition_order.get(ordinal).map(String::as_str) != Some(function.name) {
            return None;
        }
        append(&mut output, &assembly[function.bytes.clone()])?;
        cursor = function.bytes.end;
        ordinal += 1;
        while let Some(name) = definition_order
            .get(ordinal)
            .filter(|name| aliases.contains_key(name.as_str()))
        {
            if !function.text_at_end {
                return None;
            }
            templates
                .get(aliases.get(name)?.as_str())?
                .append(name, ordinal, &mut output)?;
            ordinal += 1;
        }
    }
    if ordinal != definition_order.len() {
        return None;
    }
    append(&mut output, &assembly[cursor..])?;
    Some(output)
}

#[cfg(test)]
#[path = "emission_assembly_tests.rs"]
mod tests;
