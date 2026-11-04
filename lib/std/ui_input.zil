import std.string
import std.utf8

// Platform adapters convert their indexing units to Unicode scalar offsets.
// The caller increments document revision on every committed document edit.
pub enum Status { Done, Inactive, Stale }
pub package Composition {
    text:string.String;
    active:bool;begin:i64;end:i64;cursor:i64;revision:u64
}
pub fn new()->Composition {return Composition{text:string.empty(),active:false,begin:0,end:0,cursor:0,revision:0u64}}
pub fn active(session:&Composition)->bool {return session.active}
pub fn cursor(session:&Composition)->i64 {return session.cursor}
pub fn preedit(session:&Composition)->&[u8] {return string.as_bytes(&session.text)}
// Starting a new session replaces previous preedit only after range validation.
pub fn begin(session:&mut Composition,document:&string.String,first:i64,last:i64,revision:u64)->string.Error!Status {
    let length=string.scalar_len(document)
    if first<0 || last<first || last>length{return string.Error.Range}
    string.clear(&mut session.text)
    session.active=true;session.begin=first;session.end=last;session.cursor=0;session.revision=revision
    return Status.Done
}
// Update never touches the committed document. Invalid UTF8/cursor, limits and
// allocation failure leave the previous preedit and cursor unchanged.
pub fn update(session:&mut Composition,text:&[u8],cursor:i64,byte_limit:i64)->string.Error!Status {
    if !session.active{return Status.Inactive}
    let validated=utf8.locate(text,0,cursor)
    if validated.error>=0{return string.Error.InvalidUtf8}
    if cursor<0 || validated.last<0{return string.Error.Range}
    let old=string.scalar_len(&session.text)
    string.replace(&mut session.text,0,old,text,byte_limit)?
    session.cursor=cursor
    return Status.Done
}
pub fn cancel(session:&mut Composition)->Status {
    if !session.active{return Status.Inactive}
    string.clear(&mut session.text);session.active=false;session.cursor=0
    return Status.Done
}
// A stale revision leaves both values unchanged for the caller to cancel or
// restart. Commit may differ from preedit, as actual platform IMEs permit.
pub fn commit(session:&mut Composition,document:&mut string.String,revision:u64,text:&[u8],byte_limit:i64)->string.Error!Status {
    if !session.active{return Status.Inactive}
    if revision!=session.revision{return Status.Stale}
    string.replace(document,session.begin,session.end,text,byte_limit)?
    string.clear(&mut session.text);session.active=false;session.cursor=0
    return Status.Done
}
