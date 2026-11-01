from pathlib import Path
import re,hashlib,json,collections
root=Path('/workspace/zil-243');e=root/'evidence/integration/owned-sum-20261102/legacy-migration'
def literals(s):
 out=[];i=0
 while i<len(s):
  m=re.match(r'(?:b?r)(#*)"',s[i:])
  if m:
   end='"'+m[1];start=i+len(m[0]);j=s.index(end,start);out.append(s[start:j]);i=j+len(end);continue
  if s[i]=='"':
   j=i+1
   while j<len(s):
    if s[j]=='\\':j+=2;continue
    if s[j]=='"':break
    j+=1
   out.append(s[i+1:j]);i=j+1;continue
  i+=1
 return collections.Counter(t for t in out if any(x in t for x in ['fn ','package ','resource ']))
result={}
for old in sorted((e/'originals').glob('*.rs.original')):
 p=root/'crates/zil-driver/tests'/old.name.removesuffix('.original');a=literals(old.read_text());b=literals(p.read_text());assert a==b,(p,list((a-b).elements()),list((b-a).elements()))
 result[str(p.relative_to(root))]={'program_literal_multiset_unchanged':True,'program_literal_occurrences':sum(a.values()),'literal_sha256_multiset':sorted([{'sha256':hashlib.sha256(t.encode()).hexdigest(),'occurrences':n} for t,n in a.items()],key=lambda x:x['sha256'])}
# Exact unchanged raw mutation construction prefixes, not just a type label.
for name,start,oldend,newend in [('impl206_independent','    for variant in 0..8 {','        attack(','        if variant == 5 {'),('review223_boundaries','    for variant in 0..4 {','        rejected(','        // §8.1.5 permits Product alternatives')]:
 a=(e/'originals'/(name+'.rs.original')).read_text();b=(root/'crates/zil-driver/tests'/(name+'.rs')).read_text();x=a[a.index(start):a.index(oldend,a.index(start))];y=b[b.index(start):b.index(newend,b.index(start))];assert x==y,name
 result[name+'-raw-construction']={'unchanged':True,'sha256':hashlib.sha256(x.encode()).hexdigest()}
(e/'INPUT-PRESERVATION.json').write_text(json.dumps(result,indent=2)+'\n')
print('program literal multisets and original raw construction blocks unchanged')
