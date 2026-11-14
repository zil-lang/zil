# stock TypedScalarFacts → direct SSA：精确候选独立源码复核

**裁决：ACCEPT（限定源码独审）。对 `30c0009ff94b3476c15f4afe2ba5d067f7f78cbb` 的实际入口和六文件差异，未发现确认的授权、SSA、overflow/trap 或回退阻断。该裁决不等于实际 native/暖路径通过或允许立即 merge；先前 aaa 暖路径回退不能由本报告改判为通过。**

本会话未参与该批实现，按仓库 AGENTS 和已读 goal workflow 的非作者精确候选要求复核。读取了新增 facts/SSA 全文、测试全文、emitter 接入与相关旧消费者；与 research 版本只作适配差异比较，没有把作者或旧研究结论代替本次论证。全程只读、不构建、不运行测试/native/benchmark、不改源码或 Git 状态；唯一写入为本报告。

## 身份、差异和文件边界

| 项目 | 完整身份 |
| --- | --- |
| 审查树 | `/workspace/zil-compiler-integration`，detached |
| 候选 | `30c0009ff94b3476c15f4afe2ba5d067f7f78cbb` |
| tree | `9d15a0b3e70e4e0a15138d54eb7ffb7956ad7f82` |
| 直接父提交 / 审查 base | `aaa72a6b84c097844d7484280992a1c7e4696ab1` |
| base→candidate canonical patch SHA256 | `6c01ff3a7e8d63c3bef4d157b8df589633e52a544af9d034df295f010c85ae53` |
| 实际差异 | 六文件，1809 行增加 / 28 行删除 |

起读时 HEAD 是 aaa、六文件 dirty；root 冻结后重新核 HEAD、parent、tree、差异与干净状态，后续结论锁定完整 30c SHA。没有在 dirty 状态就宣称精确候选通过。root 报告已推送；本审查没有执行 fetch/push 或独立核实时远端。

canonical diff 摘要的命令口径为：

```sh
git diff --no-color --binary --full-index --no-ext-diff --no-textconv --src-prefix=a/ --dst-prefix=b/ aaa72a6b84c097844d7484280992a1c7e4696ab1 30c0009ff94b3476c15f4afe2ba5d067f7f78cbb | sha256sum
```

| 文件（均在 crates/zil-backend-llvm/src 下） | 候选 SHA256 |
| --- | --- |
| emit.rs | `664c89301cd6d2ee4d0c6f282c1bc3d4204f06d62660f34db358433a133f348b` |
| emit/early_scalar.rs | `66e019a169f6489008b46d302b1ee791f287ace7f54977355c578f936a6bca98` |
| emit/typed_scalar_facts.rs | `262fcc4f1c5bcab71c1524e6d007852b6f77f6775939d6f4b092245c1ecbe920` |
| emit/typed_scalar_facts_tests.rs | `a6e6eb21a853d54ecbaf2d296c41c380561314ae02afaecd6ae8295ae00b0242` |
| emit/scoped_callback_tests.rs | `2c6b8009b8c3d820b57edb7566d613f46b89130164e1a118373702513ab8f72d` |
| emit/scoped_thread_tests.rs | `534f94ba58d39b32972b8de1be7bf7e043234be76f6814802cf4fc448b9797db` |

没有修改前端、Analyze、独立 Verify、公共 IR/Wire、model、Cargo/build.rs、SDK 或 native-link 配置。`git diff --check` 在该冻结差异上退出 0，无输出；这只是静态空白检查，不是编译收据。

## 当前 Verified / LIR / facts 的授权链

[emit.rs:147](/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit.rs:147) 的生产入口仍接收当前 `VerifiedModule`，先执行原工作量 reservation 和 [bind_lir:760](/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit.rs:760)，再构造各函数 producer。binding 不只核 fingerprint：还核 schema/module ID、target、实际 functions、closures、resources 与 scalar layout；原 static-UTF8 等价绑定入口未改。当前 primitive 域不会凭复制 fingerprint 把另一份 executable CFG 带入 facts。

[prepare 接入:658](/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit.rs:658) 位于原各项预算 reservation 之后；task worker 和有 task ownership 的函数不申请 facts。纯函数 fact 不是语言 capability，也不会补造、替换或跳过独立 Verify。

[Facts:126](/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit/typed_scalar_facts.rs:126) 私有地持有当前 TypedFunction 和 TargetProfile 的不可变 borrow；不 Clone、不序列化、不缓存到跨请求共享状态。每次取消 binary/negate guard 前，重新匹配当前 source/target 引用，以及精确 result ID、opcode、左右操作数或 unary operand；不能以函数名字、同字节克隆或 Analyze 的声明借用事实。finish SSA 也重新核 source/target、当前输出函数 symbol 和实际出口数量。

`std::ptr::eq` 在这里绑定已经通过完整 LIR 校验且被不可变借用的当前对象，并不单独代替 LIR 内容检查。新增测试还明确拒绝相同字节的 source/target clone、交换 operands、换 opcode 或 result ID。

## facts 的完整准入与范围证明

[admit / validate](/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit/typed_scalar_facts.rs:609) 限于支持的 x64 target、Direct、普通 by-value primitive 参数/结果/locals/values，排除 foreign、environment、storage life、reference/callable/protocol 合同等域。调用、借用/地址/投影/capture、原子、ownership、async 和未知指令全部使该 facts 请求 miss，不会按“返回类型像整数”授信。

Graph 要求密集 block/local/value 身份、唯一且完整的 value 定义、合法 CFG 边、当前 use 的同块先定义/跨块支配，以及值/结果类型一致。entry 不得有前驱；**任何环，包括不可达环，都拒绝新 facts**。`Terminator::Unreachable` 也拒绝新域，保留原 producer。全源校验不因某个分支或模块未执行而被裁掉。

数据流用完整前驱计数的 worklist，按拓扑顺序合并 local 范围。所有前驱都有同类型范围才取 union；任一路缺初始化/未知使该 local 变 unknown，不会制造零或乐观 bottom。没有读取 branch condition 后擅自收窄另一条路径；不依赖循环 widening 或未经验证的定点。

整数定义按真实类型和 x64 target，不取 host usize：覆盖 Bool 及有/无符号 8/16/32/64 位，Isize/Usize 为该受支持目标的 64 位。参数从完整类型范围开始。加减乘端点用 checked i128 算全部四个组合，且必须落在结果类型范围内，才记录精确 safe Site。u64×u64 端点超过 i128 时保守 miss，不把 host overflow 当作安全事实。

正的 singleton divisor 可传播有界 remainder/divide 范围；负号位运算按目标位宽解码；整数转换只传播成功转换后的范围交集。转换检查本身没有被删，失败路径也没有被这个交集“证明正常”。wrapping 算术在不能证明不越界时回 top，不把 wrapping 后未知值当成越界前的窄范围。

## 未知 overflow、首个 trap 与效果顺序

[transfer:895](/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit/typed_scalar_facts.rs:895) 仅为当前指令的安全端点建立 safe Site。未知 `x*3` 的结果按完整类型范围继续，不能因为后续 `%251` 变窄，倒推前面的乘法安全。后续真正依赖 remainder 的安全运算可以省去检查，但原第一个未知检查保留。

[binary:3108](/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit.rs:3108) 与 unary negate 分支只在精确事实命中时直接生成同宽普通 add/sub/mul，不附加 nsw/nuw。否则走原 `checked_binary` 的规范 overflow intrinsic/extract/trap 分支。没有先发射普通可能溢出的运算，再用另一条条件假装履行检查。

真实 [trap_if:3641](/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit.rs:3641) 仍跳向 `zil.trap`，后接 `checked.N` continuation；统一 trap 仍调用 llvm.trap 并 unreachable。除数的额外消检只复用原 `constant_divisors` 从当前完整 source 记录的 exact 常量规则；零、signed -1 和动态值不因新 facts 任意变安全。

facts 源域不含普通 call 或有可观察效果的 cleanup/ownership 操作，所以新 guard omission 不能跨它们重新排序陷阱或效果。包含这些操作的函数 miss facts，仍用原 producer、原机器 SSA/range 和原 cleanup 的效果边界。已绑定的工作预算及原资源责任没有被删除。

## direct SSA：locals、snapshot、phi 与真实出口

[SsaBuilder:181](/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit/typed_scalar_facts.rs:181) 用当前参数初始化 local state。Unit 参数不占 LLVM 参数槽，但其 source index 名仍与 emitter 的 `arg.<source-index>` 对应；非 Unit 参数不会因前面的 Unit 被错误重新编号。

Load 在 source 出现点读取当前 Operand，Store 把已解析的 operand 写入对应 local；alias 在生产计划中被即时解析到 Argument/非 Load Value/Phi/Unit，不把所有 load 指向最终 local 值。因此 `let old=y; y=...` 的 snapshot 不会变成后一次 store 的值。StorageLive/StorageDead 清状态；未初始化 load 使 direct-SSA 计划 miss。

join 为每个所有前驱已初始化且 operand 不同的 local 生成 source phi；相等状态不用 phi。前驱边保留重数，同一 predecessor 的重复边提供同一 operand；缺任一路初始化不以 undef 补齐。不可达 source component 会关闭 direct SSA，避免删除其 load 后留下悬挂 Value。

**phi 使用实际机器 guard 出口。** [emit.rs:1874](/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit.rs:1874) 在每个 source block 的 terminator 前保存最后实际 emitted label；`event` 追踪所有当前 Label，包括 unknown checked arithmetic/转换产生的 continuation。finish 按 `exits[pred]` 映射 incoming，检查实际 label 存在；不是直接拿 source bb.N 作前驱。primitive source 的 terminator 本身不再插入新的 guard，因此该记录点成立。

[finish_scalar_ssa:281](/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit/typed_scalar_facts.rs:281) 只接受恰好一个当前函数 body，向真实 source bb 对应机器块前置 phi，然后经完整 operand mapper 改写所有使用；definitions/results 本身不重命名。它拒绝任何遗留 alloca、普通/原子 load/store/rmw/cmpxchg。相应 raw 输出仍会进入独立 machine model/native preflight 和 LLVM verifier。

准入阶段完成之后 emitter 才跳过 local alloca、参数 store、source Load/Store。私有 `FunctionEmission.scalar_ssa` 只有在 finish 完整返回成功后才为 true；该 marker 成立才跳过后面的 `scalar_locals::promote` 和重复 `ssa::promote`。任何 finish invariant 错误直接让 codegen 失败，不发布半成品或在已删槽的输出上假装回到普通 memory 路径。

循环不走这个新 DAG producer。prepare 对循环返回 None，完整原槽 producer 继续，随后原 scalar_locals / early_scalar 的循环 SSA 仍存在。这是新优化的限定域，不是拒绝原合法 while/for 程序，也不能把旧 pass 处理了循环说成新 direct SSA 支持循环。

## ranges → cleanup → Select 与 fuel 回退

[early_scalar::optimize_produced_ssa:41](/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit/early_scalar.rs:41) 只由私有 producer marker 的成功分支调用：重新核当前 symbol/environment、模型大小、当前 Cfg；在 clone 上执行原 ranges 和完整 cleanup，二者全部成功后才一次替换原 machine。

这里省去的是已经由当前 source plan 完成的两次 memory promotion，不是省去 ranges/cleanup 或原生校验。源域只会产生普通 primitive 指令和既有规范 trap/checked intrinsics，所以该入口没有把旧“含普通 call 时关闭额外 CFG cleanup”的限制扩大到含任意 call 的函数。

三个回退层次区分明确：

1. 完整 facts 分析、CFG/类型准入或其 fuel 失败：不返回任何 Facts，保留旧 checked/memory producer。
2. SsaBuilder 的独立 fuel/完整初始化/可达性失败：SSA plan 丢弃，但已完整算出的安全算术事实仍可使用；内存 producer 保留，不能沿半计划删 Load/Store。
3. 后置 ranges/cleanup 超 fuel、CFG 拒绝或 cleanup 未收敛：丢弃它们的 clone，保留已经完成且有效的直接 SSA 输出；不是回退到丢失 alloca 的半完成模型。

facts 和 SsaBuilder 各有独立 1,000,000 optional fuel；blocks≤2048、instructions/values≤65536、locals≤512、状态 cells≤1048576。它们不提高语言默认额度，也不把 optional miss 变成 verifier 成功。后置 pass 仍有原有 fuel/大小限制。不能把这些逻辑缓存/工作边界夸大为精确进程 RSS 上限。

cleanup、ranges 及 Select 模型/native 消费实现本候选未修改，先前 stock 移植的边界继续适用：只有完整纯整数 diamond 可以搬动，普通 call/load/atomic/freeze/单臂 div/rem/shift 等仍不能被投机执行；matched remainder 先 Select 分子再做一次合法余数。未知首个 trap 阻止把其分支当 total arithmetic diamond。

Select 的三个 operands 仍经 model `values()`、mapper、preflight 的类型/支配检查完整遍历；native builder 仍调用标准 [LLVMBuildSelect](/workspace/zil-compiler-integration/crates/zil-llvm-api/src/native/builder.rs:540)。当前独立 model 检查与 LLVM 优化前/后 verifier 保留。无需研究版 SDK hook，也没有通过新 marker 取消后续这些检查。

## stock Vec 适配与相对 research 的差异

新 `typed_scalar_facts.rs` 与当前 research 文件逐字节相同，SHA256 均为 `262fcc…`；本审查独立阅读了其完整算法，不据相同哈希推定语义安全。相同 core 不使两个分支的公共表示、运行收据或 SDK 权限相同。

stock 的 `FunctionEmitter.text` 仍是 `m::Code`，完成时调用原 `into_items()` 取得 `Vec<m::Item>`，再私有返回 `FunctionEmission`；没有引入 research 的 ItemsBuilder / SharedVec。适配的实际顺序是 source Events 完整结束、解析成 Vec、finish SSA、range/cleanup、追加 body。

两个 scoped callback/thread 测试把原先的完整声明 Code 先转 Vec，再 extend emitter 返回的 items；补齐 scalar_facts=None、空 exits/label。原源输入、helper 构造及断言不变。不是删掉 callback/thread 的语义检查，也不会使这些域获得 typed facts。

新 facts 测试相对 research 的差异仅为 Vec 字段不再 `.into()`、`m::Code::new()`、`no_dyn_test_module` 使用明确切片及格式适配。完整断言、测试数据及重复边检查保留。没有修改 Cargo feature、链接方式、DSO/SDK pin、main/研究 flag；这是 stock 默认生产入口的源码移植，不是要求 SDK18 或 native-link profile 才能运行的新路径。

## 反例与实际 native 证据的边界

本候选新增三个测试函数及一个实际 guard-phi helper，源码覆盖：

- unknown 首次乘法不授 safe Site、rem 后安全乘法、原始 SSA 输出无 alloca/load/store、当前 model validator 接受。
- result/op/operands/source clone/target clone 绑定反例，以及 i8/i16/i32/i64/u64 端点、MIN×-1。
- 两臂 local union、缺初始化、不可达分量保留旧 producer、重复边计数。
- source arm 内 unknown checked Add 后 Store，再以 2+1 条重复前驱进入 join：断言实际 phi 有三 incoming、前驱均是 checked.N、重复项完全相等而另一臂 label 不同，并经过 model validate。
- address/BorrowLocal/call/cycle/borrow result/environment/非法最大 result ID/先用后定义均不产生 facts；逐 fuel 到首次完整成功前不改变 source。

这些是内部 raw-IR / producer / model 检查，**不是新增完整 Source→Verify→机器执行测试**，本审查也没有运行它们。原 Source/native driver 测试仍提供 load-time snapshot、十宽/Bool、branch/while/for、address/capture、原 overflow/SIGILL 和效果顺序的实际消费者；其测试源码未在此六文件差异中被放宽。

例如现有 scalar_local_promotion 对非借入 branch/loops 要求无 i32 alloca 且有 phi/Select，双后端×O0/O2 的真实返回值保持；verified_scalar_summaries 对 unknown maximum+1 仍要求真实 SIGILL/空输出，previous+1 正常返回最大值。它们需要在新 candidate 和正确 feature 下实际执行，不能拿原 aaa 的运行或 raw model validate 代替。

## root 收口与最终裁决

本报告未取得 30c 的最终测试、release、stock DSO 映射或暖路径运行收据，因此不写任何通过数量，不声称“全部 native 已过”。已有 aaa 的源码/测试适配审查可以说明未变消费者的合同，但不能关闭用户明确保留的 aaa 暖路径回退问题。

合入裁决应以 root 在完整 30c 上已安排的受影响门为准：真实 admitted leaf 经过当前 direct SSA、unknown 首 trap、branch 与实际 guard-phi、循环旧路径和 native 行为；编辑/恢复/暖请求仍当前 Verify，不能把上层 cache 命中或 fallback 成功当作该新 producer 已执行。Unsupported、超时、缺 feature/零选择、LLVM/native 失败或回退收据都不能用组件计数覆盖。

无需因本报告另开重复广泛构建；root 可按实际差异复用未受影响收据，集中完成其既定 stock 验收并保留首次失败。先前 research 的性能、全 ELF 差异诊断、SDK/native-link 和缓存计数不作为 stock 30c 的 native 或性能通过证据。

**最终：六文件源码独审接受，暂无确认的修复项；30c 的实际 native/暖路径及集成裁决仍归 root。源码接受不撤销已有暖路径阻塞，不自动授权 merge。**
