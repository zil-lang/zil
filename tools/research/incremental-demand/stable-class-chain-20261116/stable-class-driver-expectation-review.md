# full_profile_history 断言只读复核

基线：`74798f47c52b075f5307b5a0c4443d3213756b6c`，直接读取该 commit 的源码。范围为 `crates/zil-driver/src/project.rs:3263–3504` 的完整测试及其计数消费者。未运行测试、编译、fixture 生成器或性能窗口，未修改源码。以下为静态预期，不是新的运行收据；performance pending / not claimed。

结论：除 root 已准备修正的 Types 计数外，**positions 的 publication 旧上界也必须修正**：真实静态路径为 **serialized/reused = 4/421**，不是 ≤2/≥423。private/recovery publication 的 **1/424** 和 private Verify flow proof 的 **1/422** 应保留。其余现有断言未发现新的静态冲突。

## 三个计数域

fixture 为 20×21 个生成函数、1 个 main、io 的 2 个普通函数和 2 个 foreign 声明，共 **425** 行。非 foreign 是 **423**；能够产生当前 sealed scalar CFG/canonical handle 的是 **421**。

`io.read/emit` 含 unsafe，不能把 ProducerMemo 的输入命中当成 CFG 命中。更精确地说，该 SHA 的 `BodyKey::block` 未接纳 `CoreStatementKind::Unsafe`，落入 `incremental.rs:1942` 的 `None`；foreign call 本身也被 `1963–1964` 排除。ProducerMemo 仍保存已完成的 `key: None`（`1192–1217`），命中时照常累加 `body_key_reuses`（`1168–1183`）。所以 **423 个 key 查询结果包含这两个完成的 None**，但只有 421 个可缓存 CFG。两个 foreign 声明由 worthwhile 排除，仍执行普通 Types 路径。

## 逐组断言

| 请求/字段 | 应保留或修正的预期 | 判断 |
| --- | --- | --- |
| fixture | 22 文件；现有 cold 498,620 bytes 断言不需随计数修正变动 | fixture 未改；本次未执行生成器重新计量字节。 |
| 每个成功请求 | Success、Verify runs=1 | 保留。原检查和 Verify 入口不因缓存省略。 |
| cold 父历史返回（index 2/4/6/8） | Resident；function_type_work=None；有 producer_epoch_activation，无 semantic_cache_read | 保留。激活保留的完整 Types/producer/publication epoch，不把历史返回统计为本次函数检查。 |
| broad→cold（index 6） | parser_executions=20 | 未见新冲突。broad 只改二十个 m 文件，root/io 字节未改；原断言针对实际 parsed-discovery 父返回，不改为零。 |
| private | body keys **1/422**；Types **5/420**；cfg_relocated_functions=20 | key 断言保留；Types 旧 **3/422** 改正。执行为 changed h0 加四个不可缓存行；m0 其余十九个 h 和 value 共二十个 CFG 需要位置重绑。 |
| private producer | function_input_hashes/reuses **1/424**；text_comparisons=21；binding_hashes=21 | 保留。只有 m0 的 h0 声明字节改变，m0 共二十一行重新比对/绑定，其余模块直接保留。 |
| private Verify | function_proofs_checked/reused **1/422** | **保留，不改成 5/420。** flow proof 覆盖全部 423 个非 foreign 函数，包含 unsafe io；foreign 在 Verify 的 flow 前返回。完整 raw 语义输入和 plan 相等即可复用，位置被明确排除，不依赖 Types scalar cache 资格。 |
| private fresh 对照 | fresh Success；diagnostics/source_files 相同；完整 TypedProgram Eq | 保留；不得只以计数替代这些断言。 |
| positions producer | environment_imports_changed=1；function_input_hashes/reuses **0/425**；text_comparisons=425；binding_hashes=425 | 保留。所有文件加同一前缀，函数声明字节未改，但所有绝对 span 更新；imports 的当前 span 也更新。environment 的 1 是请求级变化计数，不是 import 数。 |
| positions producer 下游 | lower_modules/reused_lower_modules **0/22**；retained_normalized_items=425；retained_restored_modules=22；rebound_frontend_modules=22；rebound_source_map_validations=22 | 保留。完整 placement 重绑覆盖 22 模块，同时保留真实 lower 内容并校验当前 source map。 |
| positions materializer | executed_modules/reused_modules **22/0** | 保留。materializer receipt 要求相同 parsed revision；位置前缀使二十二个 parsed revision 均改变。此资格不同于可重绑的 lower。 |
| positions Types | body keys **0/423**；Types **4/421** | key 断言保留；Types 旧 **2/423** 改正。两个 io 的 completed-None key 仍可复用，四个不可缓存 CFG/声明仍需执行。 |
| positions publication | serialized+reused=425；**serialized/reused = 4/421** | **修正 `3475–3478` 的说明及 ≤2/≥423 断言**。这四行包括两个 unsafe io，不只是两个 foreign。若保留原上/下界写法，应至少改为 ≤4/≥421；当前静态路径支持精确 4/421。 |
| positions fresh 对照 | fresh Success；diagnostics/source_files 相同；完整 TypedProgram Eq | 保留，完整当前位置仍应一致。 |
| 两次 error | SafeReject；phase=Types；Verify runs=0；body keys **1/422**；index 9 lower/reused **1/21** | 保留。全部 key 查询先完成，随后真实 Types 对 absent_name 拒绝；失败没有 Verify/publication。第二次未额外断言 producer 工作量，不应凭首个 error 推导同量执行。 |
| recovery | body keys **1/422**；Types **5/420**；publication **1/424** | Types 旧 **3/422** 改正；其余保留。失败后恢复最后成功 cold epoch，只有 h0 的相对函数内容改变；四个 Types 不可缓存行在未改动 io 文件中，完整 raw 值相等，publication 仍复用其旧编码。 |
| 最终错误对照 | errors.len=2；两次 diagnostics Eq；fresh error diagnostics Eq、Types、Verify runs=0 | 保留。现有失败事务恢复原成功 FunctionSession；诊断使用当前原始 spans。 |

## positions publication 为何恰好 4/421

本测试 options 只有 `producer_reuse: true`，**push_reuse 为默认 false**（`project.rs:3269–3273`）。publication 的通用 `relative_function` fallback 由 `options.push_reuse` 控制（`semantic_items.rs:1172–1173, 247–257`），因此不能给四个无 sealed producer handle 的行补通用相对编码。

冷 publication 中，这四行的 canonical binding 为 false；另外 421 行通过真实 FunctionProduct canonical handle 使用完整相对函数。位置修改时 `MemoryPublication::prepare` 对 canonical 行转换相对 spans 后全值 Eq；非 canonical 行比较完整当前 raw Function，含绝对 spans（`semantic_items.rs:703–747`）。于是 421 行相等复用，四个 io/foreign 行全部因绝对位置变化重新编码，真实 node 费用不增长；这不是按 Types 执行量猜 publication 数。plan 行不含此次变化的诊断 spans，不产生必要的 plan 内容变化。

private/recovery 则不同：io 文件根本未改，四行 raw 值仍完整相等；只有 h0 相对内容不同，m0 其余二十行完整相对值相等，因此编码仍为 1/424。该路径无需把任何 unsafe/foreign 函数冒认为 Types scalar hit。

Verify 依据：`zil-verify/src/lib.rs:1937–1954` 排除两个 foreign 的 flow；`incremental.rs:454–495, 557–600, 669–742` 绑定完整语义函数、完整 plan、target/limits 和原费用，明确排除诊断位置。它不会因为 io 没有 Types CFG cache 就失去原有 flow receipt。

静态复核不能替代 root 的受影响 gate；本报告没有新增性能数字或宣称本候选更快。
