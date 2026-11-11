# 响应式编译查询图专项：本轮真实阶段交付与恢复

父任务goal G10.5.1.15；分支`research/reactive-query-graph`。本轮实际main基线`99035931d358add0eaed6d4535035c38413701dd`，从`c75c58bf7f564a1ce8049702f9c6ec41c82d1da2`正常续接，无需rebase/main变更。本轮生产实现、真实测试与测量已经执行；完整跨阶段架构未完成，当前切片没有证明大型净等待收益，默认关闭、不合main。不要把此状态写成整条zit路线失败，也不要把BodyKey局部减少写成大项目提速。

## 远端冻结身份

- 首次生产切片：71bed1fc8d5f184549b2f08ab7af724a9952fc9a。
- 最后生产改动/主测binary来源：21746adedf6eac6aee6f3c31e7b7ec4907027428；binary SHA256 `0a0aa4049c9cae9f9c872186e8b00d76066903e0c76728a106627da410c0a1bc`。
- 新rich差分测试：8762d786faf0072659bd20dbcdf3f64db6640a11；第一次具名断言失败保留。a05526bfbeb31e90962f53692468ad05f48c1620修正为当前真实E-UNINITIALIZED及第二f()精确位置。
- 全234lib+CLI1实际通过源码：a655cd12d685818732ce7d9ab96e2676735f26f3。
- 测量器修正：a655(U→AB，来自既有协议)；be6dcf9fd82566b5874298327e8c19b30ee87ec3(CT compact只含实际3模块，排除未导入wide替代fixture)。两次均先远端保全再新run重试，没有改受测production binary。
- 最终交付含本文/README/receipt的远端HEAD，以goal本任务记录和`git ls-remote`为准；不能用工作树路径中的71bed误认binary来源。

## 已有生产接入与真实限制

zil-db的图由zil-types FunctionSession实际调用，经driver/CLI `--reactive-queries`进入真实Source编译。owner为项目根/模块来源路径/函数名/分面；当前完整Core Eq、target/全部Limits、声明/namespace/import/compactID守卫及签名实际读取独立提供有效性，未删旧QueryKey.input。输入一次批量装入，直接通知、按拓扑高度去重求BodyKey，复用现有raw CFG/cost/span-rebase内容产物。图失败抛弃候选epoch；旧内容缓存保留，不携带Verified权限。

当前主要是input→BodyKey这一层。仍全CoreFunction Eq和构建全体header/context/输入inventory；Expand整项目Normalize/Lower仍执行。CFG已有缓存使private实际重算5个，两臂相同。负向名称/可见性/候选变化由完整环境守卫保守覆盖，尚未逐名称槽稀疏建模。合法普通递归依赖签名；真实通用分析SCC尚未接入。rich mixed、trait/layout/资源等走现有完整回退；简单泛型样例展开成scalar Core后有4个真实图查询，不代表一般实例化稳定身份已经完成。

driver逐attempt保留失败输入、来源、诊断、成本和时间，累加重试真实工作；phase clocks补query digest/publication，whole-query复用时工作统计null，不能借旧统计/标签宣称执行下降。Discover、磁盘cache编码/写入及部分清理仍不全在phase clocks内，因此以完整request wall为主。Analyze原初始化内容缓存、当前独立Verify、IR/schema、后端均未改。

## 写集和并行边界

实际生产改动在zil-db、zil-types/incremental、driver的lib/project/session、CLI main；真实测试在driver/function_incremental_tests和reactive_query_tests、db/reactive/tests、Types测试。root单写Types→zil-db依赖：crates/zil-types/Cargo.toml、Cargo.lock对应列表、policy/dependencies.json一条边。原认领的semantic_cache.rs、Analyze incremental和resident.rs本轮最终没有源码差异。

保护根Cargo.toml、公共IR/schema、独立Verify、LLVM后端及其unsafe策略。没有操作其他专项分支；最近实际读到ThinLTO分支8072f23187a77f2e63686696111a8e0e71e8be7b，后续必须再读实时refs。本轮结尾更新goal前读到a4e78dd67301110f072189dc76cb49bb962e9081；只改本任务行和自己的record，拒绝重读重放，不force。

## 已运行验证与等待结果

- 环境真实可用：Rust1.99.1、LLVM/Clang/LLD23.1.4，仓库checker Pass；main与217 release构建/原生程序成功。编译Rust编译器时间单列，不作为ZIL编译成绩。
- 最终db25+Types63+driver146=234lib tests，CLI resident1，fmt及architecture15crates39edges通过。七个driver差分测试逐当前Source与fresh legacy比完整TypedProgram/接受拒绝/首诊断/来源，覆盖预算N/N−1、取消重试、错误修复、80次编辑、驱逐/损坏/恢复、跨模块变化、位置/ID、动态依赖、递归、trait/泛型/CT/拥有闭包。
- 主scalar生成22文件498620字节，20×20×16；O2/check各6blocks204行(108measured+96parent preparation)。O2 1080native，check0native。所有成功当前Verify1，两臂ELF/LLVM相同。O2 private中位5160.713/5169.342ms，配对均值候选多71.526ms；multi中位5214.765/5095.267ms，配对均值省43.137ms但区间跨正负；恢复中位5248.582/5480.149ms，均值多299.541ms。check private中位547.462/732.712ms，均值多159.811ms。全场景/范围/CPU/RSS见README及receipt，不挑最快block。
- private BodyKey421→21、编码节点159558→7965、当前graph读83边，但全CoreEq仍421、CFG执行仍5。减少的是该计数器范围，不是全部AST或类型检查。
- 大mixed19文件246565字节、14×14×16，100行552native通过，graph0仅组合回退控制；小scalar204行1080native；5既有消费者×O0/O2为220行1368native，CT仅compact无allocator故障注入。另有ZUI StoreFields烟测，不称完整ZUI应用覆盖。
- 50unique私有编辑的cache-none/capacity32 check：102requests，5fresh公开JSON对拍，graph1268nodes/1685edges，无reset，edit wall27.819391→28.043637s，末RSS441.961→446.426MiB。两臂都先增长约213MiB，后半段RSS增20/328KiB；不是一般无泄漏证明，native0。
- 8CGU/1job单block诊断两臂各private7对象hit/1miss，1.838/1.645s；这是既有分区能力，不能记图收益。main vs instrumented-default及关磁盘cache各3block check仍有波动/回退，默认性能等价和缓存写入归因未接受。
- 假设频率每100请求O2总等待：人工单点多8.469s、LLM多文件多2.226s、冷/广泛多10.204s；check分别多13.016/10.514/15.682s。是假设，不是用户实测分布，不等权混合大小项目。

## 首失败与独审

保留宽Cargo集成选择耗尽自有磁盘(InfraError，98个自有可重建ELF回收清单保留)、876错误诊断预期、resource oracle BA、CT多拷wide fixture、早期无实际多编辑/restore的工具控制，以及超过默认CT/lex预算的大样本。修后新SHA/运行重试；失败、Unsupported、零执行和缺summary不进入成功分母。

非作者workload_harness按217闭合两项生产计量缺陷，按a055复核具名诊断修正，并核102条长会话raw及来源链；measurement_review独立核主测408行和补充524行、输入、native oracle、ELF/LLVM、实际计数及加权算术。统计器由后者编写、root复核公式，未假称作者独审。完整review/failures文件都在证据包。

## 已持久化证据

[zil-reactive-compiler-evidence-20261111.zip](https://chatgpt.com/library?file_id=libfile_fde3_f-2b7913d388a0819192209665af814d06_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)

Library文件ID `libfile_fde3_f-2b7913d388a0819192209665af814d06_bd147c3485448191a9df16b3c3cea7dd`，版本1；50430867字节，6490个归档条目；ZIP SHA256 `2bb15c5a045abcf493f8e789512653ac416bebfe86229fb8b56c9bd53c51dc42`，MANIFEST SHA256 `2567a515b7ef8207a7d8fb9bb4160880eece34681d2c7506397356cc57e1530d`。所有条目读回核SHA，含39组运行目录、每次完整raw/Source/命令/oracle/CPU/RSS/失败/独审。入口`reactive/out/runs/reactive-final-index-20261111/README.md`，不依赖旧临时沙盒恢复结果。原始私人zit源码未复制；已实读参考与旧ZIP的hash在receipt。

## 下一真实切片：尚未实施

下一片应新认领zil-syntax原始Source解析封装、zil-expand/modules.rs/lib.rs与Budget receipt、Types preparation/incremental、driver项目组装；若需要新的内部依赖，先root协调Cargo/policy。不要先改单个Eq为source_fingerprint/Parse Arc提示，也不要只把当前CFG Entry改挂图。

1. 真正parser构造私有不可变ParsedSource，持有真实源码字节/身份/上下文和实际ParseResult；不给任意Source+ParseResult拼证明的公共构造器。现有公开raw Parse/Core入口继续完整校验。
2. Expand生产者封装成功的普通scalar模块Normalize/Restore/Core/SourceMap与函数版本。未变密封Source和完整环境才可复用；改变模块全部fresh，第一片仍允许21个BodyKey更新，不追逐函数内位置增量。
3. 环境保留完整声明/namespace/import/compactID/target/rules/Limits，排除私有正文；名称不存在、候选/可见性变化仍保守破坏环境。实际CT、syntax生成、泛型/trait/nominal/resource/closure等首片整体回退，完整分类不能凭token提示；允许当前scalar extern IO及其已有安全形状以覆盖实际输入。
4. 成功receipt按输入预检→Normalize→plain specialization检查→Restore→Lower/SourceMap各原阶段顺序保存。当前remaining足够才整段重放扣费；不足从原remaining执行原逻辑以保持首预算诊断，不先扣半段再回退。取消/失败不发布部分产物。
5. 只有inference借用、CT unchanged和shape identity整条链成立才把生产者函数版本传入PreparedProgram，再接现有owner graph和CFG内容缓存。图值需承载真实模块产物；未变模块不需重新提交所有函数输入。CoreProgram仍是Vec值表示，组装可能深clone，必须另计实际Clone/Rebase节点，不能报零AST访问。

最短门：fresh-current完整差分；单模块/同批多模块、签名/namespace/import/ID/span变化；raw伪造来源不能走密封路径；逐阶段预算N/N−1/首诊断；失败取消、驱逐、旧源码恢复；所有富语义回退。先证明真实Normalize/Lower模块执行数及节点遍历减少，再重跑同冻结负载配对，不继续同一旧实现挑快样本。该片即使改善Expand，也不能直接解释1CGU的4–5秒机器码关键路径。

## 直接恢复命令及阻塞

当前没有环境/网络/工具链阻塞。阻止默认启用/main的具体条件：大项目净收益未成立；密封Source→Core来源链和分段预算receipt尚无；完整多分面/实例化/SCC及异步发布未实现；默认路径与main性能等价、全部产品/运行质量集成门未满足。

先读最新真实refs，不能假设本记录一直是最新：

```sh
git -C /workspace/scratch/e28839ace920/reactive fetch origin refs/heads/research/reactive-query-graph:refs/remotes/origin/research/reactive-query-graph refs/heads/goal:refs/remotes/origin/goal
git -C /workspace/scratch/e28839ace920/reactive status --short
git -C /workspace/scratch/e28839ace920/reactive show origin/goal:records/compiler-reactive-query-graph.json
```

下一个源码入口（先在goal认领上述新写集再编辑）：

```sh
cd /workspace/scratch/e28839ace920/reactive
rg -n 'pub struct ParseResult|fn normalize|expand_inner_project|identity_inference|into_prepared' crates/zil-syntax/src crates/zil-expand/src crates/zil-types/src
```

已有验证环境：`source /workspace/scratch/abf690ce880c/toolchain/env.sh`，再显式`export CARGO_TARGET_DIR=/workspace/scratch/e28839ace920/targets/reactive-validation-71bed1f`。此validation树目前be6、release仍217；未来改代码须另冻结提交和独立target，不能边测边改/替换binary。受支持工具链恢复以当前rust-toolchain/docs/guide/build.md/CI及checker为准，不照搬过期环境结论。原始结果已保存；结束前根核所有写集提交/远端SHA。回复后没有后台施工承诺。
