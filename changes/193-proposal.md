# 193 提案：同步回调的显式有限 Copy 切片参数

## 决策与当前状态

建议在 189 的同步地址参数实现上，开放一层 `&[T]` / `&mut [T]` 作为显式 callable 参数以及非逃逸 generic helper 的附加参数，`T` 限既有有限 Copy 值，新开放的入口保持 Copy 结果。这个入口直接服务文本读取、原地缓冲编辑、同步字节访问器。

本提交仅有提案、真实源面输入和观察记录；没有生产修改、任务登记或新 schema。193 尚未实施，也未验收。所有探针使用既有 189 CLI，不能归属当前 main 或 191/192。本文源码位置以 `9bb141a327cf2433efd755e24551409f4c17caab` 为准，正式实施应在 root 冻结的新主线基点复核。

| 身份 | 精确值 |
| --- | --- |
| 只读源码 / CLI 构建源码 | `9bb141a327cf2433efd755e24551409f4c17caab` |
| 其对应生产源码 | `bfe60582c46526a75349d4eefd3c60fde8f327e6` |
| CLI SHA256 | `c705c0d356209fe672ad6e1452bb174ed15709f60850a50e44c8217d0e9ff499` |
| 设计 1.4 SHA256 | `8df5f5bb3cb25fdea8670124b09941713529c859ab18864d0fc87090864786d5` |
| 预检保存基点 | `9505afd923679fd78ecd3cca4057f3038edc414a` |
| 实际操作 | 19 次 `zil check … --stop-after verify --json` |
| 已发生的原生 / raw-Wire 验证 | 0 / 0 |

CLI 的文件散列与原 `189-review/build01/command/command.json`、Linux / Win64 审查 summary 交叉核对；其中既有平台执行不计作 193 的执行。精确文件在 `evidence/preflight193/compiler/`，19 个输入和未经改写的 stdout/stderr 在 `evidence/preflight193/probe01/`。

## 1.4 依据与需由 root 登记的准入变更

- §4.1：Address / Hold 约束实际目标，当次调用期间的资格不能以“最后一次使用”绕过。
- §5、§9.3：高阶实际效果在调用点实例化；`Fn` / `FnMut` 环境模式与显式参数自身的 shared / exclusive 资格分别检查。无状态 `Fn(&mut [T])` 是合理入口，不能因为显式参数可写而强制环境变成 `FnMut`。
- §9.4、§9.7：opaque 不抹除来源、Hold、Call、Cleanup 和模式；捕获与显式实参的实际根必须联合检查，不能用类型或不同参数名推定不相交。
- **§15.2** 的 schema16 有限借入切片段：已有普通切片来源、范围、重借、有限 Copy 元素、保守覆盖整个后备对象、`{ ptr, i64 }` 表示和 Linux / Win64 checked profile；同一段明确暂时关闭 callable/helper 切片参数。193 的正式落地需要 root 追加限定开放条文并登记任务，不能把这个历史关闭条件当成已获实现许可。

建议 root 的新增条文准确区分：本轮放开的是同步调用期间的显式参数，不赋予参数窗口任何捕获、返回、跨调用留存或隐藏效果的能力。既有 184 的环境借出结果只按原 profile 保持；193 不把显式借参加入 lending 来源。

## 实际源面预检

完整逐例结果含阶段、诊断、查询阶段、`verification_runs`、命令和散列，见 `evidence/preflight193/probe01/preflight.json`。下表的“193 目标”是提议的后续验收 oracle，不是已经完成的结果。

| 输入 | 现有 CLI 实测 | 193 目标 / 所验证边界 |
| --- | --- | --- |
| `ordinary_shared` | Success / Verify | 普通 `&[i32]` 函数参数对照 |
| `ordinary_exclusive` | Success / Verify | 普通 `&mut [i32]` 读改写对照 |
| `ordinary_shared_write` | SafeReject / Types / `E-ADDRESS-PERMISSION` | 共享窗口不得写入 |
| `ordinary_desc_same` | SafeReject / Verify / `E-BORROW-CONFLICT` | 普通函数：同后备缓冲的 Desc 扩容与切片实参冲突 |
| `ordinary_desc_other` | Success / Verify | 普通函数：不同真实后备缓冲不被统一封禁 |
| `ordinary_desc_read` | Success / Verify | 普通函数：同缓冲只读 Desc 与 shared 切片相容 |
| `callback_shared` | Unsupported / Types | 显式 shared 切片参数应 Success |
| `callback_exclusive` | Unsupported / Types | 显式 exclusive 切片参数应 Success |
| `callback_unused` | Unsupported / Types | 不调用的合法显式签名也应被准入 |
| `callback_shared_write` | Unsupported / Types | 准入后应为权限 SafeReject；本次尚未测到该权限检查 |
| `helper_shared` | Unsupported / Types | `F: Fn(&[i32]) -> i32` 加 slice 实参应 Success |
| `helper_exclusive` | Unsupported / Types | `F: Fn(&mut [i32]) -> i32`、环境 `&F` 应 Success |
| `helper_extra_only` | Unsupported / Types | 即使 callback 是 `Fn() -> i32`，helper 的附加 slice 参数也受独立准入门限制 |
| `factory_signature` | Unsupported / Types | 无 slice 捕获的自包含 factory 返回 callable，其 slice 参数签名应 Success |
| `callback_desc_same` | Unsupported / Types | captured Desc 写入同一实际 backing 应在 Verify SafeReject，即使调用后无再次使用 |
| `callback_desc_other` | Unsupported / Types | captured Desc 写入另一真实 backing 应 Success |
| `callback_desc_read` | Unsupported / Types | 同 backing 的 readonly captured Desc 与 shared slice 应 Success |
| `callback_exclusive_same` | Unsupported / Types | 同 backing 的 shared + exclusive 显式实参应 SafeReject |
| `callback_exclusive_other` | Unsupported / Types | 不同 backing 的 shared + exclusive 显式实参应 Success |

汇总为 **4 Success、2 SafeReject、13 Unsupported**。13 个回调 / helper / factory 用例的 `verification_runs` 都为 0，没有执行 Analyze / Verify，不能从这组结果推断回调 loan 已正确或已失败。最早入口诊断为 `closure parameters require a finite Copy type or one Copy address layer; address parameters require a Copy result`；`helper_extra_only` 则命中独立 helper envelope 的有限参数诊断。

一个最小目标输入如下。其原始版本已作为 `callback_exclusive.zil` 实跑，当前仍在 Types 拒绝：

```zil
fn main() -> i32 {
    var values = [7, 9]
    let f = (s: &mut [i32]) => {
        s[0] += s[1]
        return s[0]
    }
    return f(&mut values[..]) - 16
}
```

## 精确最小写集

以下是正式派工建议，不是本提交已修改的文件。需要改动的生产行为集中在 **8 个文件**；另有两个诊断文本文件可同时精确更新。各 checker 保持独立验证，结构谓词只做准入形状分类。

| 文件 | 必需修改与保留条件 |
| --- | --- |
| `crates/zil-types/src/closures.rs` | `signature_parameter` 允许 `Address(inner)` 且 `inner.is_address_pointee()`；`supported_signature` 保留含 Address 参数时结果必须 Copy。`Graph::bind` 的 Address 分支同步准入 finite Copy slice pointee，特别是 **mutable slice**，因为 shared slice 当前可偶然经 `lending::supported_result` 进入，而 mutable slice 不可。核对 `Graph::solve` concrete Address 投影分支，不能仍仅拆 `inner.is_copy_value()` 导致 slice 元素推断失配。保留 `capture()` 对 slice 的关闭；裸 Slice 只作为 pointee 约束节点，不能因此变为值参数。 |
| `crates/zil-types/src/helpers.rs` | `validate_template` 的非环境附加参数分支加入同一层有限 slice 地址，新增 lane 要求 helper Copy 结果；回调签名继续委托 `supported_signature`。保留唯一非逃逸环境、FnMut/FnOnce 模式与已有限 lending 来源规则。不能直接开放任意 Address。 |
| `crates/zil-analyze/src/budget.rs` | closure 参数 preflight 在 Copy 结果前提下，独立检查一层 Slice 的有限 Copy 元素，复用既有 raw 类型计费。不得对裸 Slice 调 `scalar`，不得改 Copy 定义或上调默认预算。保留 slice capture 拒绝。 |
| `crates/zil-analyze/src/helpers.rs` | 附加参数 envelope 准入一层 slice 地址并匹配 Types 的 Copy 结果 lane；保留既有环境来源与有限调用合同检查。 |
| `crates/zil-verify/src/closures.rs` | closure 参数独立准入一层合法 slice 地址，使用已有 `slices::check_pointee` / 独立类型检查验证元素；含 Address 参数仍须 Copy 结果。不能只信 Types、Analyze 或 `ClosureDefinition` 中的声明。保留 `slices::is_window(capture)` 拒绝和 lending-capture 限制。 |
| `crates/zil-verify/src/helpers.rs` | 独立检查 helper 附加 slice 参数形状与 Copy 结果 lane，继续按实际 entry / operands / contract 验证。保持来源不依赖类型 ID 或 producer 空合同。 |
| `crates/zil-lower/src/lib.rs` | 仅扩 closure `parameters` eligibility 至一层 finite Copy slice address，Copy 结果条件不变；环境 capture 分支保持原准入。 |
| `crates/zil-backend-llvm/src/emit.rs` | 同步 closure `parameters` eligibility；已有 `llvm_type(Address(Slice)) == "{ ptr, i64 }"`，无需新布局、隐藏分配或 ABI 重新设计。capture layout 过滤不扩张。 |

诊断文本相应更新建议：`crates/zil-types/src/inference.rs` 的最早参数拒绝提示、`crates/zil-types/src/retained.rs::validate_factory` 的有限签名提示。两处行为已经依赖 `closures::signature_parameter` / `supported_signature`，没有第二套 slice 推断算法要实现。`inference.rs::name` 只在 `binding.owner != self.owner` 时拒绝外层 slice，这是 **捕获门**，必须保留。

最小新增验收与使用文件建议：

- `crates/zil-driver/tests/impl193_slice_arguments.rs`：真实源面、跨模块、helper 与平台执行入口。
- `crates/zil-driver/tests/impl193_slice_arguments_raw.rs`：独立 raw / Wire 对抗、入口一致性、预算边界。
- `examples/mainline/slice193/{main,factory,helpers}.zil`：固定输入、可核对输出的文本 / 缓冲 consumer；factory 只捕获既有许可内容，不捕获 slice。
- `tools/language/193/consumer.py` 与 `tools/language/193/review_platform.py`：沿现有精确源码 / CLI 散列 / 原始日志格式保存一次验证及平台执行。
- `changes/193-proposal.md` 与 `evidence/impl/193/`：实施说明及紧凑证据。root 独占 `_zil_design.md`、任务登记、schema 注册和主线整合。

预计不需要修改 parser、Core/Wire 字段、`zil-ir` schema/hash 编码、结果来源合同、环境布局或 `emit/resource_closures.rs`。类型本身已存在，`ClosureDefinition.parameters` 与五种 `CallClosure*.arguments` 已携带真实 operand；普通和 resource closure 的调用发射均逐个用 `llvm_type(actual_argument)`。

## Loan / actual backing 的既有接点与剩余风险

以下是源码证据，不是尚未执行的回调行为证明：

1. `crates/zil-verify/src/borrows.rs` 从 `ByteBufferBorrow` 的真实 buffer operand 继承来源；`ReborrowAddress`、`AddressSlice`、`AddressIndex` 继续沿实际 address 继承根。不能以新 slice SSA ID 或指针值冒充新后备对象。
2. `crates/zil-verify/src/borrows/loans.rs::callback_arguments` 对所有 Address 实参执行 `check_pairs` 及 active 资格检查，没有只接受 Copy pointee 的过滤。五种 callback 指令在调用时把 captures 与实参 loan 加入 active，因此最后一次调用也必须检查。
3. 同文件 `check_descriptor_callback` 把环境所持窗口加入当次检查；`borrows/captures.rs` 根据实际 captured 来源实例化 Desc 读写效果。shared slice 与 captured Desc 写入相同 backing 应失败，different backing / readonly 控制应成功。
4. `check_pairs` 不支持按不同 slice 区间证明互斥；`roots.rs::intersects` 与设计一致，subslice 仍保守覆盖整个 backing。193 不新增区间不相交证明。
5. `crates/zil-verify/src/slices.rs::check_pointee` 和 `lib.rs::check_type_shape` 已分别独立检查 finite Copy 元素、目标 profile、嵌套与预算。放开 closure gate 后仍必须经过这些入口。

因此 loan 文件当前 **不列入初始必改集**。正式作者必须通过下列 raw/Wire 与实际源面同源/异源用例确认复用是否充分；若具体失败暴露未覆盖路径，再将最小根因修复提交 root 评审，不能先泛化重写借用系统，也不能把这些用例继续在 Types 拒绝计为安全通过。

## 193 必须完成的验收矩阵

### A. 真实源面及 ABI

以现有 19 个原输入作第一轮回归，不通过改写输入规避差异。显式 shared、exclusive、helper extra-only、Fn / FnMut / FnOnce 的环境传递分别覆盖；尤其保留 `Fn(&mut [T])` 的环境 / 实参模式区分。支持自包含 factory 的 callable 签名通过跨模块导入；没有 slice 捕获或参数借出结果。

补充单次求值的范围端点、全片 / subslice / 显式重借、`.len()`、空范围与零大小 Copy 元素、有限 Copy record / array 元素、顺序重借两次、混合 Copy 与多于寄存器数量的 slice 实参。原先 189 的一层普通 Copy 地址回归必须保持。

对正常与边界 consumer 各实跑 Linux O0 / O2，并按现有工具实跑 Win64 / Wine O0 / O2；若平台不可用，精确记 NotRun，不能替代为 native Windows。记录完整输出和退出码，核 LLVM 的 `{ ptr, i64 }` 实参以及没有因借入 slice 新增分配。保留范围越界的 checked 失败；不以源码字符串中存在 slice 计为 ABI 已通过。

### B. 权限、来源与当次调用

- shared slice 写入必须为权限 SafeReject；raw 绕过 Types 后仍被 Verify 拒绝。
- `callback_desc_same` 与 `callback_desc_other` 只改变真实 backing，前者 Verify `E-BORROW-CONFLICT`，后者 Success；same backing readonly 也应 Success。当前预检三个都停在 Types，这组必须重新实际跑到 Verify。
- 同 backing 的 shared / exclusive、exclusive / exclusive 实参冲突；不同 backing 成功；subslice 区间看似分离也不能自动获得独占互斥。
- 临时最后一次使用的 slice / callback 仍受当次资格约束；顺序调用完成后的 dead 窗口不应无限延长贷款。
- helper 转发、重借 parent、环境 capture 与显式实参共同出现时，依据真实来源检查，不能用参数名、相同/不同类型或删除 future uses 判定安全。

### C. 独立 raw 与 Wire

至少把真实通过样本直接送 `verify_module` 与 `verify_module_bytes`，并在相同原始模块上逐项变造：closure signature 与 entry 参数不符；shared 伪造 mutable；删改 reborrow parent；替换成同源实参；伪造 / 删除 candidate Hold；改变 slice 元素或范围 operand 类型；裸 Slice 参数 / 值；嵌套 Address / Slice、非 Copy / owning 元素、slice capture、显式借参返回。每例保留 phase / category / code 和原始 operand，禁止仅靠 stale fingerprint 拒绝。

特别保留已有环境借出结果的安全语义：193 含 slice 参数的回调结果必须 Copy；显式 slice 参数不是可声明的 `lending_captures` 来源，空环境或 forged capture 索引不能绕过。

### D. 有界性与完成门

在至少一个真实 slice callback 模块上实际测最小 Verify 工作预算 `N` 和 `N-1`，后者必须 ResourceLimit；类型深度 / 参数数过限同样保持有界拒绝。不得提高默认预算，测试阈值若有变化须绑定原输入、生产源码和计费原因。

作者有界集 → 原始记录冻结 → 独立 review 使用冻结源码 → root 主线回归。只有预期成功输入跨 Types / Analyze / Verify / Lower / LLVM 并实际执行，安全负例到达所声称层，raw/Wire 与预算满足，才可将 193 标记完成。此提案不宣称语言全量完成或 ZUI 产品已运行。
