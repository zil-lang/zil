// Limited prefix/attribute syntax reference, NOT a ZIL parser, evaluator, or safety checker.
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
const catalogue = JSON.parse(fs.readFileSync(new URL('../spec/attributes.json', import.meta.url), 'utf8'));
export class SyntaxErrorCode extends Error { constructor(code, message, offset=0) { super(message); this.code=code; this.offset=offset; } }
const fail=(c,m,p)=>{throw new SyntaxErrorCode(c,m,p)};
function skip(s,i, allowNewline=true){
  for(;;){
    const start=i;
    while(i<s.length && /\s/.test(s[i]) && (allowNewline || !/[\r\n]/.test(s[i])))i++;
    if(s.startsWith('//',i)){const e=s.indexOf('\n',i);if(e<0)return s.length;if(!allowNewline)return i;i=e+1;}
    else if(s.startsWith('/*',i)){
      let depth=1;i+=2;
      while(i<s.length&&depth){if(s.startsWith('/*',i)){depth++;i+=2;}else if(s.startsWith('*/',i)){depth--;i+=2;}else{if(!allowNewline&&/[\r\n]/.test(s[i]))fail('E_HASH_NEWLINE','# 分发前不能换逻辑行',i);i++;}}
      if(depth)fail('E_COMMENT','块注释未闭合',i);
    }
    if(i===start)return i;
  }
}
export function balanced(s,start){
  const pairs={'(':')','[':']','{':'}'};if(!pairs[s[start]])fail('E_DELIMITER','需要定界符',start);
  const stack=[pairs[s[start]]];let i=start+1;
  while(i<s.length&&stack.length){
    if(s.startsWith('//',i)){let e=s.indexOf('\n',i);i=e<0?s.length:e+1;continue;}
    if(s.startsWith('/*',i)){i=skip(s,i);continue;}
    const c=s[i];
    if(c==="'"){const ch=/^'(?:\\u\{[0-9a-fA-F]+\}|\\.|[^'\\\r\n])'/u.exec(s.slice(i));if(ch){i+=ch[0].length;continue;}}
    if(c==='"'){i++;let closed=false;while(i<s.length){if(s[i]==='\\'){i+=2;continue;}if(s[i++]==='"'){closed=true;break;}}if(!closed)fail('E_STRING','字符串未闭合',i);continue;}
    if(pairs[c])stack.push(pairs[c]);else if(')]}'.includes(c)){if(stack.pop()!==c)fail('E_DELIMITER','定界符不匹配',i);}
    i++;
  }
  if(stack.length)fail('E_DELIMITER','定界符未闭合',start);
  return {body:s.slice(start+1,i-1),end:i};
}
export function dispatch(source,context='expression'){
 let i=skip(source,0);if(source[i]!=='#')fail('E_HASH_REQUIRED','需要#编译入口',i);i=skip(source,i+1,false);
 if(/[\r\n]/.test(source[i]??''))fail('E_HASH_NEWLINE','#后不能换行',i);
 const c=source[i];
 if(c==='['){if(!['item','field','variant','parameter'].includes(context))fail('E_ATTR_POSITION','属性不能作值表达式',i);const b=balanced(source,i);if(!b.body.trim())fail('E_ATTR_EMPTY','空属性',i);return {kind:'attribute',...b};}
 if(c==='('){const b=balanced(source,i);if(!b.body.trim())fail('E_CT_EMPTY','空求值表达式',i);return {kind:'evaluate',...b};}
 if(c==='{')return {kind:'evaluate-block',...balanced(source,i)};
 if(c==='!')fail('E_INNER_ATTRIBUTE','不提供#![]',i);
 const word=/^[A-Za-z_][A-Za-z0-9_]*/.exec(source.slice(i))?.[0];
 if(context==='generic-parameter'&&word)return {kind:'comptime-parameter',name:word,end:i+word.length};
 if(['if','for','emit'].includes(word)){
  if(!['item','statement'].includes(context))fail('E_CT_POSITION','生成指令需要声明或语句位置',i);
  return {kind:word==='emit'?'emit':word==='if'?'select':'repeat',end:i+word.length};
 }
 fail('E_HASH_FORM','未知#形式；普通求值使用#(...)',i);
}
export function parseAttribute(text,target='fn',custom=[]){
 const p=dispatch(text,['field','variant','parameter'].includes(target)?target:'item');
 if(p.kind!=='attribute')fail('E_ATTR_REQUIRED','需要属性',0);
 const m=/^\s*([A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*)/.exec(p.body);if(!m)fail('E_ATTR_NAME','属性必须有名称',0);
 const name=m[1],def=catalogue.attributes.find(a=>a.name===name)??custom.find(a=>a.name===name);
 if(!def)fail('E_ATTR_UNKNOWN','未注册属性 '+name,0);
 if(!def.targets.includes(target)&&!(def.targets.includes('item')&&['fn','struct','enum','package','resource','trait','impl','module','const','type'].includes(target)))fail('E_ATTR_TARGET','属性不允许用于 '+target,0);
 const rest=p.body.slice(m[0].length).trim();let args='';
 if(rest.startsWith('(')){const b=balanced(rest,0);if(rest.slice(b.end).trim())fail('E_ATTR_FORMAT','多余属性参数',b.end);args=b.body.trim();}
 else if(rest.startsWith('=')){if(name!=='doc')fail('E_ATTR_FORMAT','只有doc使用=字符串',0);args=rest.slice(1).trim();}
 else if(rest)fail('E_ATTR_FORMAT','属性参数结构错误',0);
 if(def.args==='none'&&args)fail('E_ATTR_ARGS','属性不接受参数',0);
 if(name==='repr'&&!/^(C|transparent)$/.test(args))fail('E_ATTR_ARGS','未知repr',0);
 if(name==='inline'&&args&&!/^(always|never)$/.test(args))fail('E_ATTR_ARGS','未知inline策略',0);
 if(name==='derive'&&(!/^[A-Za-z_]\w*(?:\.\w+)*(?:\s*,\s*[A-Za-z_]\w*(?:\.\w+)*)*,?$/.test(args)))fail('E_ATTR_ARGS','derive需要trait路径列表',0);
 if(name==='derive'){const a=args.split(',').map(x=>x.trim()).filter(Boolean);if(new Set(a).size!==a.length)fail('E_ATTR_DUPLICATE','重复derive',0);}
 if(name==='align'&&!args)fail('E_ATTR_ARGS','align需要常量表达式',0);
 if(name==='align'&&/^\d+$/.test(args)){const n=BigInt(args);if(n<=0n||(n&(n-1n))!==0n)fail('E_ATTR_ARGS','align需要正二次幂',0);}
 if(name==='doc'&&!/^"(?:\\.|[^"\\])*"$/.test(args))fail('E_ATTR_ARGS','doc需要字符串',0);
 if(name==='cfg'&&!args)fail('E_ATTR_ARGS','cfg需要配置条件',0);
 return {name,args,kind:def.kind,phase:def.phase,repeat:def.repeat,end:p.end};
}
export function attributePlan(texts,target='fn',custom=[]){
 const attrs=texts.map(x=>parseAttribute(x,target,custom));const counts=new Map();const derived=new Set();
 for(const a of attrs){if(a.name==='derive'){for(const path of a.args.split(',').map(x=>x.trim()).filter(Boolean)){if(derived.has(path))fail('E_ATTR_DUPLICATE','重复derive '+path,0);derived.add(path);}}counts.set(a.name,(counts.get(a.name)||0)+1);if(a.repeat==='error'&&counts.get(a.name)>1)fail('E_ATTR_DUPLICATE','单次属性重复 '+a.name,0);}
 // Real compiler resolves declared-phase dependencies, not just this stable sort.
 return attrs.map((a,i)=>({...a,index:i})).sort((a,b)=>a.phase-b.phase||a.index-b.index);
}
