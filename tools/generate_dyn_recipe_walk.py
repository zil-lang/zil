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
    out = header + "enum Work<'a> {\n"
    for t, k in types.items():
        out += k + "(&'a " + t + ',usize),\n'
    out += '}\nfn visit<V: DynRecipeVisitor>(first: Work<\'_>, visitor: &mut V) -> Result<(),V::Error> { let mut pending=vec![first]; while let Some(next)=pending.pop() { match next {\n'
    hooks = {'Type':'ty','StorageLifeShape':'life','StoragePosition':'position','LifeDeclarationIdentity':'identity','Constant':'constant','String':'text'}
    for t, k in types.items():
        out += 'Work::' + k + '(value,nesting) => {\n'
        if t.startswith('Vec<'):
            out += 'visitor.rows(value.len(),nesting)?;\nfor item in value.iter().rev() { pending.push(Work::' + key(t[4:-1]) + '(item,nesting)); }\n'
        elif t.startswith('Option<'):
            out += 'visitor.node(nesting)?;\nif let Some(item)=value { pending.push(Work::' + key(t[7:-1]) + '(item,nesting)); }\n'
        elif (parts:=tuple_parts(t)) is not None:
            out += 'visitor.node(nesting)?;\n'
            for index, part in reversed(list(enumerate(parts))):
                out += 'pending.push(Work::' + key(part) + '(&value.' + str(index) + ',nesting+1));\n'
        elif t in items:
            kind, rows = items[t]
            out += 'visitor.node(nesting)?;\n'
            def children(fs, is_struct):
                return '\n'.join('pending.push(Work::' + key(ft) + '(' + ('&value.' + n if is_struct else 'field_' + n) + ',nesting+1));' for n, ft in reversed(fs))
            if kind == 'struct':
                out += children(rows, True)
            else:
                out += 'match value {\n'
                for n, shape, fs in rows:
                    pattern = t + '::' + n + (' { ' + ', '.join(n + ': field_' + n for n, _ in fs) + ' }' if shape == 'struct' else '(' + ', '.join('field_' + n for n, _ in fs) + ')' if shape == 'tuple' else '')
                    out += pattern + ' => {\n' + children(fs, False) + '\n},\n'
                out += '}\n'
        elif t in hooks:
            out += 'visitor.' + hooks[t] + '(value,nesting)?;\n'
        else:
            out += 'let _=value;visitor.node(nesting)?;\n'
        out += '},\n'
    out += '} } Ok(()) }\n'
    for name in items:
        out += 'impl DynRecipeMetadata for ' + name + ' { fn visit_dyn_metadata<V: DynRecipeVisitor>(&self, visitor: &mut V, nesting: usize) -> Result<(),V::Error> { visit(Work::' + key(name) + '(self,nesting),visitor) } }\n'
    Path(root, 'dynamic_recipes_visit.rs').write_text(out)
