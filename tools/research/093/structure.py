#!/usr/bin/env python3
"""Finite model check of an explicitly restricted asynchronous-product theorem.

This does not replace or reimplement Boundary Lab U/K/D. One algorithm explores
the global 2-bit graph; another closes each local relation by Boolean matrices.
"""
import argparse, itertools, json
from pathlib import Path

def global_reach(initial, edges):
    todo=list(initial);found=set(todo)
    while todo:
        a,b=todo.pop()
        for coordinate, relation in enumerate(edges):
            for x,y in relation:
                if (a,b)[coordinate]!=x:continue
                next_state=(y,b) if coordinate==0 else (a,y)
                if next_state not in found:found.add(next_state);todo.append(next_state)
    return found

def local_close(initial, relation):
    # Warshall closure differs from the global queue traversal.
    matrix=[[i==j or (i,j) in relation for j in range(2)] for i in range(2)]
    for k in range(2):
        for i in range(2):
            for j in range(2):matrix[i][j]|=matrix[i][k] and matrix[k][j]
    return {j for j in range(2) if any(matrix[i][j] for i in initial)}

def rooted_reach(roots, edges):
    found=set(roots); todo=list(roots)
    while todo:
        source=todo.pop()
        for left,right in edges:
            if left==source and right not in found:found.add(right);todo.append(right)
    return found

def remove_unreferenced(initial, roots, edges):
    # Deliberately incorrect incremental candidate: peel unsupported vertices,
    # treating any remaining incoming edge as support even inside a rootless SCC.
    retained=set(initial)
    while True:
        gone={v for v in retained if v not in roots and
              not any(a in retained and b==v for a,b in edges)}
        if not gone:return retained
        retained-=gone

def check():
    pairs=list(itertools.product(range(2),repeat=2))
    relations=[{p for i,p in enumerate(pairs) if bits&(1<<i)} for bits in range(16)]
    initials=[{i for i in range(2) if bits&(1<<i)} for bits in range(1,4)]
    cases=queries=0
    for ra,rb,ia,ib in itertools.product(relations,relations,initials,initials):
        global_states=global_reach(itertools.product(ia,ib),(ra,rb))
        factors=[local_close(ia,ra),local_close(ib,rb)]
        assert global_states==set(itertools.product(*factors))
        cases+=1
        for ca,cb in itertools.product(range(4),repeat=2):
            bad=[{i for i in range(2) if c&(1<<i)} for c in [ca,cb]]
            actual=any(a in bad[0] or b in bad[1] for a,b in global_states)
            factored=any(factors[i]&bad[i] for i in range(2))
            assert actual==factored;queries+=1
    correlated={(0,0),(1,1)}
    marginals=set(itertools.product({x[0] for x in correlated},{x[1] for x in correlated}))
    assert (0,1) in marginals-correlated and (1,0) in marginals-correlated
    # Fixed control: set x; abort; set y. Ignoring the abort/control permits y.
    actual_prefix={(0,0),(1,0)}
    wrongly_unordered=global_reach([(0,0)],([{(0,1)}][0],{(0,1)}))
    assert (1,1) in wrongly_unordered-actual_prefix
    # Deletion of the only external seed cannot leave an SCC self-supported.
    roots={"seed"}; edges={("seed","a"),("a","b"),("b","a")}
    supported_before=rooted_reach(roots,edges)-roots
    edges.remove(("seed","a"))
    naive_after=remove_unreferenced(supported_before|roots,roots,edges)-roots
    actual_after=rooted_reach(roots,edges)-roots
    assert naive_after!=actual_after
    explicit=[]
    for k in range(13):
        rows=list(itertools.product(range(2),repeat=k))
        assert len(rows)==2**k
        explicit.append(dict(dimensions=k,explicit_rows=len(rows),factor_members=2*k))
    return dict(status="Pass",level="finite independent model check, not machine-checked universal proof",
        asynchronous_models=cases,local_safety_queries=queries,explicit_output_counts=explicit,
        counterexamples={
            "shared_guard_or_correlated_initial":{"actual":sorted(correlated),"wrong_product":sorted(marginals)},
            "control_and_early_abort":{"actual_prefix":sorted(actual_prefix),"wrong_async":sorted(wrongly_unordered)},
            "scc_delete_seed":{"before":sorted(supported_before),"wrong_reference_count":sorted(naive_after),"actual":sorted(actual_after)}},
        limitations=["two two-state components exhaustively checked", "not arbitrary CFGs, heaps, aliases or production safety",
                     "factor count does not include full explicit output; contracts are different"])

if __name__=="__main__":
    p=argparse.ArgumentParser();p.add_argument("--out",type=Path,required=True);a=p.parse_args()
    a.out.parent.mkdir(parents=True,exist_ok=True)
    with a.out.open("x") as f:json.dump(check(),f,ensure_ascii=False,indent=2);f.write("\n")
