"""Emit a raw structural visitor with no native recursion or semantic rules."""
from pathlib import Path
from dyn_recipe_model import canonical_type, tuple_parts

def generate(items, root, header):
    types = {}
    def norm(t):
        t=canonical_type(t)
        while t.startswith('Box<'):
            t = t[4:-1]
        return t
    def add(t):
        t = norm(t)
        if t in types:
            return
        types[t] = 'W' + str(len(types))
        if t.startswith('Vec<'):
            add(t[4:-1])
        elif t.startswith('Option<'):
            add(t[7:-1])
        elif (parts:=tuple_parts(t)) is not None:
            for part in parts:
                add(part)
    for name, (kind, rows) in items.items():
        add(name)
        fs = rows if kind == 'struct' else [f for _, _, fs in rows for f in fs]
        for _, t in fs:
            add(t)
    def key(t):
        return types[norm(t)]
    def enqueue(t, expression, depth):
        raw = canonical_type(t)
        out = ''
        while raw.startswith('Box<'):
            raw = raw[4:-1]
            out += f'visitor.allocation(std::mem::size_of::<{raw}>(), {depth})?;\n'
        return out + f'pending.push(Work::{key(t)}({expression},{depth}));\n'
    out = header + "enum Work<'a> {\n"
    for t, k in types.items():
        out += k + "(&'a " + t + ',usize),\n'
    out += '''}
fn reserve_work<'a, V: DynRecipeVisitor>(pending: &mut Vec<Work<'a>>, additional: usize, visitor: &mut V, nesting: usize) -> Result<(), V::Error> {
    let needed = pending.len().saturating_add(additional);
    if needed > pending.capacity() {
        let capacity = pending.capacity().saturating_mul(2).max(needed).max(4);
        let size = std::mem::size_of::<Work<'a>>();
        visitor.scratch(capacity.saturating_add(pending.capacity()).saturating_mul(size), nesting)?;
        pending.reserve_exact(capacity - pending.len());
    }
    Ok(())
}
fn visit<V: DynRecipeVisitor>(first: Work<'_>, visitor: &mut V) -> Result<(), V::Error> {
    let mut pending = Vec::new();
    reserve_work(&mut pending, 1, visitor, 0)?;
    pending.push(first);
    while let Some(next) = pending.pop() { match next {
'''
    hooks = {'Type':'ty','StorageLifeShape':'life','StoragePosition':'position','LifeDeclarationIdentity':'identity','Constant':'constant','String':'text'}
    for t, k in types.items():
        out += 'Work::' + k + '(value,nesting) => {\n'
        if t.startswith('Vec<'):
            out += 'visitor.rows(value.len(),nesting)?;\nvisitor.allocation(std::mem::size_of_val(value.as_slice()),nesting)?;\nreserve_work(&mut pending,value.len(),visitor,nesting)?;\nfor item in value.iter().rev() {\n' + enqueue(t[4:-1], 'item', 'nesting') + '}\n'
        elif t.startswith('Option<'):
            out += 'visitor.node(nesting)?;\nif let Some(item)=value {\nreserve_work(&mut pending,1,visitor,nesting)?;\n' + enqueue(t[7:-1], 'item', 'nesting') + '}\n'
        elif (parts:=tuple_parts(t)) is not None:
            out += 'visitor.node(nesting)?;\n'
            out += f'reserve_work(&mut pending,{len(parts)},visitor,nesting)?;\n'
            for index, part in reversed(list(enumerate(parts))):
                out += enqueue(part, '&value.' + str(index), 'nesting+1')
        elif t in items:
            kind, rows = items[t]
            out += 'visitor.node(nesting)?;\n'
            def children(fs, is_struct):
                return f'reserve_work(&mut pending,{len(fs)},visitor,nesting)?;\n' + ''.join(enqueue(ft, '&value.' + n if is_struct else 'field_' + n, 'nesting+1') for n, ft in reversed(fs))
            if kind == 'struct':
                out += children(rows, True)
            else:
                out += 'match value {\n'
                for n, shape, fs in rows:
                    pattern = t + '::' + n + (' { ' + ', '.join(n + ': field_' + n for n, _ in fs) + ' }' if shape == 'struct' else '(' + ', '.join('field_' + n for n, _ in fs) + ')' if shape == 'tuple' else '')
                    out += pattern + ' => {\n' + children(fs, False) + '\n},\n'
                out += '}\n'
        elif t in hooks:
            if t == 'String':
                out += 'visitor.allocation(value.len(),nesting)?;\n'
            out += 'visitor.' + hooks[t] + '(value,nesting)?;\n'
        else:
            out += 'let _=value;visitor.node(nesting)?;\n'
        out += '},\n'
    out += '} } Ok(()) }\n'
    for name in items:
        out += 'impl DynRecipeMetadata for ' + name + ' { fn visit_dyn_metadata<V: DynRecipeVisitor>(&self, visitor: &mut V, nesting: usize) -> Result<(),V::Error> { visit(Work::' + key(name) + '(self,nesting),visitor) } }\n'
    Path(root, 'dynamic_recipes_visit.rs').write_text(out)
