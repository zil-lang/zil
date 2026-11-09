# 存储借入中的同步拥有回调与本地字节视图

普通库可以在同一存储借入程序中，经 trait 和泛型 helper 借入 Item，由具体同步 Fn、FnMut 或 FnOnce 产生独立拥有结果，再读取或消费该结果。输入 loan、闭包环境的调用模式和结果的 affine 责任分别求证；新结果不解除输入或迭代器的父 loan。完整签名仍须满足[借入输入与拥有回调结果](borrowed-input-owned-callbacks.md)的资格，实际环境不能替代声明约束。

## 本地共享字节视图

新组合只从实际本地 Managed<ByteBuffer>，或普通本地 Package 的直接 Managed<ByteBuffer> 字段建立共享字节视图。资源必须是唯一、无用户 teardown 的规范封闭 ByteBuffer 定义；实际 Desc 必须由对应 DescribeLocal 或 DescribePackageField 产生。参数、转发或仅有相同名义类型的 Desc 不能取得该本地来源资格。这个新增路径要求 Package 其余字段满足完整封闭 Copy 资格；已有其他路径继续独立检查。

视图是共享的 Address<Slice<u8>>。可同步执行长度、下标、切片、共享再借入、u8 读取及已准入的本地共享地址存储；每次访问保留真实拥有头、直接字段、当前 loan 和祖先关系。本地头仅用于保活及冲突检查，不被解释为载荷地址。移动、取走、替换、销毁和初始化责任仍由资源检查核实，旧头或相同长度不授权新一代对象。

视图活跃时，重叠的修改、释放和独占窗口不能通过。这个新增有限路径也不允许视图跨普通或间接调用、闭包创建/调用、外部回调或线程/同步组合取得额外效应许可；视图消费完毕后，后续操作仍按原合同检查。嵌套拥有容器、捕获中的新 ByteBuffer 路线、任意来源转发、可变字节视图和借入结果导出不由本合同开放。

## 独立求证与成本

Analyze 和 Verify 各自从实际 IR 重建描述生产者、规范资源和本地字段身份。不可变生产者表只保存结构坐标；活跃性、初始化、权限及冲突按当前函数窗口重新求证，完整资源求解与最终检查保持。存储借入程序中的实际 borrowed-owned 闭包入口须进入相应检查，不能因另一已准入分支而漏查。raw 与 Wire 不消费 Analyze 的权限结论。

活跃 loan 收集遍历机器字和实际置位，并分别预付这些真实工作；SSA、local、捕获、外部保留端口及父 loan 的贡献仍完整保留。独立 Verify 的已索引路径保持原访问及计费。必需的固定点轮次、默认限额和失败分类不变。

## 完整消费者

[原四模块程序](../tests/language-readiness/owning-trait-hosts/owned_pipeline/model.zil)共 6052 字节，组合 trait 拥有返回、普通泛型借入、借入输入/拥有输出闭包、本地字节视图及错误清理。完整 Source、独立 raw/Wire、双后端 O0/O2 与逐分配点失败责任由[限定验收记录](../validation/stored-owned-callback-pipeline.json)绑定。更大的 collected 程序由[拥有结果收集合同](ordinary-collected-pipeline.md)及其独立收据覆盖；可增长 HashMap 与正式语言发布门仍有各自完成条件。

