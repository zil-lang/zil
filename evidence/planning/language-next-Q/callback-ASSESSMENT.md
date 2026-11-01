# Q 同步高阶拥有值：树外实测与下一切片建议

这是作者只读预检，不是实施交付、独立验收或正式全量。没有修改生产/规范/任务/既有 oracle，没有新提交。
CLI：`/workspace/.zil-env/target-236/debug/zil`，SHA256 `ebc418f594c91f1c54b4f655cf6fdd0d99a9461852aec8e1f7ad09701d433395`。调用前 `. /workspace/.zil-env/env.sh`。观察源码树 `/workspace/zil-core` HEAD `e92508be996a190fa188d08319d37cc8b522bae6`；CLI 的精确受测身份由二进制 hash 固定，不把观察 HEAD 当作嵌入版本证明。逐次精确 argv 见各 case 的 command.json；源/compile.json/compile.stderr/阶段产物/机器原件/运行流均保留。

## 实际结果

| 自然 API/边界 | Q 实际结果 |
|---|---|
| Copy `Event -> Reply`，泛型 `dispatch<F:FnOnce(Event)->Reply>` | LLVM API O0/O2 两次编译+真实运行均 exit0 |
| ByteBuffer Product `Event -> Reply`，同样 dispatch | Types Unsupported；无 native |
| 仅拥有 Event 入参，直接 concrete closure，返回 i32 | Types Unsupported；无 native |
| 无入参，move capture ByteBuffer，返回拥有 Reply | Types Unsupported；无 native |
| `keep<F:FnOnce(i32)->i32>(f:F)->F{return f}` | Expand Unsupported：escaping callable result is not implemented；无 native |
| 已有拥有 State<F>，`keep_state(state)->State<F>`，调用其 Copy callback 后进入 `Store<State<F>>` | LLVM API O0/O2 两次编译+真实运行均 exit0 |

有效成功机器运行共4次；三个拥有 callback 边界加裸 F 身份转发均未生成机器。六个自然 API 分组中第六组第一次夹具用了裸 State 构造，属 E-GENERIC-ARGUMENT 夹具错误，保留但不记产品缺口；第二次改写又碰 generic local-managed-give 既有限制，保留为 incidental Unsupported。最后使用既有合法 `State::<F>{guard:bytes_new(),...}` 控制成功。每次尝试分列于 RESULT.json / CONTROL-RETRY.json / STATE-STORE-CONTROL.json，不混计重跑。

## 源码因果与最短切片

1. **闭包拥有参数/结果不是 Store/State 准入缺口。** `zil-types/src/closures.rs:512` signature_parameter 仅 Copy/单层地址，supported_signature 仅 Copy 或已有 lending result；`inference.rs:3772` 初始 lambda 也提前拒绝。`closures.rs:1572` 参数模式仅 Infer/Copy/Move，不接受 Give。要冻结输入/输出模式，不能把函数环境的 Fn/FnMut/FnOnce 与每次调用消费 Event 混成同一种权限：无消费捕获的 closure 可以多次各消费一个新 Event。
2. **双核缺真实操作，不是只有声明门。** Analyze `resources.rs:2212..2304` CallClosure* 分支现只核环境/效果及 Once 消费环境，没有消费 owning arguments，也没有为 owning result 产生 Fact。Verify `closures.rs:200` 对签名走独立 Copy check_type；`resources.rs:1581..1599` 也仅消费 Once 环境，普通 Call 分支才逐个消费 arguments 并建立 fresh_result。这两份实现须独立补参数责任移交、结果责任、来源/借用写效果，不可仅前端授信。
3. **高阶 dispatch 也要一起贯通。** Analyze `helpers.rs:580` 仍限定 helper Copy/lending result；Verify 对应 helper public-envelope 校验也需扩展。现普通 helper 可带 owning extra parameters，并不等于被调用 callback 的签名可拥有。边界必须把 callback result 到 helper return 的路径和失败清理纳入既有真实 CFG 保守证明。
4. **IR/机器有可复用的实体表示。** `zil-ir/src/lib.rs:843` ClosureDefinition 已有 Vec<Type> parameters/Type result；现有 CallClosure* 有 arguments/result SSA，TypedFunction 参数本地有 PassingMode/result_mode。无须新增 Invoke opcode 才能表达 Product Value 所有权。`zil-types/src/lib.rs:2622..2725` 已有 callee staging、逐参数 staging、commit、具体 closure materialize；可复用并测试 ? 前后清理。`zil-backend-llvm/src/emit/resource_closures.rs:548..718` 共享/借入/Once 同走 emit_resource_closure_call，以 llvm_type 实体参数/结果；llvm_type 已支持 Package，Once 调用后还有环境 drop。机器层必须实际测 owning result 先保存再清理环境；不能从文本发射路径推定所有 ABI 边界已通。Lower 继续承接核后 CFG；先查原有 preflight，预计不需要新运行时表示。
5. **裸 F→F 转发是独立公共关系。** Expand `generic_callable.rs:730` 明拒 escaping callable。已有 CallableParameterContract roles/traces 的 Owned cleanup 表示 helper 结环境债；现 CallableReturnContract 是具体 opaque factory result（自包含），没有 source-parameter 身份对应栏。不要把 `keep(f)` 伪装成新工厂品牌，丢掉输入 F 的 Hold/Call/借入根。冻结最小输入角色→返回同一实际环境的公开原始关系（如 source parameter ordinal），Types 实例化保留 ClosureId+实际值，Analyze/Verify 独立从 return/branch/SSA 证明精确转发、无复制/漏债并搬运原 Hold/Call/Cleanup；是否以新枚举/字段编码由公共 IR 作者决定，不能默默授信已有 opaque result。先自包含拥有 F，再支持带外部 Hold 的转发，或一次冻结二者均可，但必须写明依赖。

建议最短可交付分两步（共同定义“同步高阶拥有值”，不扩 FFI/thin pointer）：

- A：普通同步 concrete closure + 泛型 dispatch 的有限 owning Product Value 入参/结果（叶 ByteBuffer/已核 Managed，复用现 Product 准入），先支持值型 owned reply，无外部借用结果。写集 Expand generic_callable/generic_check 的签名门；Types closures/inference/materialize/lib/helper；Analyze closures/helpers/resources；Verify closures/helpers/resources；必要 backend preflight/ABI小补；driver source/raw/Wire/O0/O2。共享谓词用于 FunctionPointer/ForeignCallback/Store callback qualification 时须隔开，不能顺手扩大 FFI/Store 合同。预计约12–18个生产文件，测试另计；不预先新增 schema，模式冻结若超现 PassingMode 再评审公共表示。
- B：`keep<F>(f:F)->F` 精确环境原样转发。依赖公开 return-origin 关系（Core+Typed+Wire/schema/预算/遍历/缓存哈希），以及独立 helper envelope/opaque boundary 证明；Expand、Types helper/factory及两核相关模块，backend通常复用现实体环境返回。预计8–14生产文件+公共schema/Wire接点。可以先冻结 B 的自包含 owning F，小范围但通用；不能宣称自动覆盖借入闭包。

必要最近反例：同一 owning Event 两次交入；已move Event字段偷读；Reply 分支漏造/双造；callback 未调用时 Event 必须清理；callee/第1owner实参已staging 后后续 ? 失败；先 return 准备再 defer/drop；FnOnce 剩余环境和 Reply 共享同一个叶伪造；Fn/FnMut 重复调用各获独立 Event；raw/Wire 错签名/漏消费/复制 result；keep 的返回别的同型实例、同值两返回/分支漏返、借入 F 超源窗口。O0/O2 原生清理字符序列应同时核调用前失败、调用后成功/错误，不能只 exit0。

Author scoped probe：Pass（按现有成功/Unsupported边界据实记录）。Independent/full/product acceptance：NotRun。未实现上述建议。
