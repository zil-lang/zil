import runpy
from pathlib import Path
root=Path(__file__).resolve().parents[1] / 'crates/zil-ir/src'
g=runpy.run_path(str(Path(__file__).with_name('generate_dyn_recipes.py')))
items=dict(g['items'])
items.update({
'DynGenericActual':('enum',[('Type','tuple',[('value_0','Type')]),('Const','tuple',[('value_0','Constant')]),('Life','tuple',[('value_0','StorageBinder')]),('QualifiedType','tuple',[('value_0','DynTypeValue')])]),
'DynTraitApplication':('struct',[('declaration','LifeDeclarationId'),('actuals','Vec<DynGenericActual>')]),
'DynMember':('struct',[('application','DynTraitApplication'),('ordinal','u32')]),
'DynCallAccess':('struct',[('reads','Vec<StoragePosition>'),('writes','Vec<StoragePosition>')]),
'DynResultSource':('enum',[('Value','unit',[]),('Create','unit',[]),('Give','unit',[]),('Borrow','struct',[('sources','Vec<StoragePosition>'),('mutable','bool')])]),
'DynThreadRequirements':('struct',[('transfer','bool'),('share','bool')]),
})
header='''//! Shared structural visitor for raw recipe accounting, never semantic proof.
//! Consumers supply their own bounded callbacks before cloning or hashing.
use crate::*;
pub trait DynRecipeVisitor {
    type Error;
    fn node(&mut self, depth: usize) -> Result<(),Self::Error>;
    fn rows(&mut self, length: usize, depth: usize) -> Result<(),Self::Error>;
    fn ty(&mut self, ty: &Type, depth: usize) -> Result<(),Self::Error>;
    fn life(&mut self, life: &StorageLifeShape, depth: usize) -> Result<(),Self::Error>;
    fn identity(&mut self, identity: &LifeDeclarationIdentity, depth: usize) -> Result<(),Self::Error>;
    fn position(&mut self, position: &StoragePosition, depth: usize) -> Result<(),Self::Error>;
    fn constant(&mut self, constant: &Constant, depth: usize) -> Result<(),Self::Error>;
    fn text(&mut self, text: &str, depth: usize) -> Result<(),Self::Error>;
}
pub trait DynRecipeMetadata {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(&self, visitor: &mut V, nesting: usize) -> Result<(),V::Error>;
}
'''
def visit(t,e):
 if t.startswith('Box<'):return visit(t[4:-1],e)
 if t.startswith('Vec<'):
  return 'visitor.rows(('+e+').len(), nesting + 1)?;\nfor item in '+e+' {\n'+visit(t[4:-1],'item')+'\n}'
 if t.startswith('Option<'):
  return 'visitor.node(nesting + 1)?;\nif let Some(item) = '+e+' {\n'+visit(t[7:-1],'item')+'\n}'
 hooks={'Type':'ty','StorageLifeShape':'life','StoragePosition':'position','LifeDeclarationIdentity':'identity','Constant':'constant','String':'text'}
 if t in hooks:return f'visitor.{hooks[t]}({e}, nesting + 1)?;'
 if t in items:return f'({e}).visit_dyn_metadata(visitor, nesting + 1)?;'
 return f'let _ = {e}; visitor.node(nesting + 1)?;'
out=header
for name,(kind,values) in items.items():
 out+=f'impl DynRecipeMetadata for {name} {{\nfn visit_dyn_metadata<V: DynRecipeVisitor>(&self, visitor: &mut V, nesting: usize) -> Result<(),V::Error> {{\nvisitor.node(nesting)?;\n'
 if kind=='struct':out+='\n'.join(visit(t,'&self.'+n) for n,t in values)
 else:
  out+='match self {\n'
  for n,k,fs in values:
   p=name+'::'+n+(' { '+', '.join(n for n,t in fs)+' }' if k=='struct' else '('+', '.join(n for n,t in fs)+')' if k=='tuple' else '')
   out+=p+' => {\n'+'\n'.join(visit(t,n) for n,t in fs)+'\n},\n'
  out+='}\n'
 out+='\nOk(())\n}\n}\n'
(root/'dynamic_recipes_visit.rs').write_text(out)
p=root/'lib.rs';s=p.read_text();s=s if 'mod dynamic_recipes_visit;' in s else s.replace('mod dynamic_recipes_fingerprint;', 'mod dynamic_recipes_fingerprint;\nmod dynamic_recipes_visit;\npub use dynamic_recipes_visit::*;');p.write_text(s)

from generate_dyn_recipe_walk import generate as generate_walk
generate_walk(items,root,header)
