"""Conservative pure-IR witness checks; no compiler/tool/native launch on import."""
import re
from pathlib import Path

NAME = r'[A-Za-z$._][A-Za-z0-9$._-]*'
SYMBOL = re.compile(r'@(' + NAME + r')')
HEADER = re.compile(r'^(define|declare)\s+.*?@(' + NAME + r')\(')
LABEL = re.compile(r'^(?:[A-Za-z$._][A-Za-z0-9$._-]*|[0-9]+):$')
ASSIGN = re.compile(r'^%(?:' + NAME + r'|[0-9]+)\s*=\s*')
# Exact registered LLVM23 intrinsic spellings already observed in the old O2
# corpus. Full opt verification is mandatory separately; no llvm.* wildcard.
INTRINSICS = {'llvm.trap'} | {
    f'llvm.{op}.with.overflow.i{bits}'
    for op in ['sadd', 'ssub', 'smul', 'uadd', 'usub', 'umul'] for bits in [8, 16, 32, 64]
} | {f'llvm.vector.reduce.add.v{lanes}i{bits}' for lanes in [2, 4] for bits in [16, 32]}

def parse(text):
    defs, declarations, active = {}, set(), None
    for number, raw in enumerate(text.splitlines(), 1):
        line = raw.split(';', 1)[0].strip()
        if not line:
            continue
        if active is None:
            if line.startswith(('define ', 'declare ')):
                match = HEADER.match(line)
                if not match or '@"' in line:
                    raise ValueError(f'unsupported function header at line {number}')
                kind, name = match.groups()
                if SYMBOL.findall(line) != [name] or name in defs or name in declarations:
                    raise ValueError('duplicate/unknown header reference')
                if kind == 'declare':
                    declarations.add(name)
                else:
                    if not line.endswith('{'):
                        raise ValueError('multiline function header unsupported')
                    defs[name] = {'header': line, 'calls': set(), 'blocks': 0, 'instructions': 0,
                                  'available_externally': 'available_externally' in line.split()}
                    active = defs[name]
            elif line.startswith('@') or '@"' in line or SYMBOL.search(line):
                raise ValueError('global/alias/address metadata outside pure scope')
            continue
        if line == '}':
            active = None
            continue
        if LABEL.fullmatch(line):
            active['blocks'] += 1
            continue
        instruction = ASSIGN.sub('', line)
        instruction = re.sub(r'^(?:tail|musttail|notail)\s+', '', instruction)
        opcode = instruction.split(None, 1)[0]
        refs = SYMBOL.findall(line)
        if opcode in ['invoke', 'callbr']:
            raise ValueError('unsupported call control flow')
        if opcode == 'call':
            match = re.search(r'@(' + NAME + r')\(', instruction)
            if not match or refs != [match.group(1)] or '@"' in instruction:
                raise ValueError('indirect or address-bearing call unsupported')
            active['calls'].add(match.group(1))
        elif refs or '@"' in instruction:
            raise ValueError('global/function address operand unsupported')
        active['instructions'] += 1
    if active is not None:
        raise ValueError('unterminated function')
    return {'definitions': defs, 'declarations': declarations}

def export_proof(preflight):
    proof = preflight.get('visibility_proof')
    if not isinstance(proof, dict) or proof.get('format') != 'zil-thinlto-complete-visibility/1':
        raise ValueError('current original-planner proof absent')
    for key in ['current_complete_planner', 'canonical_owners_equal', 'original_required_exactly_allowed']:
        if proof.get(key) is not True:
            raise ValueError('original planner proof flag absent')
    units = proof['units']
    if len(units) != 8 or [v['unit'] for v in units] != list(range(8)):
        raise ValueError('eight exact unique original owner units required')
    owners, allowed, costs = {}, {}, {}
    total = 0
    for unit in units:
        index = unit['unit']
        names = unit['canonical']
        if not names or names != sorted(set(names)):
            raise ValueError('invalid canonical owner set')
        for name in names:
            if name in owners:
                raise ValueError('overlapping canonical owners')
            owners[name] = index
        copied = unit['allowed']
        if [v['name'] for v in copied] != sorted(set(v['name'] for v in copied)):
            raise ValueError('duplicate/unsorted allowed imports')
        allowed[index] = {v['name'] for v in copied}
        if unit['required_original'] != sorted(allowed[index]):
            raise ValueError('original closure did not exactly match actual planned imports')
        for item in copied:
            name, cost = item['name'], item['cost']
            if type(cost) is not int or cost <= 0 or (name in costs and costs[name] != cost):
                raise ValueError('inconsistent original function cost')
            costs[name] = cost
        subtotal = sum(v['cost'] for v in copied)
        if subtotal != unit['copy_cost']:
            raise ValueError('original unit copy cost mismatch')
        total += subtotal
    for index, names in allowed.items():
        if any(name not in owners or owners[name] == index or name == 'main' for name in names):
            raise ValueError('allowed item not an original cross-owner source')
    if total != proof['total_copy_cost'] or total > proof['original_source_cost']:
        raise ValueError('original copy budget mismatch')
    foreign = set(proof['foreign'])
    if not foreign.issubset({'getchar', 'putchar'} | INTRINSICS) or foreign & owners.keys():
        raise ValueError('unknown/conflicting original declaration')
    return proof, owners, allowed, costs, foreign

def match_unit(input_text, proof):
    defs = set(parse(input_text)['definitions'])
    candidates = [u['unit'] for u in proof['units'] if set(u['canonical']) == defs]
    if len(candidates) != 1:
        raise ValueError('actual exported unit differs from canonical ownership proof')
    return candidates[0]

def check_modules(originals, prelinks, imported, preflight):
    proof, owners, allowed, costs, foreign = export_proof(preflight)
    if set(originals) != set(range(8)) or set(prelinks) != set(range(8)) or (imported is not None and set(imported) != set(range(8))):
        raise ValueError('missing/duplicate actual LLVM module')
    all_prelink_defs, original_costs = {}, {}
    valid_external = foreign | INTRINSICS
    for index in range(8):
        original, prelink = originals[index], prelinks[index]
        if set(original['definitions']) != set(proof['units'][index]['canonical']):
            raise ValueError('actual input owner set changed')
        for name, f in original['definitions'].items():
            original_costs[name] = f['blocks'] + f['instructions']
        if not set(prelink['definitions']).issubset(set(original['definitions'])):
            raise ValueError('unknown/new definition introduced during prelink')
        if not prelink['declarations'].issubset(set(owners) | valid_external):
            raise ValueError('unknown prelink declaration')
        for name, function in prelink['definitions'].items():
            if not function['calls'].issubset(set(owners) | valid_external):
                raise ValueError('unknown prelink direct target')
            if name in all_prelink_defs:
                raise ValueError('duplicate actual prelink definition')
            all_prelink_defs[name] = function
    # Machine cost is one per block plus one per instruction. Original renderer
    # emits one instruction per physical line; unsupported forms fail closed.
    source_cost = sum(cost for name, cost in original_costs.items() if name != 'main')
    if source_cost != proof['original_source_cost'] or any(original_costs.get(name) != cost for name, cost in costs.items()):
        raise ValueError('actual original IR and machine copy-cost proof differ')
    required, receipts = {}, []
    for index in range(8):
        pending, visited, needed = list(prelinks[index]['definitions']), set(), set()
        while pending:
            name = pending.pop()
            if name in visited:
                continue
            visited.add(name)
            if name not in all_prelink_defs:
                raise ValueError('required canonical body missing from all actual prelink modules')
            for target in all_prelink_defs[name]['calls']:
                if target in valid_external:
                    continue
                if target not in owners:
                    raise ValueError('unknown prelink closure target')
                if owners[target] != index:
                    needed.add(target)
                pending.append(target)
        required[index] = needed
        if not needed.issubset(allowed[index]):
            raise ValueError(f'prelink Required exceeds original Allowed for unit {index}: {sorted(needed - allowed[index])}')
        receipt = {'unit': index, 'required_prelink': sorted(needed),
                   'allowed_original': sorted(allowed[index]), 'required_count': len(needed)}
        if imported is not None:
            actual = imported[index]
            definitions = set(actual['definitions'])
            ae = {name for name, f in actual['definitions'].items() if f['available_externally']}
            own = set(proof['units'][index]['canonical'])
            if not needed.issubset(definitions):
                raise ValueError(f'actual .3 definitions lack Required for unit {index}: {sorted(needed - definitions)}')
            if not ae.issubset(allowed[index]) or not definitions.issubset(own | allowed[index]):
                raise ValueError('extra/unknown actual imported definition, including non-AE')
            if not actual['declarations'].issubset(set(owners) | valid_external):
                raise ValueError('unknown post-import declaration')
            for f in actual['definitions'].values():
                if not f['calls'].issubset(set(owners) | valid_external):
                    raise ValueError('unknown post-import direct call')
                if any(target in owners and owners[target] != index and target not in allowed[index] for target in f['calls']):
                    raise ValueError('post-import call exceeds original Allowed')
            receipt.update(actual_definitions=sorted(definitions), actual_AE=sorted(ae),
                           required_covered=True, actual_imports_budgeted=True)
        receipts.append(receipt)
    return {'format': 'zil-thinlto-actual-visibility/1', 'units': receipts,
            'required_counts': {str(i): len(required[i]) for i in range(8)},
            'source_cost_recomputed_from_original_IR': source_cost,
            'machine_copy_cost': proof['total_copy_cost'], 'prelink_complete_checked': True,
            'post_import_complete_checked': imported is not None}

def force_diagnostics(stderr, foreign):
    accepted = []
    for line in stderr.splitlines():
        if not line.strip():
            continue
        match = re.fullmatch(r'Error importing module: Failed to import function (' + NAME + r') due to (.+)', line.strip())
        if not match or match.group(1) not in foreign:
            raise ValueError('unknown/non-foreign ThinLTO diagnostic: ' + line)
        # No body and no owner were established in export_proof. Complete
        # Required coverage is still independently mandatory after this message.
        accepted.append({'symbol': match.group(1), 'reason': match.group(2), 'raw': line})
    return accepted
