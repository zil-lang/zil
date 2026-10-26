# 049 有限资源表示、原生清理与集成

## 为什么

draft08 已有一层地址和动态 IO，但 Managed/Desc/give 与自动清理尚未经过真实源码到机器链。本轮先闭合有限资源载体，避免以地址转交或手填IR冒充资源语义。

## 明确的源面与边界

规范第7章补齐资源 Drop 唯一绑定：`impl Drop for R { fn drop(self: Self) { ... } }`。Self 为 Desc，清理激活授予访问；源普通调用、FunctionRef 和一般 Call 不得调用隐藏 teardown。无自定义 Drop 时只有字段清理。现有 trait 语法不新增关键字。

首片资源头只含有限 Copy payload。普通资源参数是 Desc，give 参数和工厂 create/give 返回为 Managed；普通资源赋值取描述，显式 give 使用 TakeLocal。Desc 不是 &T，不建立持续只读窗口，不产生 noalias；内联头移动、清理和再次激活仍必须阻止旧描述悬垂或复活。

正常块结束、return、break、continue 必须执行实际清理，按逆成功构造序。新 opcode DropManaged 对真实局部 live flag 条件清理，Store/Take 同步维护flag；进入cleanup先移交/暂停责任，再执行已核入口，不在释放后的存储中写标记。资源替换、嵌套资源/拥有聚合、Desc返回/捕获/嵌入、一般地址到资源暂为Unsupported，终态责任不撤销。

既有 checked trap 为进程abort，不作unwind；可恢复错误、panic展开、取消和异步逐出口清理尚未实现，不能宣称完整失败合同。没有通过清理特例放开FFI或资源安全。

## 公共表示

- schema/raw wire 统一发布6；旧格式1–5拒绝。
- CoreFunction和TypedFunction有result_mode；CoreAggregate有显式hidden teardown符号。
- TypedModule与LirModule记录ResourceDefinition{name,payload,teardown}。后端精确绑定VerifiedModule中的表。
- 新操作MakeManaged、DescribeLocal、ReadDesc、WriteDesc、DropManaged；所有字段均未核，序列化不制造权限。
- 不扩CandidatePlan的大证书；独立Verify重建资源来源、SSA/局部责任和清理余额，调用不得依赖前端求证状态。

## 验收

真实两模块工厂/转交/描述操作、分支和循环退出、构造顺序与参数自动清理；O0/O2观察相同轨迹并与独立Rust程序比较。Raw反例覆盖遗漏/重复清理、重复责任、Desc冒充Managed、旧描述复活、普通调用teardown和错误create来源。wire逐字段绑定，篡改/旧版本拒绝。正式全量由root执行，精确实现由非作者审核。
