# 178 后续：保留具体资源的既有 callable helper give 参数

基线 e3392259e1fbae8239bf98f608539285729e8a9f；主任务追加派工。
178 在检查 Store 模式时把其它所有 give 参数一并拒绝，误伤 065 已有的
callable helper 附加具体资源参数。原 065 两个源码输入和预期不改。

返修只恢复当前 helper 合同已经接纳的具体资源：实际声明为 resource 的
有限具体类型，以及内建 ByteBuffer；必须在已核 callable helper 形状中。
无约束 T、StoreElement 对应的普通 T、空名义 bound、Copy 参数与 Package
Value 均不能因为写 give 获得 Managed。Store<T> 仍走原有独立模式检查。
实际 Managed 布局、参数清理与调用次数仍交既有 Types/Analyze/Verify 检查，
不增加新合同、不改公共 schema 或额度。

先保留基线 065 限定失败，再运行原样 065 回归、178 模式邻接与最近非法
参数反例；冻结后由 root 独审。此补丁不替代完整泛型 Managed 模式实现。
