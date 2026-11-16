# 当前接续：重启缓存与完成机器链恢复

当前源码 **4a23edc7d06bc18cbed1fb958fdaacd886d9607a**，tree `35116da8321766cf49106143bd699faada166dbc`，release sha256 `302d58db48f53cd4bc1952f2a9032ead5a6d479314d24eb2ca344f558c2c113e`。唯一施工分支research/full-push-native；main9170未合。SDK18/native-link继续沿固定[V32](https://github.com/zil-lang/zil/blob/1eecf9b81a4929dde79e4fa98cd0ee40fb74af75/research/incremental-demand-full-chain-20261113-v32-recovery/RESTORE.json)恢复，未重建LLVM；研究开关默认关闭，独立Verify、原默认预算、完整O2保持。

本批已完成实际逻辑：push重建接入既有单函数CFG缓存，同一次当前Core/完整上下文/签名读取检查重放原费用并记录真实push状态；记录不全即原路重建，不从下游Typed/hash伪造上游epoch。带life声明在计费前直接走一次原Builder，避免试探miss重复计费。稀疏编辑不重写未变函数manifest。真实Types/Candidate双owner匹配后在Analyze provider之前恢复已有状态。

后端完成Transform/Partition/Diagnostic/ObjectKey产物附在既有四槽MachineProduct历史中，同槽淘汰，不另建无界表；当前TransformPolicy、protected集合、原PartitionInput及当前Verify/LIR/预算/首产原模型检查完整保留。A→B→A可恢复实际完成链，首次/不兼容仍原路运行。两成功出口才附完成产物，无新磁盘SourceMachine授权。

错误恢复也已修复：Entry区分实际磁盘候选和当前进程真实产物，仅实际磁盘命中触发原防御回退；使用同容量临时会话重算同一不可变输入，成功才替换、失败保留成功producer/push/Verify状态。原先self.clear导致的五个真实恢复失败已按原断言修好，没有放宽安全分类或限额。

当前实际通过Types110、Driver224、Source9、native publication1及mechanical/strict/fmt/release。backend299在b197实际通过，此后backend/API逐blob无差异，精确复用；Expand124/CT76也按未变差异复用。102文件例确认303个合格函数重启复用、随后私改仅1体检查；另有真源错误/恢复、签名变化、累计费用边界、取消与配置变化。Astra按精确源码独审；首败Types108/1、机械private访问、Types109/1、Driver219/5全部保留，见[审阅和收据](restart-epoch-reviews-20261116/current-acceptance.json)。

用户要求发现落后后直接改逻辑，本批没有追加性能矩阵。**上述新源码没有新的速度或磁盘净收益数字。**最近已测是bcf409b37，不能转归本批，也不能转归021997或98fa。bcf两窗396原始请求/132功能格/61运行Rust门过；完整流程1k/2.5k/5.1k为6.807/8.367/28.926s，对照同窗Rust8.458/9.410/32.233s；main5.1k仍Expand限额。bcf5.1k重启2.725s、首改1.223s，联合3.948s，Rust联合1.304s，仍落后。旧f19完整流程24.823s属于不同窗口；bcf28.926s的退化与Rust控制漂移并列保留，绝不据跨窗数字宣称净算法提速。bcf原始rows/summary/manifest及重启首改JSON子集已发布，完整cache/artifact raw仍本地，未冒称全量恢复包。

下一按绝对等待成本处理全局机器变换/分区：旧bcf对象16/16已命中，machine1.087s/partition.315s是定位桶。一般单函数编辑仍可能触发全量checked-add/scalar/call-class；须记录实际callee、DAG/class代表和address/protected依赖，不能只缓存最后rewrite或删除原模型检查。冷进程仍缺实际上游产品持久恢复，不能把补建藏到后台或编辑timer之外。改完相关昂贵链后再统一比较。

新代码的cache编码/解码/盘量及四槽下游内存保留代价尚未量化；有限标量单crate/O2/no-debug不代表工业大工程。10105/25105仍受原token/10000函数合同限制，全面超Rust目标开放、main Hold。旧f19全面证据固定7b604bef1/afc97106d，旧98fa仅绑98fa/59fd，goal/archive永不合main。

当前[恢复入口](evidence/restart-function-chain-20261116/RESTORE.json)包含当前与bcf二进制、当前所有门/首败、审阅和bcf原始请求收据子集：1份/19,598,435bytes，96files/61,849,972raw，已从空目录完整解包逐文件hash验证。它不包含两份bcf完整cache/artifact；SDK18依赖固定V32。旧两native-dependency证据目录仅sparse-excluded，旧raw退役均有全hash ledger；不据恢复后的hardlink重算产品磁盘收益。
