# Blaze 当前源码复核：批量编辑、候选并发与可搬的小机制

## 结论与固定身份

本轮评估了一个小机制：**对完整、相同的原生CGU action key做并发miss single-flight**，不拆优化边界、不缩减导入body、不改变O2。只读原始key核验后结论为**暂不实施**：旧warm没有重复miss，新增cold仅有静态重叠上界；保持整请求成功发布合同还需解决跨请求持锁循环，调度扩展已超出小补丁的可信收益范围。Blaze可借鉴的是精确身份与在途复用原则，不能据此声称已找到ZIL提速补丁。

官方默认分支为`main`；`git ls-remote`读取HEAD和main均为`ae1a7a7a1771c1658173ee4829392c40b4f1d4cc`，tree `d469e309a0a645f83ff107f8b94bc5dcf02335c6`，提交时间2026-11-11T00:47:56-08:00。本地只读浅clone在`research/blaze-source`，未构建、执行程序或采样。上轮完整SHA已由官方commit API解析为`ccd4a6f31f0ad10a97b10941802bc9bd3f91d37a`，tree `cb939c2254b82ef0fb7702e8df37d96f12aeb793`。

两SHA间精确`git diff`确认：`crates/blaze-codegen`、`blaze-api`、`blaze-intent`、`blaze-query`、`hir-ty-codegen/v2`及`docs/v2/plan.md`均无改动。因此下列机制不是上轮之后新增加的突破，而是本轮重新沿生产调用链核实的现有实现。搜索索引返回的旧`docs/architecture.md`及ra-jit路径已不在当前HEAD；旧网页内容不能替代当前源。[当前仓库README](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/README.md)也明确列出的旧速度数字属于v1，v2尚未测量，不能搬作ZIL同质量优势证据。

## 1. 一次多点修改与多个独立候选是两种操作

| 场景 | 已实现入口及算法 | 它避免的重复工作与边界 |
| --- | --- | --- |
| 多文件/多位置一次提交 | `Request::Commit {base,head,ops:Vec<Op>}`；VFS的`commit_owned→move_head`先`tree.apply_all`成功，再移动head并记journal ticket；session收到durable ticket后才答Committed。 | 一组修改形成一个最终内容root。Commit自身不触发编译；随后对该root一次Build/Check。它不意味着每写一个文件都编译，也不把提交组拆成N个候选。 |
| 多个独立候选 | `BestOf {base,candidates:Vec<Vec<Op>>,...}`逐候选对同base `vfs.apply`，打开独立view，并分别`builder.start`，不是一个大编辑root。 | 候选可以同时在共享daemon内执行；按错误/警告/测试结果排名，最终全部完成再答Ranked。`test=false`是Check，`true`是Test，不能把BestOf直接说成N个完整release产物比较或运行速度排名。 |
| 语义多点编辑 | `blaze_intent::apply`按有序Edit作用于view，Rename/Replace有顺序限制；只splice触及语法；输出仍是VFS文本Op。 | `Applied::seeds`把已重parse的无错误树、精确text content ID和edition交给前端，避免再parse同一最终文本；仍以精确文本作为身份，不能把LLM声称“只改body”当验证权威。 |

来源：[Request类型](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-api/src/lib.rs)，[Session的Commit/BestOf执行](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-api/src/session.rs)，[VFS提交及what-if](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-vfs/src/lib.rs)，[BestOf状态机](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-api/src/session/best_of.rs)，[语义编辑和parse seed](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-intent/src/lib.rs)。

这些API证明它有批量编辑/多候选接口，不单独证明任意LLM并发请求有更低wall；共享工作的实际条件仍由下文完整身份决定。ZIL main990的`project.rs`/`session.rs::project_parse_input`已有当前语法借用、精确Parse身份与剩余预算重验，不能把“传parse seed”包装成本轮新增优化。

## 2. 函数single-flight确实阻止重复prepare/compile

[flight.rs](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-codegen/src/flight.rs)实现的单位是一函数，而非一批函数。其`Flights::ask`先查immutable landing，再以scheduler `run_if_idle(key)`抢运行权，在运行中再次查landing消除竞态。结果分`Ran/Theirs/Landed`：winner先把template放进memo，再发布shape landing；loser先做自己的其他函数，自己的runs结束后才wait。等待挂到scheduler flight，线程可以执行其他工作；不在持有别的函数运行权时等待，防止候选互等。失败或unwind没有landing，waiter可重问；`ASKS=256`防无限重试。landing和summary随memo template一起回收，不能指向已淘汰template。

它还有第二级shape flight：两个实例prepared IR在完整设置、架构及引用place结构上相等时，只emit一次，之后按每实例引用填充template。**shape去重发生在prepare之后，function flight则可让loser连prepare都不做**；两者不能混称。

不重复工作的直接源码证据是[driver/tests/flight.rs](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-codegen/src/driver/tests/flight.rs)：五个并行build各持自己的空store，却共享memo；总共五root加五shared函数恰好prepare/emit十次，且每build结果与单独build完整相等。另有release并行共享函数只编一次、后续release build不prepare/不summarize/不emit的断言。这是作者已写的模型backend测试，不是本轮执行结果，也不能替代真实LLVM并发测量。

作用域是**一个daemon共享memo**；不同进程、不同store或不同机器的独立miss不因这些表自动合并。磁盘CAS能共享已完成值，但不能据此宣称跨进程在途任务全去重。

## 3. 复用身份与失效：没有忽略被内联的body

[Instance与Settings](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-codegen/src/lib.rs)中基础instance key为domain hash(code hash、type-argument facts、settings)。[settings.rs](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-codegen/src/settings.rs)进一步包含producer工具身份、target/CPU/features、backend版本及优化等级、panic、溢出检查、debuginfo等；producer身份当前按相关package内容构造，源码明确标其粒度仍临时较粗，不能当成已实现任意函数级compiler自举身份。

基础instance key不是完整release复用条件。[query.rs的key表](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-codegen/src/query.rs)与[driver.rs::release_template_key](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-codegen/src/driver.rs)把当前body built-ins、引用种类及别名关系、每import的实际unit/settings/其built-ins合进key；[summary.rs::Imports::blob_key](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-codegen/src/summary.rs)继续哈希全部imports、全调用方一致常量、callee属性及dead参数。release flight直接使用该完整blob key。同名函数若导入body或IPO假设变化，就不能共用旧landing。

前端[blaze-query机制](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-query/src/lib.rs)以immutable view的内容输入验证constructive dependency trace：每条read记录fingerprint，按原读序验证，第一不同即停止；同key可保留多memo，只有全部依赖匹配才复用。重算结果fingerprint相同可early cutoff；淘汰value后仍保留fingerprint。这与“按文件改动次数推测失效”不同，也不是每次提交重置全表。它是一整套内容寻址、多view、回收协议，不能作为一个小补丁直接复制进ZIL。

## 4. 当前优化边界与运行质量：文档/实现必须分开

实际CLI策略见[plan.rs](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-codegen/src/plan.rs)：native O0/O1为copy-and-patch stencil，O2/s/z为LLVM O2，O3为LLVM O3；文件输出按所选等级编译一次，输出不依赖tier完成时机。native不走Cranelift机器码producer；源码仍保留Cranelift backend供其他路径/测试和wasm。live/tiered依赖可执行代码slot、counter、后台LLVM及patch权限，它的更快响应不能冒充同质量完整AOT请求收益。

当前[总设计第7节](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/docs/v2/plan.md)写“MIR→自有LIR，Cranelift IR不再输入”，但生产`blaze-cargo/src/serve/unit/codegen.rs`仍选`hir_ty_codegen::v2::Backends`；[Backends的prepare/emit](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/hir-ty-codegen/v2/backend.rs)将同一个`Function`装入`Prepared::Stencil/Llvm/Cranelift`，[Function/Prepared定义](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/hir-ty-codegen/v2.rs)与release emitter仍使用Cranelift IR builder结构。因此不能仅据plan宣称生产链已经完全切到独立LIR；这里按已读实现描述native producer选择与IR中间层，明确两者不是一回事。

[实际release emitter](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/hir-ty-codegen/v2/release.rs)已不只是旧版“每函数孤立LLVM、完全无跨函数inline”：它每函数跑一次ThinLTO pre-link，保留export；从全build summaries决定imports，然后复用该prelinked module、链接imports及应用IPO facts，跑所选等级的post-link pipeline。import export在同build通过Arc共享，避免每个importer重新做同一callee的pre-link，但每个caller消费body后的post-link优化仍有成本。优化实际使用的所有body记录到`inlined`，driver拒绝未由key保全的built-in code。

它不是自动保住与rustc完整release完全相同的机器码/性能：导入预算仍有限，live patchable函数还有不同inline边界。当前作者的单函数缓存和功能oracle不能证明ZIL所需每case B/A≤1.10、Rust/B≥0.90，更不能证明两个不同优化边界“同为O2就同质量”。本轮不重提bounded imports、complete AE、CP084或reverse index旧NoGo。

## 5. ZIL可落地的唯一小候选：完整CGU miss去重

**保持ZIL完整CGU机器模型和现有action key不变**，在同一可信共享cache内按该key抢一次生产权。loser完成自己的其他不冲突unit后再等；winner必须仍执行完整O2、当前请求独立Verify及全部机器预检，且仅在原发布成功点发布可复用对象。waiter拿到对象也必须按现有cache-hit路径执行自己当前机器模型的完整preflight。不得把winner的Verified能力传给waiter；失败/取消无完成值，waiter按自己的deadline正常接管或失败。

ZIL main990依据是`crates/zil-backend-llvm/src/build.rs::compile_api_unit`目前cache.lookup miss后直接走builder，`object_cache.rs::PendingObjectCache::commit`只在完整请求成功后发布；namespace已包含当前compiler/LLVM身份。小实现应围绕这两个既有边界，初期可采用每key专用协作锁并重查已发布对象，而非照搬Blaze的unsafe lock-free表。独立进程若仍使用各自cache根目录，就完全没有共享机会；显式共享一个新的可信cache才进入该机制的适用域。若为保留原“成功后才发布”合同需要锁跨完整请求，必须禁止多个unit锁互持后互等；可按全序抢占或未抢到先释放/排队，不能简单每worker拿锁就阻塞。

**收益边界**：cold多候选有相同完整unit miss才省工作。旧warm并发各candidate先seed，未改变unit已经命中；salt令大root或改动leaf的key不同，就不省那些工作。它不声称解决root68 canonical＋504 import导致的主要重新优化，也不把“只编相同任务一次”的算法事实当作已测完整wall优势。验证仍应保留冷/暖单请求、批量k文件一次请求、2/4候选并发三个独立轴，记录同key overlap、winner/等待/接管次数和全部wall/CPU/RSS；已有完整defaultO2、Verify/preflight及运行质量门一律不降低。此次仅建议，未改生产、未运行这些控制。

## 6. ZIL main990 的shared-local object cache专项审计

现有代码**支持多个进程同时使用同一可信目录的完整对象，但没有对同key并发miss做single-flight**。`PreparedCache::new`将显式cache根目录规范化，并追加`objects-v1/<namespace>`；同一个已安装binary/LLVM文件、相同target/pipeline的进程会共享namespace，tool identity还含文件元数据，不可把两个不同路径的binary副本仅凭SHA相同就假定为同namespace。

`PreparedCache::lookup`计算完整machine key后直接load；miss返回`None`。`build.rs::compile_api_unit`随后直接进入完整LLVM builder，没有claim/recheck/wait/跨进程锁。`compile_units`的AtomicUsize只调度该请求自己的worker，不能协调别的编译进程。`object_cache.rs`的`NEXT`也只保证该进程临时文件命名，不是同key生产权。

落盘已是atomic replacement：`store`使用同目录`.pending-<pid>-<counter>`、create_new、0600，写magic/action key/object checksum/length/bytes，flush后rename为最终`.zobj`。读端重验普通文件/非symlink、magic、key、长度和SHA256，故竞态读可得到旧完整对象或新完整对象，读失败按miss处理。没有fsync/directory sync的durability承诺，也没有same-key write-once或冲突对象比较：并发writer可以各自编译，再替换同一最终key。已有`object_cache.rs::concurrent_writers_only_publish_complete_entries`与driver的`concurrent_requests_share_only_complete_native_objects`仅证明完整发布/功能和后续命中，不断言builder只运行一次；本轮只读这些测试，没有执行。

对象由`PendingObjectCache`携带，到`session.rs`的`publication.commit`成功后才`artifact.commit_object_cache()`；失败/取消直接drop pending。source/IR/独立Verify能力不因共享cache而变成跨请求许可。以上源码固定为`99035931d358add0eaed6d4535035c38413701dd`，具体路径：`crates/zil-backend-llvm/src/{object_cache.rs,build.rs}`、`crates/zil-driver/src/session.rs`、`crates/zil-driver/tests/native_object_cache.rs`。

### 已存真实候选的重叠机会，及不应声称的收益

只读恢复包`recovered-evidence/zil-multi-edit-raw-evidence-20261111.tar.gz`内`raw/multi-edit/runs/main-concurrency-990/n256-k{k}-c4-b0-p0-A/final-log/{0,1,2,3}.stdout`，直接比较四个不同salt候选的完整`cache.action_key`，结果如下（没有修改快照、salt或调用编译器）：

| 原cell | 完整unit key请求数 | distinct key | 重复共享key | 全部builder key | 原warm可省的重复builder |
| --- | ---: | ---: | --- | --- | ---: |
| N256/k16/c4 | 32 | 14 | 6个key各出现4次，均为object-cache | 8个，全部不同 | 0 |
| N256/k64/c4 | 32 | 17 | 5个key各出现4次，均为object-cache | 12个，全部不同 | 0 |

k16共有相同key的位置为unit2/3/4/5/6/7；k64为unit2/3/4/6/7。例unit2的完整key为`1a34291f071931d33fcc04f0b051b11ed56a5fd8bcae706141350a0b342cf19a`，unit3为`b0b083681266fb723998a8bf55ad74b2b59ea96bd066e761e5473b6841ab0c30`。k16四份stdout SHA256依次为`043d7328081e6b66d99d80ea7a98bebcd1d651a21cc33bc6cf6417652cae6f7f`、`102c11efb51a797ec676e9ec7109a617e01b0969dabe065b56c3125c83a15825`、`393e836ceb384a34a205a08029fbc82024be9ea17465d6e9d0266941a0453466`、`a3365e51fca11eb9708ece51d227f7cf34094f4d3ae03440eb2fe00e666d1d2d`，可直接复核而不依赖summary。

可证实有真实同key机会的**新cold控制**应原封保留这些候选source/path/salt及每文件多点常量变化，独立clean oracle继续用自己的no-cache目录；正式批次四进程同一binary、总四CPU、各自输出，指向每批新建的一个空trusted object cache，完全不seed。基础文本和模型并未人为变相同：上述已有key证明六/五个不变unit仍相同，改动root/leaf key仍不同；在此假设下32次unit需求分别只有14/17个distinct producer工作，潜在重复数18/15。**这只是静态同key上界，不是已经观察到在途重叠，更不是wall改善预测**；后续必须保留各key claim/start/end/wait记录证明真的相撞。大root是独有任务且优先调度，去重的小unit即使降低CPU也未必缩短关键路径。k64更不能删除不利格。旧warm矩阵保持原样，不改salt或清cache后仍称warm收益。

### 最小写集与开工前阻断

必要最小生产写集是`object_cache.rs`（按现有namespace/key claim、重查、lease/取消清理）、`build.rs`（miss协调、保持所有当前preflight、等待不饿死自身unit）、以及按成功发布边界所需的pending持有生命周期；后者若可由`PendingObjectCache` RAII封装，可避免改driver，否则仅限`session.rs`现有commit附近。测试写集是object_cache单元控制及`zil-driver/tests/native_object_cache.rs`的真正多进程/同key成功与失败接管控制。不得触及partition/import/cost32、defaultO2、Verify/preflight或machine key身份，不应先增加general scheduler框架。

但“per-key锁一直持有到整请求发布”**不能直接当作已完成的小实现设计**：请求A持X等Y、B持Y等X会死锁，甚至所有unit编译完仍可能在完整请求发布前互等。原调度按cost而非key全序，也不符合全序锁条件。锁一整个namespace可避免循环，却会把独有root也串行化，不能冒称并发优化。提前把失败/取消请求的对象放进正式cache来解锁则违反现有发布合同。

因此只保留Blaze的“先做自己的任务、最后等别人”作为设计检查；ZIL没有其共享daemon调度/memo语义，保持整请求发布合同所需的无循环协调扩展已经超出小补丁的可信收益范围，本轮暂不实施single-flight，不把它包装成已可信的快速补丁。可以把非阻塞抢占失败直接回原编译作为保守降级，但它不是全场景at-most-once，必须如实记录重复和收益边界。此次没有写生产实现或测量脚本，也没有新编译/采样。
