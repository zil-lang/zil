# 178 既有 helper give 回归修复

源码 `6b2a700a017679d7ded5550d313501578d082972`，基线 `e3392259e1fbae8239bf98f608539285729e8a9f`。178 的 Store 参数检查把非 Store 的所有 give 参数一起拒绝，误伤已有 callable helper 的具体 Managed 资源入口。现在仅在已核 callable helper profile 内，恢复实际 resource 声明的具体类型及 ByteBuffer；普通无约束 T、StoreElement 对应的 T、空 bound、Copy、Address 和 Package 不获得 Managed 模式。

065 原始输入/预期没有修改；064、065、070、178 与新近邻测试在干净冻结源码共 37/0/0，通过既有原生与 Rust 对照路径。新增六个非法 give 输入和一个 ByteBuffer helper 正例。fmt 与 Expand/Driver all-target Clippy -D warnings 通过。实际资源布局、调用、错误清理仍由原 Types/Analyze/独立 Verify 处理，本补丁不增加新模式、不改 schema、预算或产品门。

基线 065 的两个真实失败、开发中新夹具 main i64 的入口签名错误、继承 handoff Fail 都完整保留。基线命令开始于仅多了提案文档的 78b2928，其生产与 e3392259 一致。最终所有命令记录实际退出码和干净源码身份。未运行正式全量，等待 root 独审。
