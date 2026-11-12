# Frozen900 r7：独立剩余成本诊断

范围：只读 `/tmp/full-pipeline-profile-900.json/.md`、`out/runs/full-pipeline-profile-o2-r7/rows.jsonl` 的少量字段，以及 `/workspace/zil-demand-frozen-900293cfd` 中相关生产源码；该 worktree HEAD 已读为 `900293cfd8afcd7118775e8165e1ec4f5266e3d7`。未构建、测量、压缩、运行 validator 或重新哈希原始文件；不改变 root/source/helpers。源/tree、34rows/180native/15成功对 LLVM+ELF 同字节等有效性是既有收据，本报告没有冒充重新验证这些结论。这里只诊断单块 r7，不抢先使用正在运行的六轮矩阵。

## 直接可见的负结果与边界

r7 cold：main6907.034ms、candidate7866.906ms，差+959.872ms，1.139倍；broad：6163.278→8248.034ms，差+2084.756ms，1.338倍。这两个负结果必须保留。当前数据不足以把差值归因给某一种缓存、分配或遍历，也不能从这一块估计稳定回归幅度。

基线 `phase_timings_ns` 缺失，因此下表只是候选内部的绝对成本。不能把某一候选子阶段当作 main→candidate 回归的净增量。候选计时器之间的包含关系如下：codegen 包含 native timings；object 包含每 CGU API 的 preflight/builder/optimization/emit，publication 的 encode/io 位于各自 driver publication 之内；这些父子项不能相加。

## 候选实际计时字段

单位ms，保留三位。第一张表来自候选 `phase_timings_ns`、`driver_timings_ns`，不是互相包含的后端细分。

| 字段 | cold | broad | private | positions |
|---|---:|---:|---:|---:|
| phase.codegen | 5759.629 | 6311.337 | 576.098 | 83.016 |
| phase.expand | 711.882 | 541.131 | 43.132 | 233.135 |
| phase.types | 330.264 | 262.114 | 28.379 | 35.725 |
| phase.analyze | 85.914 | 79.622 | 25.707 | 49.197 |
| phase.verify | 68.627 | 55.180 | 45.568 | 38.162 |
| phase.ct | 52.508 | 33.492 | 32.956 | 28.169 |
| phase.lower（非 machine lower） | 2.041 | 2.651 | 2.689 | 3.707 |
| driver.source_ingress | 420.195 | 175.558 | 5.662 | 48.441 |
| driver.function_cache_publication | 102.880 | 461.550 | 3.638 | 2.443 |
| driver.semantic_candidate_publication | 302.536 | 74.495 | 7.207 | 5.218 |

`exclusive_costs.unassigned_ns` cold21.345ms、broad231.440ms；两者 overlap_ns=0。不得把 broad 的231ms 未归属成本悄悄加给 clone、Verify 或 LLVM。total_ns 与外部 wall 也有边界差别。

下面来自各测量行的 `native_codegen.timings`，是 codegen 内部细分：

| native 字段 | cold | broad | private | unchanged | positions |
|---|---:|---:|---:|---:|---:|
| machine_ns | 1157.926 | 1136.855 | 17.343 | 6.968 | 22.889 |
| partition_ns | 71.098 | 218.455 | 0.427 | 0.102 | 0.020 |
| diagnostic_ns | 288.573 | 872.810 | 15.793 | 18.371 | 18.270 |
| object_ns | 3703.986 | 3887.571 | 423.304 | 5.415 | 3.379 |
| link_key_ns | 489.750 | 133.000 | 93.638 | 42.182 | 35.076 |
| link_execution_ns | 47.490 | 54.396 | 23.558 | 1.312 | 1.494 |
| other_ns | 0.796 | 4.782 | 0.434 | 0.394 | 0.360 |
| total_ns | 5759.618 | 6307.870 | 574.498 | 74.743 | 81.489 |

`machine_ns` 不等于单独“机器代码函数 lowering”：build.rs 先计 lower_view_with_session、task ABI/transform 处理，后又把当前 LIR 的 scoped-thread 扫描加到该字段。`diagnostic_ns` 合并 session.render/完整 formatter 与 staging LLVM 文件 write，不能把872.810ms全称作文本生成或全称作I/O。partition 明确减掉包含的 render 计时。codegen/native total 的小差值保留在其真实边界，不强行对齐。

各16个 object_reports 的 API 子阶段求和：

| 场景/编译器 | preflight | builder | optimize | emit | API total |
|---|---:|---:|---:|---:|---:|
| cold main | 139.039 | 219.400 | 1268.285 | 2177.942 | 3841.709 |
| cold candidate | 163.935 | 211.736 | 1191.351 | 2026.962 | 3627.525 |
| broad main | 134.862 | 208.379 | 1232.316 | 2270.973 | 3884.020 |
| broad candidate | 142.463 | 189.836 | 1175.665 | 2025.031 | 3565.341 |

这组单次观测没有支持“候选 LLVM optimizer/emitter 更慢导致全部负结果”的证据；也不构成 LLVM 改善的统计结论。候选编译器侧 machine/diagnostic/partition/publication 是实测的大项，但尚无基线对应细分，因果归因仍待匹配矩阵及后续更细计时。

## 必要工作和仍可消除的重复

1. **改变的原生单元仍确实编译。** cold/broad 都是16miss/0hit，当前 broad400函数 lower、25复用、16单元重建；在固定 CGU16/O2/jobs1 下，重新优化受影响完整 CGU 是现有正确性边界。private1lower/424reuse，15对象hit/1miss，object423.304ms；multi3函数lower、recipe3单元重建，实际对象14hit/2miss，不能把recipe重建数当LLVM执行数。无实测依据把整单元 optimizer 工作说成伪增量，或通过改CGU数解决本轮同参比较。

2. **后端仍构造/检查当前整体输出，但共享已到真实消费者。** 调用链为 build.rs→emit::lower_view_with_session→current machine/task ABI→partition recipe/original planner→BackendSession::render→DiagnosticText::write→对象→link key。cold machine1157.926ms，broad1136.855ms是真实优先诊断区。ordinary O2 不运行显式 incremental_o2 的 fold/stable transform；旧F2 COW问题不能用于解释本轮。unchanged whole-machine=true，同时425函数输入比较仍执行，machine6.968ms；其代价已经小于 linkkey42.182ms。projection traversals/visits=0证明不能再次把“去掉无人消费的source projection”当作尚未实施的新收益。

3. **LLVM sidecar 仍完整发布；没有重新聚合14MB String的证据。** render 是逐项缓存 Arc<String>，DiagnosticText::write 使用 BufWriter 顺序写 shared fragments；items/Function.blocks 已 SharedVec。private只新render1项/复用428，broad新render400/复用29，cold429项。unchanged/positions没有新render，仍约18ms diagnostic，说明输出路径当前有可见成本。应首先分开 render 与 write，并量化 retired entries 的释放，才能解释 broad872.810ms为何明显高于 cold288.573ms；不能凭该单轮时间差推导额外3倍文本工作。输出语义仍要求当前完整 LLVM sidecar，不能为了计时静默省略。

4. **冷/广发布确实还有两条 raw body 编码通路。** FunctionStore::save_items(semantic_cache.rs)→FunctionSession::encode_item_products(incremental_items.rs)→Entry::canonical+Entry.encoding；SemanticPublicationSession::encode(semantic_items.rs)→private CanonicalHandle→canonical_function→另一套逐函数 slots→encode_module_candidate_plan_segments。cold FunctionStore421次序列化、semantic425；broad分别400和400，全部425 FunctionPlan 已复用。canonical_function 可替换诊断名，故两个完整函数字节不能未经证明直接混用。实际 publication：cold function encode52.010/io44.287ms，semantic encode154.745/io140.008ms；broad function encode319.418/io125.937ms，semantic encode36.139/io31.666ms。encode_ms不是单纯wire body遍历：它含相应编码调用、canonical化、指纹/manifest等；这些字段不证明全部是重复。两Store各一个dirty pack且无compaction；不能再把数百logical segment误报为数百sync写，也不能为节省I/O删sync/manifest-last/源alias保护。

5. **位置改变的残余主要位于真实 producer/materializer/currentbinding。** positions BodyKey0execute/421reuse、producer text hash0/reuse425；但lower22fresh、materialize22fresh/22cloned，Expand233.135ms中materialization104.359、production69.213、assembly42.723、context13.224ms。新相对input seal仅授权沿用函数输入版本，不授权旧绝对Core、SourceMap或lower输出。仍需425 currentbinding hashes与一次当前span重绑。安全后续若研究source-relative producer输出，必须保留当前文件/函数ID、完整rewrite/CT环境、quota/首错、当前maps重建与fresh oracle；不能把现有span忽略gate扩大成旧stage output授权。

6. **Analyze/Verify的全局工作仍存在，但已不是旧缓存深复制。** private Analyze5execute/420reuse/425equal；Verify1checked/422reuse、empty-borrow1/424、112197valuesavoided。raw budget::preflight、CandidatePlan::empty_for→TypedModule::fingerprint、全局effect/interface/task等检查以及结果/依赖inventories还运行。25.707/45.568ms没有内部细分，不能把所有时间叫admission/hash，也不能在未建立完整current flags/quota/first-error receipt时删除这些检查。完整候选数组与CFG共享、独立Verify借用/receipts已经实现，不应重做Arc共享或把Verify称为重新全函数分析。

7. **warm link key不是继续重哈希clang。** cold tools full14/reuse14、fallback282512476B；private/broad full0/reuse28、fallback0；unchanged/positions full0/reuse14、fallback0。现有link key每次发现clang -###、clang/ld运行时ldd、ld --verbose、当前所有候选目录/库/script/对象字节，成功freshlink后再复核一次，所以private/broad两轮发现，unchanged只一轮。F1/F3要求仍成立：工具memo不能吞掉content=true库全文绑定、不能按旧argv就省当前搜索/loader发现。当前42/35ms warm key是可见地板，安全下一步只能考虑把已完整验证的确定性解析结果与发现输入区分，并补齐任何新缓存的tool/config/runtime/search/missing-path依赖；不是直接缓存整条旧probe输出。

8. **multi的261.236ms Types不支持旧deepclone论。** actual CFG3execute，其types_other197.729ms、Expand_other96.557ms是现有残余桶，尚无内部样本；可能涉及分配、析构或调度，不在此作未经测量的归因。不得用它反向推断Initialization/Hold的共享无效。

## 下一安全切片的排序建议（本轮基线完成之后）

优先做精确计时而非立即换实现：分开machine lower/header/currentconsumer、diagnostic render/write/retirement，FunctionStore encode的canonical rebase/wire/hash/manifest；把broad231.440ms unassigned与multi other残余保持显式。现有单轮 broad encode319ms与cold52ms、diagnostic873ms与cold289ms的反常差距，要求先定位而非先承诺消除某个遍历。

若矩阵/计时确认发布路径稳定占优先级，最小可审的功能候选是：**由同一个sealed Entry共享source-relative raw body编码片段，分别组装FunctionStore与semantic的不同header/诊断名，保留两种旧wire流逐字节和两边原aggregate limits/节点费用。** 不能直接共享完整函数buffer或依赖公开digest；应按私有body所有者、全部Limits、实际name/header分片绑定，任何未证明域fresh。重点反例是qualified rename、位置/FileID变化、COW mutation、N/N−1限额、partial failure、取消和恢复。已有两套函数序列化计数证明存在重复通路；其净时间收益未被当前timer隔离，本报告不估计毫秒收益。

若用户更重视positions，接下来是严格scalar域的source-relative producer/materializer transport；若重视unchanged，优先诊断当前link发现和完整sidecar发布。上述都比再次优化已经共享的Hold/Initialization更符合本轮实际剩余计数。现阶段继续完成同参基线，不启动实现，不据此宣称整个pipeline最小或Rust等价。

## 当前audit.md复制段核对

root在7c79fa218更新的当前段落与冻结900实际源码一致：Initialization.blocks/predecessors/reachable为SharedVec；holds成功命中clone的是HoldRows=SharedVec<ValueHold>，FunctionPlan.blocks/holds也维持共享。嵌套BlockPlan事实、predecessor Vec和ValueHold.roots仍可在首次真实COW mutation时复制，但只读成功命中不深复制它们。当前段明确Verify原已借用Candidate Arc，未冒领新移除Verify clone；保留inventory/admission/fingerprint/global残余，表述合理，无需再替换。

唯一建议是将“Nested mutation uses COW”读作所有权保证而非零成本：丰富域fresh行保持unique，后续真实mutation可能分配；cachedscalar行不受这些overlay修改。现有candidate_queries::holds注释已明确此边界。r1段仍明确标注历史r1、不应用r7替换或跨候选推断提速。若补r7，宜另列private25.707ms、5/420/425与remaining-global范围，不给旧r1阶段捏造细分。
