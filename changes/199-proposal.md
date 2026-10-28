# 199 预检提案：直接 owning State callable 字段调用应保持原位置

状态：只有实际输入、现行拒绝与只读根因定位；未改199生产、未登记已完成能力。由197有限组合测试发现，root要求留作后续语言实施项，不从完整1.4终态移除。

## 可复现输入与实际阶段

197源码 `cb1321846a727647086b79bad55de4ed12e71c33` 的限定组合首次执行该原始输入，完整原字节保留在197作者证据压缩包 `combined01/cases/state197-state-loop-exits-413579-4/main.zil`。CLI生产与197候选da3a663a一致。实际结果 **SafeReject / Analyze / E-ANALYZE-IR**，`resource closure: package field take requires that field live on every path`，verification_runs=0，未产生成功原生执行。

```zil
package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn pack<T:Copy,F:FnMut(&mut T)->T>(callback:F,value:T)->State<T,F>{
    return State::<T,F>{bytes:bytes_new(),callback:callback,value:value}
}
fn spin<T:Copy,G:FnMut(&mut T)->T>(state:State<T,G>,count:i32)->T{
    var current=state
    var i=0
    while i<count{
        i+=1
        if i==2{continue}
        current.callback(&mut current.value)
        if i==4{break}
    }
    return current.value
}
fn factory()->impl FnMut(&mut i32)->i32{
    var hits=0
    return move(value:&mut i32)=>{hits+=1;*value+=1;return *value}
}
fn main()->i32{return spin(pack(factory(),10),7)-13}
```

`impl197_mixed_state::mixed_state_explicit_fnmut_field_borrows_compose_with_continue_and_break`保留真实直接调用的拒绝断言；只将调用点改为下列既有192显式字段借址后，`27d8e92269b1618006503cc6ae7139a6c9c50aa2`的实际O0/O2执行均返回0：

```zil
let callback=&mut current.callback
let value=&mut current.value
callback(value)
```

故197的普通T身份、两字段真实分离和195循环出口可以组合；未开放的部分是直接调用位置的保留，不能把前门拒绝或字段移动误报成别名反例。这个提案不声称完整回调字段语法已经完成。

## 只读根因与最小建议写点

`zil-types/src/lib.rs::call_closure`只有`CoreExprKind::Name`得到`existing_place`。局部环境名按Fn/FnMut借入真实local，FnOnce转移责任；直接`CoreExprKind::Member`则先走`expr(callee)`。

`zil-types/src/aggregates.rs::aggregate`的Package成员进入`zil-types/src/packages.rs::package_field`。后者对`Type::Closure`发`TakePackageField`，因此回调环境被取到临时位置；后面的FnMut调用借的是那个临时位置，原State字段不再存活。循环回边上的第二次take被资源分析正确拒绝。简单放松Analyze的live检查会破坏责任守恒，不能作为修复。

已有`zil-types/src/addresses.rs::address`可根据实际`package_place`生成`BorrowPackageField{local,index,mutable}`，`call_closure`已有`borrowed_callee`分支发`CallClosureBorrowed`。建议首先只对**真实本地完整owning State的直接callable字段**：Fn建立共享字段调用地址、FnMut建立独占字段调用地址，FnOnce保留take；一般字段读、显式取出/替换、临时返回State和无法固定位置的表达式不应被批量改写。Group/Sequence等形状如需开放须明确一次求值、临时生命周期与来源，不靠复制callee或无条件移入临时规避冲突。

最小行为写集预计以Types `lib.rs::call_closure`为入口，复用`addresses.rs`的真实字段地址能力；仅当实际编译证明必要才调整`packages.rs`、闭包推导或Expand符号检查对应块。已存在的公开BorrowPackageField/CallClosureBorrowed似乎足以表示首片，**尚未实证，不能预先宣布无需schema变更**。不得直接修改Analyze/Verify检查以制造接受；两阶段应独立从实际IR重建来源和清理。

## 必须完成的相邻核验

- 原直接调用输入逐字成功；普通`State<F>`与混合`State<T,F>`各有Fn/FnMut、0/1/多次、break/continue与错误返回的源码执行。
- FnOnce真实消费仍只能一次；取出callback之后未补回的State仍不能完整转交，清理不重复释放环境或兄弟资源。
- 真正不同字段可同时借入；同一字段窗口、捕获资源的实际当前调用冲突、父State移动/销毁/替换和活跃lending结果继续拒绝。callee建立之后参数中的失效/重入按真实顺序处理，不先算参数来躲过冲突。
- callee位置和有副作用参数只求值一次；错误准备出口与分配故障释放实际已准备责任；O0/O2与零调用清理按原有平台记录。
- raw/Wire伪造字段来源、mutable模式、FnOnce改借入、删take/cleanup及篡改实际父local须被独立Verify拒绝；每个攻击保留相邻源码正控制。
- 新增位置、类型遍历和临时图工作在原额度内预先收费，N/N−1与fresh retry实测，默认额度不增加。

当前只证明一个具体缺口和一个已存在的显式写法正控制。没有199实现、没有199原生或raw/Wire新语义验收，完整语言NotSatisfied、产品NotRun保持。
