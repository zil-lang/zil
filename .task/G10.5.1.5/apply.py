from pathlib import Path
import hashlib
p=Path('crates/zil-analyze/src/resources.rs');s=p.read_text()
assert hashlib.sha1(b'blob '+str(len(s.encode())).encode()+b'\0'+s.encode()).hexdigest()=='19a9b0437b669cc720106032284cbff1f11c71bd'
def replace(a,b,n=1):
 global s
 assert s.count(a)==n,(a,s.count(a),n)
 s=s.replace(a,b)
replace('mod summary_graph;','mod summary_graph;\nmod order;')
replace('later: BTreeSet<(LocalId, LocalId)>,','later: order::Order,')
replace('.saturating_add(state.later.len())','.saturating_add(state.later.storage_cost())')
a='''    budget.spend(
        to.later.len().saturating_add(from.later.len()),
        "resource initialization order join",
    )?;
    let before = to.later.len();
    to.later.extend(&from.later);
    changed |= before != to.later.len();'''
replace(a,'''    changed |= to.later.join(&from.later, budget)?;''')
replace('''    budget.spend(state.later.len(), "resource order removal")?;
    state.later.retain(|(a, b)| *a != local && *b != local);
    Ok(())''','''    state.later.forget(local, budget)''')
replace('''budget.lookup(state.later.len(), "Unpack order insertion")?;
                                    state.later.insert((*older, *target));''','''state.later.insert((*older, *target), budget, true, "Unpack order insertion")?;''')
replace('''budget.lookup(state.later.len(), "resource order insertion")?;
                                    state.later.insert((*other, *local));''','''state.later.insert((*other, *local), budget, true, "resource order insertion")?;''')
replace('initial.later.insert((*older, *local));','initial.later.insert((*older, *local), budget, false, "incoming resource order insertion")?;',2)
replace('for (before, later) in &state.later {','for (before, later) in state.later.cleanup_edges(*local, budget)? {')
replace('''if *before == *local
                                && copied(state, Place::Local(*later), budget)?''','''if before == *local
                                && copied(state, Place::Local(later), budget)?''')
p.write_text(s)
print('Applied exact resource relation packing; no Verify/backend/default-limit changes')
