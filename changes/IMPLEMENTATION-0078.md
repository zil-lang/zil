# 078 拥有 Package 捕获的独立核验

2026-10-25（Asia/Shanghai）。公共前置提交 `f13ce1588d8105491ea6f78a2f8a40aa6aeee67f`，schema/wire 13，规范锁 `fadd560193bd9253adbb09e4763867b2740cb7d0db48ad0e35afd89185185c8b`。本提案在实现修改之前保存；依据 `_zil_design` §§8、9 和 `IMPLEMENTATION-0079` 的冻结合同。

## 为什么

Package 字段已有独立责任和旧描述检查，但直接把环境捕获伪装为普通局部会制造第二份 owner、错误地被 StorageDead 消账，并使 Shared 下的字段 Desc 绕过 OwnedCapture 写限制。Verify 必须从真实 IR 重建环境根与视图，不信任前端的 mutable、调用模式或计划事实，也不调用 Types/Analyze 的证明算法。

## 改动合同

1. 每个实际 Package 捕获有且仅有一个同型、非参数的 view local；Bind 只在无入边的入口根 ScopeEnter 后的连续前缀出现。End 只结束词法活动；禁止普通 StorageLive/StorageDead、重绑、错误索引和结束后使用。所有可返回路径关闭视图。
2. 环境 shell 与直接拥有字段在函数入口即持有责任；Bind/End 不产生、清理或重置物理责任。view 债不进入普通局部清理次序和返回清账域。Fn/FnMut 的每个 Return（含 Error）独立证明全部捕获责任确定 LIVE；FnOnce 可交回部分或整包已移动状态。Trap 保持 abort。
3. Shared 拒绝 view 的 Copy、payload 和责任写入，包括 Desc 经过 callee 或嵌套闭包的间接写。非 Shared 的整包和字段操作复用独立 SSA、完整转移及旧债已清规则。End 和重建均检查未来使用的实际字段根，旧 Desc 不能对新实例复活。
4. 拥有闭包分类、自包含工厂参数与清理合同同时接纳已经独立检查的有限 Package。closure 显式参数/结果仍有限 Copy；Package 的 ReadCapture/TakeCapture 和 view 的地址借用仍拒绝。
5. Bind/End 的初始化事实、无 result/operand 分类和作用域检查独立更新。新增实际扫描与数据结构工作计入预算；无 Package 捕获的资源路径不增加视图扫描。

## 限定验收

独立 raw/wire 正例与最近反例覆盖绑定、词法关闭、Shared 直接/间接写、FnMut 正常及错误出口、FnOnce 部分/整包消费、旧 Desc、清理顺序和伪造计划；wire 旧/未来/截断版本继续拒绝。验证采用本 crate 限定测试和精确预算边界，不运行全门，不自审或合并。最终证据记录精确源码 candidate、命令、结果与已知边界，交给非作者审核。
