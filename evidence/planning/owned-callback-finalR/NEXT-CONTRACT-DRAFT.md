# 下一主线草案：同步拥有 Event/Reply 的具体闭包与公共 dispatch

这是派工准备稿，尚未修改或冻结规范、工单、代码。依赖239–244最终共同候选及其独审通过；不把本批普通helper数据参数已贯通，误记为callback签名已拥有化。

## 1. 最小可交付合同

必达接口使用现有拼写：

```zil
fn dispatch<F:FnOnce(Event)->Reply>(handler:F,event:Event)->Reply {
    return handler(event)
}
```

Event/Reply是已具体化的普通有限Value类型，复用§8.1.5的完整资格：Copy伴随值、已核Managed叶、Product和真实活动分支Sum；Store叶仍核实际canonical定义和原元素资格。新的拥有接口通过完整Product/Enum/ErrorUnion传值，不新增裸`R`/`ByteBuffer`的拥有含义：裸资源仍是Desc，`manages`、create/give继续沿既有路线。保留错误联合的精确槽规则：旧CopyError!ManagedSuccess例外不扩大；其它非Copy槽只能Product/Enum，不能直接嵌拥有ErrorUnion槽。

第一派工必须闭合：具体闭包拥有Value入参、拥有Value结果，以及唯一F、按值FnOnce公共普通free-function helper的同签名入口/返回。支持既有有限参数数量，普通Copy参数可与拥有Value参数同列；不能仅做一个匿名闭包直调或仅做入参。具体环境仍按捕获效果推导Fn/FnMut/FnOnce；每次消费Event不等于消费环境。无消费捕获的Fn可连续处理不同Event，正确回写的FnMut保持可重复；这些具体调用也须贯通。实际Fn/FnMut向FnOnce请求的已有适配保持，不能把它们强行推为FnOnce。

公共泛型首要边界固定为上面的单F、按值FnOnce。新增拥有签名不顺带给公开借入F/FnMut dispatch、多F/F-G、State构造/返回、callback Store/数组、opaque callable factory和shared/lending profile授资格；那些入口沿各自现有合同，只有明确纳入后续切片并核验才能晋升。现有Copy/同步Copy-address签名保持原能力；本片的新拥有签名不混入地址/Desc显式参数或借入结果。这个数据签名准入不替代capture准入：直接拥有Enum、Package<Sum>捕获仍按现边界拒；204/225 Store拥有环境及父环境的Copy/Unit结果上限、226共享Product捕获结果上限均保留，不能借新通用predicate绕过。

Owned输入在调用成功进入时各转交一次，结果在每条正常返回中完整转交一次。callee-before-arguments、从左到右实际准备、prepare成功临时的逆序失败清理、return结果先准备再退出清理、FnOnce未消费capture的清理均与普通调用一致。FnOnce本身是至多一次调用资格：未调用而合法销毁仍可；`dispatch`源码的恰好一次调用由真实CFG和公共call trace证明。它不等于defer注册后的必须履行债。

普通Value跨closure call和dispatch边界的Managed叶按Incoming/Unknown处理，不导出callee私有Created；只有已核既有直接Managed create合同仍有原新鲜性权限。实际carrier、活动分支、字段位置、Store根和活窗口不能因同型合并；来源保真不等于承诺返回原物理输入。返回Reply禁止含新的借入依赖或偷偷携带F；普通结果并非`keep<F>(f:F)->F`。后者需要独立公开返回来源/环境身份关系，不在本片。

## 2. 已有规范依据与需要增列的位置

| 条目 | 直接复用的义务/仍需新登记的边界 |
|---|---|
| §8.1、§8.1.1 | 普通责任形状、完整/部分Product、实际责任/深层叶来源；普通函数返回无逐叶fresh合同不能洗成Created。 |
| §8.1.3、§8.1.3.1、§8.1.4 | Store实际资源/名义身份；完整准备后提交替换；不继承父清理年龄。作为数据经过callback，不取得额外容器资格。 |
| §8.1.5 | 本批普通有限Sum、只读tag与原子拆解、完整边界、ErrorUnion槽、真实新初始化与清理计划。末段明确未开放callback拥有签名，所以本片是新具名接受性，不能只称旧bug修复。 |
| §7.1 | return/?/break/continue/defer退出必须清当前真实债；trap无unwind承诺。 |
| §9.1、§9.2 | 声明点捕获、完整自由路径、三执行时点、实际字段/实例，不由参数消费改变capture行为。 |
| §9.1.1–§9.1.4 | 各capture独立有限资格和结果上限；不能递归调用ordinary准入替换这些门。 |
| §9.3、§9.7 | 模式只描述环境访问；调用联合实例化capture/显式实参的来源、写效果和窗口；最后一次调用不免除冲突。 |
| §9.4、§9.4.1 | 具体环境身份与真实ABI；F返回、opaque工厂、擦除另有合同，不能从Event/Reply返回类推。 |
| 有限helper既有公开roles/traces/Cleanup合同 | 单F的实际调用次数/顺序/cleanup仍由CFG核；允许owned数据结果时不能依靠callee私有效果向caller授新权限。 |

可在§9.3附近增加一段上述“同步拥有Value调用签名”，并在有限helper入口明确单F FnOnce dispatch；无需另设宏大批准或产品门。最终工单沿既有root冻结/区域实施/非作者核验流程即可。

## 3. 当前真实阻断与表示缺额

2026-11本次只读复核使用现有CLI `/workspace/.zil-env/target-241/debug/zil`，SHA256=`4d0f5bae7012050c22aa2c387f4599fe2a75c458eb69e058e666f45f9e56b9ba`（239–244施工中的5be窗口，不冒充最终freeze）。原文件未改，三次`zil check … --json`均在Types Unsupported、无Verify/native：

| 原始探针 | 实际首阻断 |
|---|---|
| `/tmp/q-owned-callback-precheck/02_owned_event_reply_dispatch/source.zil`：FnOnce(Event)->Reply，二者含ByteBuffer | Types `E-UNSUPPORTED`，closure参数有限Copy/一层地址，地址参数需Copy结果。 |
| `…/03_owned_event_copy_reply_direct/source.zil`：具体closure拥有Event入参、i32结果 | 同一Types签名门；证明不只是公共generic helper问题。 |
| `…/04_owned_reply_from_once_capture/source.zil`：零参、实际消费ByteBuffer capture、返回拥有Reply | 同一Types签名门；证明结果方向也独立缺失。 |

这些原Q探针的源/hash/旧失败在`/tmp/q-owned-callback-precheck`，本片必须保留原字节晋升。当前本批ordinary helper extra-data Enum/Package/owned ErrorUnion正例通过，仅表示`F:Fn()->i32`与另一个拥有参数可以同在，不能替代这些红测。

后继阻断来自代码，而非猜测：

- Types `closures.rs::signature_parameter/supported_signature`和`inference.rs`lambda头首先只准Copy/address；闭包参数模式、参数局部与字段/match用途推导需要拥有输入，必须区分input资源操作与environment effects。
- Types普通helper template/header/实例化仍核callback签名，并把helper结果限制为Copy/lending/既有State；新的Reply结果需独立ordinary数据资格，不得走State或opaque factory近路。
- Analyze/Verify各自closure-definition签名准入仍有限Copy；CallClosure/Shared/Mut/Borrowed/Once资源分支主要处理callee环境与效果，缺拥有arguments的消费及拥有result的建立。只去掉Types保护会留下漏债/无合法结果，而不是可运行切片。
- 两核helper结果边界还需支持封闭拥有Value数据；公共roles/traces仍只描述实际callback角色，不能把data参数或Reply当新环境角色。普通`Call`已有完整数据责任路径可复用，但不可私看callee返回树来授Created。
- `ClosureDefinition.parameters:Vec<Type>`、`result:Type`和现有CallClosure* arguments/result SSA已经能表示完整具体Product/Sum值；环境入口TypedFunction也有普通参数/结果类型。最小片预计不需新Invoke或新权限字段。若实施发现必须新增公开信息，报告那一条不可表达的关系，而不是先造新schema。
- LLVM common resource closure emitter现按`llvm_type`传实参/结果、FnOnce调用后清剩余环境，普通函数已有Product/Sum ABI。必须实测返回owned header/inline payload在环境清理前正确保留及失效flags；不能从“能emit struct”推定安全或O0/O2布局已经完成。

## 4. 一个完整的ZUI同步提交用例（拟验收原源）

一次提交事件拥有payload字节，handler消费一份捕获并留下另一份供环境退出清理，Reply直接交渲染器。没有保存callback进新State/Store，也不请求新capture权限。用例沿现有语法编写，拟晋升拥有callback签名与dispatch数据结果；它尚未在放开后的全链路运行，下列输出是拟验收义务，具体ABI/flag遗漏仍属未知。

```zil
extern "C" fn getchar()->i32;
extern "C" fn putchar(n:i32)->i32;
fn read()->i32{unsafe{return getchar()}}
fn emit(n:i32){unsafe{putchar(n)}}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
enum Fault{NoMemory,Cancelled}
package Event{manages mark:Mark;manages payload:ByteBuffer;code:i32}
package Reply{manages mark:Mark;manages payload:ByteBuffer;code:i32}
fn prepare(cancel:bool)->Fault!Event{
    let mark=Mark{code:65}          // A
    let payload=bytes_new()
    if !bytes_reserve(payload,1){return Fault.NoMemory}
    bytes_push(payload,85u8)
    if cancel{return Fault.Cancelled}
    return Event{mark:give mark,payload:give payload,code:41}
}
fn dispatch<F:FnOnce(Event)->Reply>(handler:F,event:Event)->Reply{
    return handler(event)
}
fn render(reply:Reply)->i32{
    emit(82)                      // R
    if bytes_len(reply.payload)!=1{return 2}
    return reply.code-42
}
fn submit(cancel:bool)->Fault!i32{
    emit(83)                      // S
    defer{emit(81)}               // Q
    let active=Mark{code:72}      // H
    let spare=Mark{code:104}      // h
    let handler=move(event:Event)->Reply=>{
        emit(67)                  // C: proves the body actually entered
        let held=give active     // forces actual consuming environment entry
        let seen=spare.code      // leaves this actual capture for env cleanup
        return Reply{mark:give event.mark,
                     payload:give event.payload,code:event.code+1}
    }
    let reply=dispatch(handler,prepare(cancel)?)
    return render(reply)
}
fn main()->Fault!i32{return submit(read()==49)}
```

确定性义务：输入`0`且分配成功：stdout `SCHhRAQ`、exit0；输入`1`：`SAHhQ`、recoverable error exit，C/R绝不出现。ByteBuffer reserve真实失败：同`SAHhQ`，实际allocator live=0。H来自body局部清理，h来自FnOnce剩余capture，A随Reply到renderer后清理；不能用名字/类型去重、倒换或提早毁掉Reply。原ByteBuffer实际指针/长度/内容应另有机器或allocator收据，而不是只看标记字符。

这个用例闭合事件准备→真实高阶调用→拥有响应→渲染→退出；它不是完整ZUI框架、长期handler存储或性能胜出声明。另需一个Event/Reply含本批普通Enum/ErrorUnion的自然变体，以及普通Store Product作为数据穿越的变体；容器/capture资格保持独立。

## 5. 跨阶段必须同时交付的义务

| 区域 | 最小工作与证据 |
|---|---|
| Syntax/Core/Expand | 拼写与F bound沿现有表，不新加give形参语法。定义期和未调用模板核实际非Copy Event/Reply，含Enum完整字段与资源同名F的词法身份；模式/参数/result逐项保真，禁止通过未实例化T或宏生成丢`manages`。 |
| Types | 独立有限signature admission；拥有输入按Value Move、仅一次stage/commit，真正ENV用途决定kind。结果先完整准备后清局部/剩余ENV；参数/返回/match/?/defer/循环所有出口，同普通函数资源路径。直接与泛型两入口同时完成。 |
| Analyze | 独立核全部unused closure定义；所有CallClosure*逐个结实参债并建立结果债/来源，sum只持活动分支；保留外部Desc效果/窗口。F调用语言仍真实求出；普通Value边界不导出Created。 |
| Verify | 从raw定义、指令、CFG独立重建同一事实；不调用Types/Analyze求证，不能只接受新signature bit。raw/Wire错类型、错mode、漏/重复consume、制造result、坏slot/capture/loan都须拒。 |
| Public helper/接口 | 允许单F FnOnce封闭数据签名与普通Reply返回，保持既有roles/traces/Hold/Call/Cleanup关系；不把Reply推成State/F、不私看callee证明新鲜性。跨模块调用与unused声明都核。 |
| Lower/LLVM | 现实体布局/实际flags/剩余ENV清理；owned返回不能因Once环境teardown被释放或覆盖。O0/O2、直接/间接环境入口、helper与跨模块ABI一致，无Box/RC/新全局表。 |
| Wire/缓存/预算 | 若公开表示完全复用，保所有类型/实参/result/环境身份指纹，缓存命中仍重核。每阶段共用实际请求remaining，资格/复制/来源/布局/结果临时在增长前预付；新N/N−1/fresh/暖调用，旧无新profile费用保持，不按每次callback重置额度。 |

## 6. 最少近邻集（非法例不跑native）

1. **输入消费/结果责任**：同一个Event交两次、call后再读已移出字段、丢掉Reply不归还/不清理、把一份Reply同时当返回与局部存活；source与raw/Wire都拒。
2. **环境模式分离**：无消费capture的Fn连续处理两个不同Event；FnMut计数持续为1/2并各归还Reply；FnOnce captured active第二次调用拒。不能因消费参数就把前两者降成Once。
3. **失败准备**：callee实际先取走后`prepare()?`失败，无C、只清已成功stage的env；`handler(event,fail()?)`覆盖拥有实参已stage后失败，反序只清当前责任。另测callback返回Fault!Reply及?退出。
4. **返回先准备**：Reply拥有Event叶或ENV叶时，return后的局部defer/Drop和FnOnce剩余ENV只清剩余部分，不能毁掉结果；实际每个分配失败点均live0。
5. **来源**：直接构造与跨Call清楚分开；callback/dispatch返回的Incoming叶不能满足caller result(create)，包括private factory内部新造及identity relay两类。不能按同型Store/资源合并根或复活旧描述。
6. **活窗口**：callback capture保有Event字段的Desc/视图、随后又把该Event整体交入；最后一次调用也须冲突拒。已结束窗口/安全Copy快照保正例。
7. **闭profile与ABI**：Event/Reply数据资格不得开放Enum capture、Store capture owned-result上限、State/two-F、StoreElement/数组、FFI/thin pointer、lending、async/CT、裸F返回。raw未用定义仍核；整个含拥有variant的Enum在Empty活动分支仍非Copy。
8. **原始调用伪造**：closure definition与entry/实参/result type或kind不一致；重复参数SSA、凭空owned result、错tag/路径、错误ErrorUnion槽、错误实际Store定义；逐项raw与Wire复核，借口trap不能替代证明。

最少正控保原Q三红源原字节晋升、上述ZUI用例三模式、具体Fn/FnMut重复新Event、跨模块FnOnce dispatch、Event/Reply的Product/Sum/ErrorUnion与Store数据变体。实际运行按各最终窗口记数，不累计重放；完整产品、Win64及性能比较沿既有产品门另记。

## 7. 建议派工最短写集

前端：Expand generic callable/check与必要walker；Types closures/inference/普通helper及参数/结果物化接点。两核分别：closure signature、helper结果、resources调用transfer、必要Hold/来源预付接点。Lower/backend只补实际ABI/flag/计划遗漏，不在后端扩准入。Driver承接原source、raw/Wire、跨模块与O0/O2失败清理。

先用原三红+ZUI单文件把所有层贯通，再以独立raw近邻收口；不需要先建设一套新的通用callback框架。裸F→F原样转发、持久拥有handler容器与公开借入FnMut dispatch是明确后继，不混在这次必达接口里。
