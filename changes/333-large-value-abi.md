# 333：一般大 Copy 内部按值 ABI

## 原源实测（实现前）

基线40fbcca6，自建target333、显式llvm-api、jobs1，原
`tests/language-readiness/codegen-pending/large-array-a18-by-value.zil`未改，
SHA256 `3acb74e1ffc85123469f402d2a3c02178d267543b7928692ad9e01547d9a7791`。
每组CPU45s/VM2GiB/wall55s、core0、原生等待5s。LLVM23.1.4/Linux x64：

| 请求 | 实际结果 |
| --- | --- |
| Clang O0 | 23.190s，Verify1后link InfraError/E-LLVM-COMPILE；cc1明确Segmentation fault及SelectionDAG回溯，外层工具SIGABRT；非OOM |
| LLVM API O0 | 25.223s，编译器SIGSEGV(-11)，无最终JSON |
| Clang O2 | 51.541s，工具报告Killed；CPU受限运行，不宣称无约束优化逻辑崩溃 |
| LLVM API O2 | 47.293s，编译器SIGKILL(-9)，无JSON、未触发55s父wall超时；资源受限未完成，不宣称无约束逻辑崩溃 |

本地结果/tmp/zil-333-probe/results.json；不归档大回溯，不缩源或改优化级别。

## 待 root 冻结的合同

333复用324已建立的>=4096B有限Copy Array/Tuple/Record独立快照与私有copy helper。
不把Source Value改成Address，不制造NoAlias或隐式heap。不只修改Call端：统一
计划定义/参数入口、返回、直接调用、FunctionRef和CallIndirect的完整机器签名。

建议内部大参数用指向已准备独立值快照的指针，在callee入口复制到真实参数local；
大结果由caller固定结果槽的显式outptr承接，callee在真实Return位置写完整结果后
返回void。保留原源求值顺序，所有普通实参与outptr位置一致；不能把可变local的
定位代替调用前值快照，多个同型实际参数仍是独立值。闭包的隐藏env角色保留，
若允许大显式参数/结果进入闭包入口，则所有closure调用模式也须消费同一ABI计划。
ABI按真实完整类型、目标和定义角色决定，不按名字或相同布局推断源类型相等。

FunctionPointer从其完整参数/结果类型重建相同ZIL机器签名；FunctionRef与间接调用
必须一起迁移，不能将foreign callback指针默认为ZIL内部指针。foreign声明及C
callback保持旧受支持C ABI，不以内部indirect规则扩大外部按值聚合准入。

CGU声明、跨单元引用、完整call-closure副本和函数地址身份须保留统一签名；完整
machine项/ABI版本进入对象、layout和patch缓存键，旧候选不能复用错误签名对象。
新增计划、类型快照与发射工作用当前Verified limits模块累计预付，N/N−1/fresh
和冷热对象控制须实测，默认额度不提高。

### 导出边界的实际阻塞

当前TypedFunction无pub/export字段，所有ZIL机器函数均先发为External。
默认生成符号导出合同在效，只有显式closed-executable放弃该合同。因此不能用
“Source私有函数”猜机器入口可任意改签名。保留旧LLVM大SSA wrapper本身仍可能
再次触发该故障，不能用一个未证明可编译的adapter冒称ABI兼容。

root需明确选择：证明并实现与既有目标机器ABI兼容的独立adapter编码；或者为受
影响大型ZIL生成入口明确发布版本化ABI/符号及迁移规则，而非暗改既有导出。
若仅closed模式允许迁移，要明确原默认构建仍未完成，不以该模式替代原探针。
所需public machine typed attrs（如byval/sret）由root持写；实际位置在
`crates/zil-llvm-api/src/model.rs`，文本/preflight/API须同批完整消费。

本片写集先限共享backend与已有aggregates/机器测试；未冻结导出边界前不实施
破坏性签名变化。普通大Enum/ErrorUnion及大型环境内部值构造的旧SSA桥接也须
明确范围，不把只修普通数组调用宣称所有大值机器路径完成。

## 冻结与实现

root在e28e08d8825085e9a3c6bae57d2a85d3a2418481冻结§19.4：选版本化迁移，
受影响生成入口后缀`.lc1`、仍External；参数copy-in指针，大结果首hidden outptr，
retvoid；小值/foreign C符号与ABI保持。无旧大型SSA兼容wrapper。
共享Emitter同时消费definition/call/ref/indirect/return；ABI计划与324 snapshot
计划共用同一剩余Codegen额度，额外header/函数指针完整类型布局检查确实收费。
CGU按实际版本化符号分组、导入和声明；完整machine签名/名称继续进入原对象键。
未贯通的closure entry/foreign/scoped-thread大签名提前Unsupported，未消费大snapshot
不再临时Load回巨型SSA：发射记录Unsupported并在返回机器产物之前终止。

## 有限验收的实际结果

生产写集：共享`emit.rs`、`emit/copy_aggregates.rs`、新
`emit/large_copy_abi.rs`、`machine_cgu.rs`；两个既有scoped模型构造仅补私有
ABI计划字段。没有IR/Types/driver生产修改，也没有新公共machine attrs。
计划在324的同一`work`剩余额度内支付参数位置映射、完整类型遍历/布局和保留；
ABI planner没有新建默认预算或reset。每个callee参数入口仍有独立local，复制发生
在跳入任何源block之前；返回写caller独立结果快照后retvoid。按逻辑源参数位置
擦除Unit，不挪动实参求值时机，调用传准备好的SSA快照而不是源local地址。

LLVM23.1.4，target333/jobs1，显式`--features llvm-api`：

- 既有aggregates整target在新增borrow正控前31/0；最后ABI七测7/0，覆盖最终32个
  target用例的并集。原a18参数、a18返回、a18函数指针三源码共12个native组合均过：
  Clang/API各O0/O2，实际backend、请求opt/pipeline、fallback=null、native0均断言。
  各原scale子进程CPU45s/VM2GiB/wall55s/core0，native5s；原参数源SHA仍为上表原值。
- Copy-in/RHS次序、双大参数、record/tuple/Unit/padding、大tuple返回、Unit参数擦除/
  void返回、小array旧签名均有实际原生消费者。新用例共36次native执行（含12个
  CGU/cold-warm-body-change运行），不将对象构建次数算native次数。
- 三模块canonical函数地址跨CGU声明/引用，两后端O0/O2实际units>=2、无fallback；
  API冷miss、暖全hit、callee body改后miss并返回新值。Clang不请求其未支持的
  object cache。既有layout action key和object key确实hash完整machine项，包括
  `.lc1`名称/签名及copy helper attrs；不是仅source fingerprint。
- a18函数指针Codegen精确N=356839，N−1为Codegen ResourceLimit，N两次fresh成功。
  真实API对象缓存暖后同源N−1仍在Codegen拒绝且无executable artifact；恢复预算
  再暖hit成功。324旧local-a18的精确N由323040增为323041：新增main header映射1，
  没有隐瞒新费用或提高默认额度。
- backend现有machine_cgu模型16/0、scoped模型5/0；backend两feature配置
  all-targets Clippy `-D warnings`、API driver aggregates Clippy、fmt通过。
  默认ELF实际`nm -g`显示`T zil.fn.0.lc1`，保留External；未internalize生成入口。

重放主正控：

```sh
source /workspace/.zil-env/env.sh
export CARGO_TARGET_DIR=/workspace/.zil-env/target333 CARGO_BUILD_JOBS=1
ulimit -c 0
cargo test -p zil-driver --test aggregates --features llvm-api --offline large_copy_a18_by_value_abi_native_all_backends -- --nocapture
cargo test -p zil-driver --test aggregates --features llvm-api --offline large_copy_abi_ -- --nocapture
```

## 已知边界：大move capture与API线程栈

`tests/language-readiness/codegen-pending/large-copy-a12-move-capture.zil`保留550字节
最小实际CLI输入，a12=4096B；SHA256
`6ba91840b41b64ca098d57ca29778bd862b8e5aada06a0ffd546c6368077b1b8`。
普通CLI线程下同源`check`为Verify1 Success；`build --entry result`为Codegen
E-UNSUPPORTED，无LLVM/executable。这是尚未贯通的大物理move环境，不能冒称已支持。
近邻直接mutable借入同一Big源变量的closure实际环境字段为Address<Big>（一个ptr），
两后端O0/O2原生输出45均成功，guard没有按原数组referent盲拒。

独立的KnownIncomplete：Rust API在2MiB工作线程处理该move capture时，明确在
Verify API调用开始与完成标记之间stack overflow/SIGABRT，尚未进入Codegen。
在6b首次固定该复现时，Syntax/Expand/Types/Analyze/Verify/Lower源码相对40fb基线零差异；这是未修的API
小栈可靠性问题，不计成Unsupported或ABI修复成功。未重建旧baseline binary来
声称旧binary实跑过。默认8MiB测试线程只匹配本轮普通CLI测试前置，不改变compiler
线程栈、预算或语义，也不把8MiB通过宣称2MiB问题已修。core0、无repo core。

```sh
# 正常CLI阶段；先按上述命令构建/使用当前target333 CLI
ulimit -c 0; ulimit -t 15; ulimit -v 2097152
timeout -s KILL 20s /workspace/.zil-env/target333/debug/zil check tests/language-readiness/codegen-pending/large-copy-a12-move-capture.zil --json
timeout -s KILL 20s /workspace/.zil-env/target333/debug/zil build tests/language-readiness/codegen-pending/large-copy-a12-move-capture.zil --entry result --backend llvm-api --opt O0 --emit-dir /tmp/zil-333-capture -o /tmp/zil-333-capture/program --json
# 故意重放未修的小栈API失败，非通过测试
ZIL_333_CAPTURE_STACK_BYTES=2097152 cargo test -p zil-driver --test aggregates --features llvm-api --offline large_copy_abi_special_closure -- --nocapture
```

未贯通大型closure显式签名/环境、foreign或特殊helper产物前仍明确Unsupported；
本片不声称全部大型Enum/ErrorUnion或所有callable/外部C ABI已支持。初期测试脚本
曾误向Clang请求仅API支持的object cache、额外capture测试曾漏选result入口，均已
修正测试请求；未将这些InfraError/SafeReject或2MiB栈崩溃计入正控。

## 非作者阻断后的版本传播修补

root/impl317在6b确认：factory返回受影响薄fnptr、apply接收fnptr、小record携带fnptr
仍同名，旧机器消费者可误链接。原先仅物理Slot决定后缀不足，此前交付不得视作版本
隔离验收。按root规范5eb792faf，分离`Signature.versioned`与物理`large()`：递归
完整公开参数/结果中的language fnptr与载体依赖决定后缀，copy-in/out形状不扩大。
普通Address<Big>只传数据、无受影响fnptr，保持旧名；foreign callback不混进ZIL
函数指针约定。依赖walk与旧snapshot/ABI layout共同扣同一个remaining，逐节点预付，
不reset。普通小物理direct call也必须使用实际abi_symbol。未贯通closure显式接口/
隐式env、resource/teardown等helper若带该依赖，在产物前Unsupported。


### 版本修补的固定验收结果

规范5eb历史合并带入root330/331/332共同生产；本作者只改上述backend版本传播。
因此“六frontend零diff”只描述6b原始发现时点，不描述修补后的整个共同树；本轮
不宣称其它root生产已经全仓回归，也未用大测试栈结清已保留的小栈可靠性问题。

- 审者原扩展source原样保存`codegen-pending/large-copy-version-carriers.zil`，
  SHA256 `c4b67bd65c0c5436c34b42e9e0b3db769c52dcf42899109a72ff780e545544ba`。
  两后端O0/O2实际native0，nm全部0–8为External T `.lc1`且不存在旧T名称；
  9–12（纯&Big、小array、小fnptr工厂、main）保持旧名。factory/apply物理header
  仍ptr。真实C机器consumer旧`zil.fn.1`链接undefined reference，`.lc1`可链接。
- structural模型2/0：完整carrier依赖、纯数据Address<Big>旧约定、普通C callback
  旧约定、禁止C callback混入受影响language pointer；共享剩余额度N/N−1/fresh。
- API aggregates本轮ABI限定8/0；其中五类special正源先Verify1再Codegen
  Unsupported，无LLVM/executable：物理大closure参数/结果、大move capture、
  薄受影响fnptr显式closure接口、薄受影响fnptr环境、resource helper。合法
  mutable借入Big物理ptr环境仍四组合native45。没有按源referent大就盲拒。
- 同一个累计预算下a18函数指针实际N从356839增为356960，N−1仍Codegen
  ResourceLimit，fresh与API暖缓存控制均通过；旧local-a18从323041增为323042。
  增费来自真实完整signature dependency遍历，未提高默认额度或新开预算。
- 三模块CGU/cache标准O0/O2控制已过；另将O2明确开启incremental_o2完整直接调用
  副本模式后，两后端cold/warm/body-change三轮实际native也过，仍实际units>=2、
  无fallback、API暖全hit，引用保持canonical版本化fnptr。
- 修补后backend两feature alltargets Clippy、API aggregates Clippy/fmt通过。
  旧整target31/0是6b收口历史，不冒称最终共同树33用例已整target复跑；本轮按
  root要求只重跑受影响限定控制，原a18独立非作者复验由impl317绑定最终交付。

补充只读边界核查（root收尾问题）：一般Enum/ErrorUnion本体>=4096B并没有统一的
Codegen Unsupported门槛。333 physical classifier只选择Array/Tuple/Record；
`llvm_type(Enum/ErrorUnion)`仍inline aggregate，enum layout的现有拒绝是shape/
地址范围溢出检查，不是4KiB拒绝。部分构造/投影碰到未消费ordinary大snapshot会
明确Unsupported，但不能推广为所有大Enum/Error都已拒绝。故一般大型Enum/
ErrorUnion内部按值可靠性仍是本片未支持/未验收范围；本次只读确认没有新增LLVM
崩溃探针、没有推测为已复现新缺陷、没有扩大本片生产修复或宣称完整大Copy支持。
