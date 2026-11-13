"""Generate an explicit-stack codec for the finite public recipe grammar."""
import re
from pathlib import Path
from dyn_recipe_model import canonical_type, tuple_parts

def generate(items, existing, gen, snake, root):
    def norm(t):
        t = canonical_type(t)
        while t.startswith('Box<'):
            t = t[4:-1]
        return t
    types = {}
    def register(t):
        t = norm(t)
        if t in types:
            return
        types[t] = 'K' + str(len(types))
        if t.startswith('Vec<'):
            register(t[4:-1])
        elif t.startswith('Option<'):
            register(t[7:-1])
        elif (parts := tuple_parts(t)) is not None:
            for part in parts:
                register(part)
    for name, (kind, values) in items.items():
        register(name)
        rows = values if kind == 'struct' else [f for _, _, fs in values for f in fs]
        for _, t in rows:
            register(t)
    def key(t):
        return types[norm(t)]
    def extract(t, e):
        t = canonical_type(t)
        if t.startswith('Box<'):
            return 'Box::new(' + extract(t[4:-1], e) + ')'
        if t.startswith('Vec<'):
            return 'match ' + e + ' { Value::Rows(rows) => rows.into_iter().map(|item| ' + extract(t[4:-1], 'item') + ').collect(), _ => unreachable!("recipe row grammar") }'
        if t.startswith('Option<'):
            return 'match ' + e + ' { Value::Optional(value) => value.map(|item| ' + extract(t[7:-1], '*item') + '), _ => unreachable!("recipe optional grammar") }'
        if (parts := tuple_parts(t)) is not None:
            expressions = [extract(part, 'fields.next().expect("tuple field")') for part in parts]
            return 'match ' + e + ' { Value::Tuple(rows) => { let mut fields=rows.into_iter(); (' + ', '.join(expressions) + (',' if len(parts)==1 else '') + ') }, _ => unreachable!("recipe tuple grammar") }'
        payload = 'value' if t == 'String' else '*value'
        return 'match ' + e + ' { Value::' + key(t) + '(value) => ' + payload + ', _ => unreachable!("recipe field grammar") }'
    def fields(fs, base):
        return '\n'.join('pending.push(Write::' + key(t) + '(' + (('&value.' + n) if base else ('field_' + n)) + ', depth + 1));' for n, t in reversed(fs))
    others = '\n'.join(p.read_text() for p in Path(root).glob('*.rs') if p.name != 'dynamic_recipes_wire.rs')
    roots = [n for n in items if n != 'StoredBorrowMode' and re.search(r'\.\s*' + snake(n) + r'\s*\(', others)]
    out = '//! Bounded recipe transport using explicit grammar stacks.\nuse super::*;\n'
    out += "enum Write<'a> {\n"
    for t, k in types.items():
        out += k + "(&'a " + t + ', usize),\n'
    out += '}\n#[derive(Clone,Copy)]\nenum Kind {\n' + ',\n'.join(types.values()) + '}\n'
    out += 'enum Read { Start(Kind,usize), Finish(Kind,u8,usize) }\nenum Value { Rows(Vec<Value>), Tuple(Vec<Value>), Optional(Option<Box<Value>>),\n'
    for t, k in types.items():
        if not t.startswith(('Vec<', 'Option<')) and tuple_parts(t) is None:
            out += k + ('(String),\n' if t == 'String' else '(Box<' + t + '>),\n')
    out += '}\nimpl Writer<\'_> {\n'
    out += 'fn dyn_recipe_node(&mut self, depth: usize) -> WireResult<()> { if depth >= self.limits.max_call_depth.min(128) {return Err(quota("dynamic recipe depth exceeded"));} self.charge(1) }\n'
    for n in roots:
        out += 'pub(super) fn ' + snake(n) + '(&mut self, value: &' + n + ', depth: usize) -> WireResult<()> { self.recipe_write(Write::' + key(n) + '(value,depth)) }\n'
    out += 'fn recipe_write(&mut self, first: Write<\'_>) -> WireResult<()> { let mut pending = vec![first]; while let Some(next) = pending.pop() { match next {\n'
    for t, k in types.items():
        out += 'Write::' + k + '(value, depth) => {\n'
        if t.startswith('Vec<'):
            out += 'self.count(value.len(),self.limits.max_nodes)?;\nfor item in value.iter().rev() { pending.push(Write::' + key(t[4:-1]) + '(item,depth)); }\n'
        elif t.startswith('Option<'):
            out += 'self.boolean(value.is_some())?;\nif let Some(item) = value { pending.push(Write::' + key(t[7:-1]) + '(item,depth)); }\n'
        elif (parts := tuple_parts(t)) is not None:
            out += 'self.dyn_recipe_node(depth)?;\n'
            for index, part in reversed(list(enumerate(parts))):
                out += 'pending.push(Write::' + key(part) + '(&value.' + str(index) + ', depth+1));\n'
        elif t in items:
            kind, values = items[t]
            out += 'self.dyn_recipe_node(depth)?;\n'
            if kind == 'struct':
                out += fields(values, True)
            else:
                out += 'match value {\n'
                for tag, (n, shape, fs) in enumerate(values):
                    pat = t + '::' + n + (' { ' + ', '.join(n + ': field_' + n for n, _ in fs) + ' }' if shape == 'struct' else '(' + ', '.join('field_' + n for n, _ in fs) + ')' if shape == 'tuple' else '')
                    out += pat + ' => { self.u8(' + str(tag) + ')?;\n' + fields(fs, False) + '\n},\n'
                out += '}\n'
        else:
            out += 'let _ = depth;\n' + gen(t, 'value', 'w').replace('depth + 1', 'depth')
        out += '\n},\n'
    out += '} } Ok(()) }\n}\nimpl Reader<\'_> {\n'
    out += 'fn dyn_recipe_node(&mut self, depth: usize) -> WireResult<()> { if depth >= self.limits.max_call_depth.min(128) {return Err(quota("dynamic recipe depth exceeded"));} self.charge(1) }\n'
    for n in roots:
        out += 'pub(super) fn ' + snake(n) + '(&mut self, depth: usize) -> WireResult<' + n + '> { let value=self.recipe_read(Kind::' + key(n) + ',depth)?; Ok(' + extract(n, 'value') + ') }\n'
    out += 'fn recipe_read(&mut self, first: Kind, depth: usize) -> WireResult<Value> { let mut pending = vec![Read::Start(first,depth)]; let mut values = Vec::new(); while let Some(next) = pending.pop() { match next { Read::Start(kind,depth) => match kind {\n'
    for t, k in types.items():
        out += 'Kind::' + k + ' => {\n'
        if t.startswith('Vec<'):
            out += 'let count=self.count(self.limits.max_nodes,1)?; pending.push(Read::Finish(kind,0,count)); for _ in 0..count { pending.push(Read::Start(Kind::' + key(t[4:-1]) + ',depth)); }\n'
        elif t.startswith('Option<'):
            out += 'if self.boolean()? { pending.push(Read::Finish(kind,0,1)); pending.push(Read::Start(Kind::' + key(t[7:-1]) + ',depth)); } else { values.push(Value::Optional(None)); }\n'
        elif (parts := tuple_parts(t)) is not None:
            out += 'self.dyn_recipe_node(depth)?; pending.push(Read::Finish(kind,0,' + str(len(parts)) + '));\n'
            for part in reversed(parts):
                out += 'pending.push(Read::Start(Kind::' + key(part) + ',depth+1));\n'
        elif t in items:
            shape, rows = items[t]
            out += 'self.dyn_recipe_node(depth)?;\n'
            variants = [(0, rows)] if shape == 'struct' else [(i, fs) for i, (_, _, fs) in enumerate(rows)]
            if shape == 'enum':
                out += 'match self.u8()? {\n'
            for tag, fs in variants:
                if shape == 'enum':
                    out += str(tag) + ' => {\n'
                out += 'pending.push(Read::Finish(kind,' + str(tag) + ',' + str(len(fs)) + '));\n'
                out += '\n'.join('pending.push(Read::Start(Kind::' + key(ft) + ',depth+1));' for _, ft in reversed(fs)) + '\n'
                if shape == 'enum':
                    out += '},\n'
            if shape == 'enum':
                out += '_ => return Err(malformed("E-WIRE-TAG","unknown dynamic recipe tag")), }\n'
        else:
            payload = gen(t, '', 'r').replace('depth + 1', 'depth')
            if t != 'String': payload = 'Box::new(' + payload + ')'
            out += 'let _ = depth;\nvalues.push(Value::' + k + '(' + payload + '));\n'
        out += '},\n'
    out += '}, Read::Finish(kind,tag,count) => { let at=values.len().checked_sub(count).expect("recipe grammar arity"); let mut fields=values.split_off(at).into_iter(); let value = match kind {\n'
    for t, k in types.items():
        if not (t.startswith(('Vec<', 'Option<')) or tuple_parts(t) is not None or t in items):
            continue
        out += 'Kind::' + k + ' => {\n'
        if t.startswith('Vec<'):
            out += 'let _=tag; Value::Rows(fields.collect())\n'
        elif t.startswith('Option<'):
            out += 'let _=tag; Value::Optional(Some(Box::new(fields.next().expect("optional field"))))\n'
        elif tuple_parts(t) is not None:
            out += 'let _=tag; Value::Tuple(fields.collect())\n'
        else:
            shape, rows = items[t]
            def record(n, form, fs):
                args = [(name, extract(ft, 'fields.next().expect("recipe field")')) for name, ft in fs]
                return n + (' { ' + ', '.join(name + ': ' + e for name, e in args) + ' }' if form == 'struct' else '(' + ', '.join(e for _, e in args) + ')' if form == 'tuple' else '')
            if shape == 'struct':
                out += 'let _=tag; Value::' + k + '(Box::new(' + record(t, 'struct', rows) + '))\n'
            else:
                out += 'Value::' + k + '(Box::new(match tag {\n'
                for tag, (n, form, fs) in enumerate(rows):
                    out += str(tag) + ' => ' + record(t + '::' + n, form, fs) + ',\n'
                out += '_ => unreachable!("validated recipe tag"), }))\n'
        out += '},\n'
    out += '_ => unreachable!("recipe leaf has no assembly"), }; values.push(value); } } } Ok(values.pop().expect("complete recipe grammar")) }\n}\n'
    Path(root, 'dynamic_recipes_wire.rs').write_text(out)
