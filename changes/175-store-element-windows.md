# 175：动态拥有前缀的共享元素窗口

## 为什么

172 的 `Store<T>` 能持有运行时数量的完整 Package，却只能通过消费尾元素来读
其中的数据。布局和事件遍历需要读取已有元素；真实元素地址会在 reserve、pop、
责任转交及销毁时失效，因此不能只补一条机器 GEP 而省略来源与窗口检查。

依据 `_zil_design.md` 的 ZIL-ACCESS、ZIL-ORIGIN、ZIL-PACKAGE、ZIL-LOWERING。
本提案是既有终态规则的有限实现，不更改默认预算、产品条件或责任模式。

## 是什么

- 裸原语 `store_at(store, index)` 接收精确 `Desc<OwnedStore<T>>` 和 `i64` 索引，
  返回现有类型形状 `Address<false, T>`；T 保持 172 的非零布局、直接 Managed 加
  Copy 完整 Package。源面为 `&Item`，不增加一个可伪造权限的用户类型。
- 新 raw 指令 `OwnedStoreBorrow { store, index }` 由公共 177 登记 opcode 86 和
  schema/Wire 21；其输出是实际 `[0,len)` 元素位置的共享地址。
- 负数及 `index >= len` 是定义的 trap，检查先于元素 GEP/读取。空容器不能取得
  元素。不会按 capacity 在编译器中展开静态元素槽。
- 共享视图只投影现有 Copy 字段；普通 `fn read(view: &Item) -> i32` 可以跨模块
  读取公共字段。Managed 字段、整个 Package 读取/写入、mutable 地址及从局部
  Package 任意创建地址仍拒绝，不因新视图扩大一般 `is_address_pointee`。
- 直接 Package 视图捕获不开放。返回 Address 的函数只要其参数、局部或 SSA 中
  出现本片共享 Package 地址，就明确 Unsupported：包含从元素窗口导出的 Copy
  子地址。此限制作用于该函数，不关闭同一模块内无关的既有合法地址返回函数。
- Copy 子字段的局部地址与局部闭包继续使用既有来源/loan 核验，不能借局部闭包
  隐藏窗口。Store 的拥有/借入捕获仍按 172 的拒绝边界保留。

## 怎么做

Types 只建立 raw 指令和源字段访问，显式限定元素形状及同步接口。公共结构谓词
`is_shared_package_address` 仅分类；独立 Verify 重算名义资源定义、Store 元素
布局、指令类型、输入活性、实际 Hold 和每个 loan，不信任 Analyze 的候选行。

Analyze 和 Verify 分别从 Store 描述建立来源。`store_at` 的根是原 Store 实际
载体；由 174 的 `DescribePackageField` 产生的 Store 描述绑定 Local+Field，
父壳移动/销毁覆盖该字段。字段替换后的新描述不恢复旧窗口。

reserve、push、pop 是容器写效果，len/capacity/store_at 是读效果。所有调用效果
和未知参数分离要求沿普通 helper 传播；不同参数名本身不证明不交。活动窗口
阻挡相交 reserve、push、pop、give、drop 和字段取出/替换。最后一次真实使用
结束后允许相应修改。沿用 171 含资源窗口模块的混合 Desc+Address 捕获保守
拒绝，不能用不完整逐捕获效果扩大资格。

LLVM 从真实头读取 len/data，在边界通过后对已核元素布局做地址计算，返回指针，
不复制 Managed 字段、不构造第二项责任、不加入 RC、堆盒、全局表或 noalias。

## 验收输入

实际源码正例包括运行时输入索引、跨模块 Copy 读取、多视图共享、最后使用后的
扩容/尾取、保留无关地址返回函数；普通消费者核对析构顺序与实际分配失败保持。
相交 174 组合检查字段来源、父壳移动以及不同真实 Store 字段的分离。

最近反例覆盖负数/空/末端越界 trap、窗口后的 reserve/push/pop/give/drop、同源
和未知别名 helper、闭包隐藏地址、Managed 字段/整头复制、mutable/捕获/返回、
伪造 raw 候选 Hold、错误载体/元素/索引与 Wire 输入。预算用精确 N/N−1 记录，
不将 ResourceLimit、崩溃或错误阶段当作语义拒绝。

所有数字只在真实执行后登记；作者只跑限定测试，非作者审核固定精确实现提交。

## 首轮实测修正

960abcf6 的首轮限定测试为 5 通过 / 9 失败：地址类型已通过独立 Verify，
LLVM 三处表示准入仍缺共享 Package 地址，故原生正例停在 Codegen/Unsupported。
补齐精确的已核指针表示，不改变一般 pointee。另两个作者夹具问题分别是
可见性测试缺 import/函数分隔符、四模块样例重复声明外部 putchar；修正源样例，
保持现有可见性和唯一外部声明规则，原始日志和失败提交保留。
