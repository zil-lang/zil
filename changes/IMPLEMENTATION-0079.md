# 079 完整组件的拥有捕获：公共表示、机器布局和原生闭环

2026-10-25（Asia/Shanghai）。生产基线 draft15 fc7aaca720591f2361813ed5c03f684b388a2ffa。此提案先于本片生产代码；依据 _zil_design §§8、9 的拥有捕获、调用能力及责任顺序。

## 为什么

有限 Package 已能在局部更换资源，但不能被保存或返回的闭包整体拥有。本片使组件环境保留真实数据和逐字段责任，重复调用不搬出再填回整个组件。图研究076独立，不参与安全核的验收前置。

## 冻结合同

- 只开放现有有限 Package 的显式 move 捕获：至少一个直接 Managed<Record>，其余有限 Copy；无嵌套拥有包、闭包、Desc、Address 字段、整体用户 Drop 或部分 SSA/ABI。默认借用 Package 捕获仍 Unsupported。闭包显式参数和结果仍有限 Copy；普通工厂可接收完整 Package 并返回自包含环境。
- 公共 schema/wire 13、typed fingerprint domain V13；既有指令编号不变，新增无 result 的 `BindPackageCapture { local, index }`（57）和 `EndPackageCapture { local, index }`（58）。每个 Package capture 恰有一个 local 视图，即使未访问，也必须入口 root ScopeEnter 后的连续 Bind 前缀一次绑定；入口不能有入边或回边，索引/类型必须精确对应实际 function.environment，local 不能是参数、不能别名。普通捕获初始化随后发生。
- Bind/End 自己负责视图词法激活/结束：禁止这些 local 的任何 StorageLive/StorageDead。Bind 设置初始化事实，End 清除；End 接受 FnOnce 已整包移走的状态，不要求值仍初始化。无重绑、结束后使用或跨路径不一致。作用域必须关闭全部视图。ReadCapture/TakeCapture 对 Package 均禁止，BorrowLocal(view) 禁止，字段 Desc 仍受实际根和未来 liveness 检查。
- 视图的数据指针、whole-shell flag 和每个直接 Managed field flag 与真实环境存储同址。物理责任从环境入口存在，Bind/End 不创建、清理或重置 flag；End 将访问权交回环境。视图不加入普通 local 的逆初始化清理顺序。环境最终按捕获声明顺序，再按 Package 字段声明顺序，条件清理剩余 live 字段。
- Fn（Shared）禁止通过视图、Desc、callee 或嵌套闭包改动捕获 Package 的 Copy 内容、拥有 payload 或责任。FnMut（Exclusive）每个正常 Return，包括 Error Return，都必须恢复 shell 及全部拥有字段确定 LIVE；FnOnce（Consume）可以留下部分或整包已移动状态，由 caller 的环境清理读取保留的真实 flags。Trap 保留现有 abort 边界，不新增恢复或 unwind。
- 非 Shared 允许整包 TakeLocal、显式 DropPackage 和 Store，沿用完整转移及旧债已清规则；普通源码 whole 赋值本片仅沿用 whole-moved 后补回，不新增活 whole 替换语法语义；FnMut 仍必须恢复。完整 Package 值 ABI 不变，只有 closure 环境 ABI 增加持久 shell/field flags；类型、布局、模式、绑定指令及其索引均进入指纹。无隐式复制、RC、Box、锁或全局表。
- 源码模式通过有预算的 Package 槽责任流分类：纯读 Fn；所有普通/错误出口完整的写或替换 FnMut；任一普通出口缺槽/whole move 为 FnOnce。分类不授予安全许可。Analyze 与 Verify 从实际 IR 分别重建状态、作用域、别名及退出完整性。不会因检查失败自动重试 consume 模式。

## 最近反例与验收

错误/重复/越界/非入口 Bind、多个 local 指同一环境字段、ordinary Storage 重置、End 前后非法使用、Shared 经 Desc/函数/嵌套捕获写、FnMut 单条错误出口缺字段、FnOnce 部分字段清理一次且顺序固定、旧描述跨 take/store、view 逃逸、精确预算 N/N−1、无 Package 的费用路径。源与真实 LLVM/Rust O0/O2 对照覆盖工厂返回、自包含保存、重复更新、错误恢复、部分消费及观察到的 teardown 序列。

077 作者 Types/Analyze，078 独立 Verify，079 root 公共表示及机器闭环。作者仅限定验证，非作者审核精确 SHA；root 统一正式全门。该有限阶段不宣称语言完成或 ZUI 产品门通过。
