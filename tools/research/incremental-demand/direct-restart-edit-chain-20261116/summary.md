# 直接重启首改：1759876 精简结果

正式被测源码 `175987698fd27ccd26a99e7bf52c7634a6409d44`，binary SHA256 `3411ee9a11c835733783278bc8f8cde7bc96a030b0379da8457f4a0c9f9f026b`；actualmain `9170bb4210554392e36eb7528596612596ebef80`。Rust1.99.1，同O2/CGU16/compiler jobs1、两平衡区块，check/native分开。

| 场景 | ZIL秒 | Rust秒 |
|---|---:|---:|
| 原生5105冷 | 3.772 | 6.991 |
| 原生5105无改 | 0.060 | 0.028 |
| 关闭→先改→新进程首次原生编译 | 3.077 | 1.424 |
| check5000冷 | 1.468 | 1.165 |
| check5000缓存进程重启 | 1.201 | 0.018 |
| check5000连续8次私改合计 | 0.995 | 3.076 |

直接首改两区块C−R毫秒：[1560.3790619999997, 1745.5841919999998]；仍都落后。原生五请求总等待 9.952/R15.724s，不能用总量领先遮住重启短板。check全流程 5.746/R7.682s；cold/restart/restore负结果保留。

用户缓存/产物边界峰中位：原生 42.416/R65.934MiB；check 13.637/R58.355MiB；check八改增长 1.064/R15.148MiB。是请求边界lstat/st_blocks，非瞬时峰值，不含SDK/宿主target/研究资料。旧窗口缓存清理不算产品节省。

正式132请求/44格/4运行质量全部完成，候选/Rust门过；main5105默认ResourceLimit不是快成功，main合入与全面门仍开放。

当前源码另有三请求profile，直接首改 2.971s，257字节+EOF原生oracle全过，无正式跨语言比值。Analyze 112.4ms，4857次初始化/local/scope/hold实际成功阶段复用、143模板；当前5105行ID/输入/结果与全局义务仍独立。首次语义发布5个函数重新序列化、5100个真实decoded编码复用，但encode 157.1ms、发布 272.1ms，不能只以计数宣布耗时收益。

剩余主要等待：Codegen 1498.7ms，内部object 718.3、machine 346.2、partition 312.3ms；不能再加其子项preflight/emit/transform。Expand 304.7ms、function cache read 166.4ms、source ingress 183.9ms也仍存在。当前无旧物化时使用当前图一次规划，诚实报告非disk hit；下一步针对未变组的机器工作产品、稳定分组和首改完整O2对象消费，不再把LLVM对象命中当作整链命中。

已贯通：Analyze请求内相同scalar阶段计算；一次当前图规划及同body观察共享；真实decoder→Types sealed canonical product→semantic publication；编码cache余额同时扣function和plan，原Writer N/N−1与独立Verify保留。无持久Source→Machine权限，无提限，无减O2，研究默认保持。

验收：IR91/Types115/Analyze307/backend331（992子树未变）；driver231/native2（432生产相同）；最终175严格检查、加强当前limits的正负绑定1、短流程磁盘回归1、fmt/release；工具quality22/SDK录制2。非作者预算阻塞已修、原失败/审阅停止与收据单复数误判均有独立记录，未放松原断言。

普通CLI每次启动新进程，这条路径可反复发生；常驻IDE/服务频率较低，没有用户遥测百分比。当前覆盖仍是5k合成项目，未证明工业多crate/proc-macro/build.rs、分钟/GB场景。全面超Rust尚未完成；旧8f/480e/bee7/98fa/021997均保留原数字归属。

详见[精简JSON](summary.json)，本地必要证据out/runs/cache-restart-*04及direct-edit-diagnostic-02。默认不打包/上传全量运行资料，native-link SDK仅身份audit默认不复制payload；既有映射LLVM运行校验快照仍按原正式工具保留。SDK18/native-link恢复继续固定e85/V32，未重建LLVM，未合main。
