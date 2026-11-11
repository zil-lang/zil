# 私有 ThinLTO 的净编译成本路径评估

结论：**先测“请求内只做一次完整工具身份核验”的独立driver，再决定是否加CGU级prelink缓存；暂不做函数级prelink缓存。** 私有导入已经过两个runtime case，并不表示编译更快。首个成本对照保持当前所有LLVM验证、原budget/owner证明、实际.3覆盖及hook检查，首完整validate仍在B完整请求计时内，不从已有时间中算术扣除诊断开销。

本轮只读现有源码、官方LLVM23.1.4 API及已完成raw，另按root授权只写新research/thinlto-private-identity/common.py和wrapper一处调用；未构建、运行LLVM/native、采样或改旧freeze、Rust/Cargo/schema。root正在编写新36次编译screen；以下缓存方案尚未实现。

## 1. 现有数据支持什么

这是上轮runtime质量probe每arm的一次clean编译收据，**不是重复编译benchmark均值**：

| case | A完整请求wall | B完整请求wall | B完整CPU | B的65个互不嵌套工具leaf CPU总和 |
|---|---:|---:|---:|---:|
| N64/base | 1255.692ms | 8285.686ms | 14240.383ms | 1596.819ms |
| N256/base | 4955.840ms | 14630.546ms | 22469.956ms | 5477.809ms |

来源：out/next-optimization/private-runtime-v1/n64/base与n256/base的compile-A/B/raw-metrics.json及B/wrapper下process-attempt.json。CPU差额还包含Python、Rust lower/双计划、JSON/IR解析、hash及文件复制，不能全部归因于hash，更不能把CPU差额当wall收益。

每B请求有8个compile wrapper+1个link wrapper；每个validate读取约748.540MB的p.tools，加plugin compiler再次读取clang约282.507MB，固定image full SHA读取下限约1.031GB/次、9.279GB/请求。它是源码可数的字节量，不是可以直接减去的秒数。

65个外部leaf分解为：8×(input verify、clang prelink、prelink verify、bcanalyzer dump、llvm-dis)=40；link1；post-O2 verify8；.3 verify8与dis8=16。当前插件内部before/after验证及lld标准验证另在link进程中执行，不与这些leaf相加计算wall。

现有研究已用sha2、BufferedHasher、直接renderer及必要条件短路，不建议再把这些已有机制包装成新优化。

## 2. 已author的第一项对照：只去重请求内固定image核验

新ROOT为research/thinlto-private-identity。common新增validate_request，wrapper仅改validate(p)→validate_request(p)；新format为zil-thinlto-private-identity-runtime/1，避免旧协议混用。ROOT其余协议/screen/README由root另行冻结。

每请求用现有fresh logdir、实际protocol SHA、当前Rust receipt SHA及绝对路径绑定；Linux flock让首wrapper在B计时内部执行原完整validate，原内部重复clang读取也保留。只有validate全部成功、各固定image读前后精确stat一致、发布前再次一致才生成request-tool-identities.json。

登记集合严格为p.tools及plugin compiler固定image的精确路径，不按目录缓存；小protocol、scripts、buildlogs、CPP/source、当前proof、IR/BC、manifest和输出仍真实SHA。后续wrapper继续运行原validate，只有这些已登记image的sha读取可由dev/ino/mode/size/mtime_ns/ctime_ns、symlink自身及resolved path全相等的stamp返回原已核SHA。每工具启动前后再stat；变化或错request直接失败，不假设SHA、不静默降级。每个新请求重新full validate，不跨请求保存身份许可。

这采用与现主线installed_identity相同的“受信已安装工具在请求内不被修改”边界，未把stat当不受信文件的密码学证明；主线实现本来也是metadata+GNU build-id，缺build-id才full SHA。[object_cache.rs](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/object_cache.rs#L413-L479)

它不缓存VerifiedModule、model、preflight或任何编译结果；全部32个外部LLVM verify、完整Rust proof、实际.3和hook仍保留，native cache仍每请求fresh/empty。这只是研究driver去除9→1重复身份核验，不能直接宣称产品优化。root的新真实compile screen才能判断是否净胜。

## 3. 8CGU会不会让16处编辑全部失效

**不是必然；也没有“16处一定只动一个unit”的保证。** Disabled canonical planner按source file/chunk并组，local identity组件再约束归属；当前pure无global/closure，leaf每file9个函数，远低于256函数chunk边界，普通归属来自FileId mod8。代码仍逐unit构造真实model，最终必须按实际model判定，不以编辑文件数代替依赖。[partition_view_once](https://github.com/zil-lang/zil/blob/2549cbec8e1f3096d4c8c8a960459c83095ad13d/crates/zil-backend-llvm/src/machine_cgu.rs#L325-L450)

当前fixture selected_files公式为sorted((1+j*N//k)%N)。在已经观察到的FileId/fixture归属下：

| 编辑选择 | canonical prelink潜在dirty owner | 理由/限制 |
|---|---|---|
| N64 k16：1、5、9…61 | 1和5 | 文件编号步长4与mod8碰撞；不是均匀16个owner |
| N256 k16：1、17…241 | 1 | 步长16与mod8碰撞 |
| 16个文件覆盖全部mod8，如连续1…16 | 可为全部8个 | CGU粒度本来就不能复用这些变更 |
| 改ABI、local/state连接、函数增删/编号、常量摘要传播 | 可能扩大乃至全部8个 | 当前fold和canonical符号/分组结果可变，必须重新hash真实unit |

上表不是新缓存实测命中率；它说明原“很多文件→必定8unit全失效”不成立，也提醒现有均匀选文件公式偏向少数owner。不能为展示cache命中而只报告有利选择。

更直接的全失效陷阱是**诊断元数据**：每unit.to_text含全项目verified-fingerprint注释，任一源码变化都会换它；clang还从fresh staging路径生成source_filename。直接SHA整个input.ll或把当前临时路径放入新key，会让未变model也miss。现有object machine_key明确排除诊断fingerprint，说明它不是Builder语义输入。[machine_key](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/object_cache.rs#L301-L323)

最小prelink缓存需有新协议：只排除那一条已知唯一诊断注释，不任意规范化IR；其余名字、类型、属性、顺序、常量、声明、target均完整绑定。也可让Rust研究receipt输出同一真实model的完整hash（仍排诊断fingerprint），避免用文本当模型索引。

## 4. 下一步可行的CGU prelink缓存

### 4.1 缓存对象与key

缓存实际clang产出的带summary/hash prelink BC及其compact facts：canonical/prelink definitions、直接调用图、已验证intrinsic/foreign集合、原input hash、BC hash、producer前后LLVM验证、精确命令与工具身份。facts只描述这个BC，不保存“今后模型已验证”的许可。

初版可采用保守key：

K_unit = H(新格式版本、完整unit语义输入、owner/canonical身份、当前原Rust proof的稳定语义payload、clang/LLVM/lld/plugin完整身份、triple/cpu/features/PIC/model、prelink/postlink O2与merge/force/private policy)

原proof payload完整保留canonical/Allowed/Required-original/逐函数cost/各unit与总copycost/sourcecost/foreign及policy版本；只去掉elapsed_ns、fresh路径/nonce等非语义字段。**不能把原receipt原字节SHA直接当持久key**，其每请求计时不同会强制全miss。当前literal edits通常不改这些集合或cost，所以这个保守全proof payload仍可保留未改unit命中；一旦任一成本/owner/边界变化，初版允许所有key保守miss，不为局部命中削弱门。

actual prelink witness是BC输出的事实，不能在生成BC前要求它作为key输入形成循环。当前请求完成全部unit lookup/producer后，再由这些actual BC+facts重算全局Required，重新核原Allowed/cost，并生成fresh request manifest/action digest，绑定本次完整Rust receipt SHA、actual prelink witness SHA和plugin。这样persistent artifact identity与当前请求接受记录分开，均有显式关系；cached BC不是验证能力。

### 4.2 source_filename与原生cache

BC内source_filename必须与actual BC一致并被当前manifest唯一匹配，不能直接改字符串后继续沿用旧summary hash。一个小实现是miss时从compiler-owned稳定输入路径编译，如namespace/owner/inputKey/unit-N.ll；hit直接复用已验证BC的真实source_filename。该路径是artifact provenance，不替当前source fingerprint，fresh诊断sidecar仍按当前请求输出。

初版**只缓存prelink，native ThinLTO cache仍fresh/empty**。每次仍真实thinlink、import、Internal pass及完整O2，8份fresh hook收据必需。LLVM原生cache key不含plugin字节/环境manifest；即便目录namespace加入plugin，hit也会绕过当前hook，不满足现gate，不能盲开。

future若要native hit，必须另设计postlink action key显式绑定plugin、完整import source module hashes/resolution、当前proof policy和实际接受记录，并把“复用旧机器码”与“本请求fresh hook”合同分清；本轮没有这个授权或实现。

### 4.3 每次请求保留什么

当前Verify、完整原machine与partition input、实际unit preflight、原Complete/Disabled owner与copy预算证明仍全部跑。hit核当前unit key、BC/facts校验和、producer身份；当前prelink BC仍需由LLVM验证，真实postlink before/after验证和fresh hook仍执行。

miss需完整producer验证和summary生成；hit只省clang prelink及其重复dump/dis/facts生成，不省当前模型或语言检查。原node/edge/ABI或外部身份不支持即fail，不能从旧receipt推断“这次也安全”。

它只减少CGU级后端前半段；Rust lower、原完整planner clone及两套owner证明、所有当前unit输入hash、thinlink与8个cold postlink backend仍在。当前N256原prelink工具CPU约1.084s，而link CPU约3.638s，不能以省prelink宣称请求趋近零。owner覆盖全部8时，缓存还增加lookup/hash开销，应完整保留不利样本。

## 5. 研究诊断可以怎样减少

本轮identity对照不动以下项；若之后单独授权新协议，可分步：

| 当前开销 | 可实施替代 | 不能省掉的约束 |
|---|---|---|
| 每次8份bcanalyzer全文dump | miss producer确认summary一次，缓存BC hash绑定的summary/facts；hit使用同一BC并由实际thinlink消费summary | 普通BC不能冒充ThinLTO；损坏或不匹配必须miss/fail |
| prelink llvm-dis及Python全图重复parse | compact facts与BC hash同存；miss生成，hit重用精确artifact facts，再组装当前Required | 不能沿用旧全局Required或旧budget许可 |
| .3 dis、全文落盘再parse | host C++在实际post-import模块中实现同样完整定义/引用/Required检查并出fresh紧凑收据 | 当前插件inspect尚未完整替代Python全图引用检查，不能直接删Python门 |
| 每hook重复print/hash/写before+after全文 | 新协议可在host内保留完整比较/验证，成功仅存绑定hash与紧凑receipt，失败再落已捕获全文；零AE可证明无变换后去重相同快照 | 不改body/attrs约束、fresh phase和前后verify必须保持，不能把“少写日志”当语义优化 |
| .3/.4额外opt验证 | 固定LLVM默认LTO本身有pipeline前后Verifier，插件也有变换前后Verifier；可在新协议明确覆盖等价checkpoint后再评估外部重复验证 | 首轮不删；必须证实DisableVerify=false且实际after-O2验证仍执行，不拿插件的before-O2 verify冒充after-O2 |

官方[LTOBackend.cpp](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTOBackend.cpp#L340-L376)在标准pipeline两端添加Verifier；[PostOptModuleHook](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTOBackend.cpp#L427-L434)在运行后保存优化产物。减少外部重复检查需要新明确证据合同，当前source/协议不变。

## 6. 为什么函数级prelink不是小补丁

prelink O2会本地inline、常量传播、属性推断、function merging等；一个函数优化后的body依赖同module的callee bodies、声明/ABI、global/type及优化上下文。实际leaf.value已内联8个helper，所以“只hash该函数原machine body”会漏依赖。root主value又引用多file声明；不能把源文件改动与唯一函数key等同。

要缓存真正的函数prelink产物，至少需二选一：

- 保留相同CGU优化语义：记录足够完整的module/调用者/递归SCC依赖并可重建IPO结果，依赖可能退化成整个CGU，收益不保证。
- 把每函数或闭合组件作为独立LLVM module预优化再重组：需要完整declaration/type/global/linkage/COMDAT/attributes处理、唯一canonical owner、symbol映射及summary重建，也会改变原optimizer neighborhood，必须重新过运行质量；这不再是当前已Pass policy的单纯cache。

现有C API提供LLVMCloneModule、LLVMParseBitcodeInContext2和LLVMLinkModules2等whole-module操作，不能直接提供“独立缓存任意Function再无损接回原prelink pipeline”。更关键的是LLVMWriteBitcodeToMemoryBuffer只写普通BC，不生成ThinLTO summary；先前已核的[BitWriter.cpp](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Bitcode/Writer/BitWriter.cpp#L20-L49)不带summary index。

若真要跨这一层，最小FFI/模型工作也包括：稳定、完整的per-function/组件输入与依赖key；同LLVM实例下的body materialization/clone/link与Value/type映射；当前完整unit preflight；C++ PassBuilder ThinLTOPreLink pipeline和ThinLTOBitcodeWriterPass的summary/hash输出；新的artifact协议与当前budget/owner绑定。不是增加一个LLVMCloneModule调用就完成。相关主源：[Core.h](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm-c/Core.h)、[BitReader.h](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm-c/BitReader.h)、[ThinLTOBitcodeWriter.h](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/Transforms/IPO/ThinLTOBitcodeWriter.h)。

对目前已过runtime的纯多文件场景，CGU prelink artifact cache不需要新FFI或共享schema，成本与失效范围都容易观测；函数级方向先不做。先保留本轮单变量身份去重的新真实compile结果，再以它决定是否进入缓存，而不是把runtimePass换算成compilePass。
