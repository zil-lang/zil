# 闭包验收夹具合同

范围：`conformance/closure.json` 中 CL21—CL26。目标模块名为 `conformance/support/closure_runtime`，源码统一 `import conformance.support.closure_runtime as fx`。本文件固定测试前提与协议元数据，不提供可执行实现。

**状态：NotImplemented。全部引用案例状态为 NotRun。** 接口合同是待施工的测试输入；尚不存在已验证实现、可信外部调用凭据或运行结果。T08 负责闭包侧行为，资源模式、管理路由与准确清理由各自合同的施工方提供；最终仍由相应工单与产品验收核验。不能把本文件的声明当作已经完成实现审计。

## 1. 类型与操作形状

夹具导出一个名义 `resource Tracked`，一个实例具有 immutable `i64` 测试载荷、Live/Closing/Done 协议状态，以及一次实际清理责任。真实表示及测试事件记录由夹具实现定义并纳入其审计，语言不因此默认引入全局资源表。

固定源码接口如下；这些声明只用于说明必须实现的名字、形状与已有合同语法，不提供函数体，不是 FFI 实装：

```zil
pub resource Tracked {
    value: i64
}

pub fn make(value: i64) -> Tracked
contract {
    result(create)
};

pub fn read(file: Tracked) -> i64
contract {
    requires(read(file), state(file, Live))
};

pub fn take(give file: Tracked) -> i64;

pub fn close(file: Tracked)
contract {
    requires(manage(file), state(file, Live))
};
```

`Tracked` 的普通参数模式是 Desc；`make` 的 create 结果建立实际 Managed 载体；`take` 的 give 参数要求真实责任载体。公开接口必须携带下表的完整协议/清理元数据，不能把示意声明中未能列出的退出合同当作空合同。编译器没有装载并核验这些接口时，应报告 fixture 未实现并保留 NotRun，不能把“找不到模块”计作负例通过。

| 操作 | 入口与来源 | 正常出口 | 测试事件 |
|---|---|---|---|
| `make(value)` | 无借入资源；测试配置规定构造成功；无回调、挂起、重入 | 发布一个新鲜 Live 实例、一次责任及可用管理路由；返回其 Managed 载体 | `create += 1` |
| `read(file)` | Desc；目标 Live、载荷已初始化且当前上下文有读资格 | 返回独立 i64；目标仍 Live；不返回地址、来源依赖或责任 | `read += 1` |
| `take(give file)` | 实际 Managed 载体；目标 Live；责任转到被调激活 | 取得独立载荷，完整关闭实例，责任 Discharged，返回 i64；不回传描述 | `take += 1`，`close += 1` |
| `close(file)` | Desc；当前上下文具有与该实例匹配的 BindingRoute 和 Manage 资格；Live；无冲突使用者 | Live→Closing→Done；实际清理一次；更新责任记录后不再读已释放存储 | `close += 1` |
| `Tracked` 自动 Cleanup | 实际持责载体；按实例当前协议状态决定剩余义务 | Live 时完成一次关闭；已 Discharged 时不再次关闭；结束时责任余额为 0 | 只对实际仍需关闭的实例增加 `close` |

`close` 和自动 Cleanup 在这个专用夹具中无可恢复错误、无挂起、无重入、无用户回调；这些条件是本夹具的测试合同，不是 ZIL 对所有析构函数的默认要求。测试事件记录不得改变目标的权限、存活或析构顺序。

## 2. 环境与管理路由的必须行为

`let file = fx.make(17); let alias = file` 只建立一项责任；alias 是 Desc。默认捕获 alias 并调用 `close(alias)` 需要真实外部管理记录的借入路由，环境 Hold 必须覆盖该记录寿命；借入路由不把责任搬进环境。

`move () => fx.read(file)` 捕获直接 Managed file 时，将实际责任从局部激活转到环境。若只捕获 `alias`，即使写 move 也只复制 Desc，不能自动找到 file 并接管它。

`() => fx.take(give file)` 的正文已明示责任消费。闭包创建时先将真实 file 载体转入环境，调用时再通过 give 传给 take。两次转交都只移动同一项义务；源激活和已消费环境不能再自动关闭该实例。

`move () => fx.close(file)` 履行环境自身持有的责任并更新其清理状态，属于消费环境的 FnOnce，不允许伪装成 Fn 的共享环境调用。`() => fx.close(alias)` 的环境可保持 Fn，但第一次调用后的外部 Done 状态使第二次调用不满足前提。

## 3. 执行与证据要求

- CL21 与 CL23 的运行验证必须同时检查返回值、事件计数和责任余额，不能只验证没有崩溃。
- CL22、CL24—CL26 必须在接口夹具已正常装载且完成相关阶段时得到指定语义诊断；模块缺失、未实现、基础设施失败和超时都不算通过。
- CL21 的闭包正文不执行；环境未调用时仍需清理一次。优化可以删除无可观察用途的环境物理布局，但不得删除、重复或提前改变可观察的清理事件。
- 所有事件和诊断证据须绑定源码、规则版本、夹具实现版本与编译器版本。当前文档、接口声明和手工期望不构成编译或运行证据。

## 4. 无夹具案例的入口约定

CL01—CL20 除 CL12 外，运行正例均暴露无参数 `check() -> i64`，由通用用例驱动调用并比较 JSON 中的 `expected.return_value`。负例仅要求完整模块到达指定编译阶段，不需要添加 main。CL12 是函数参数级静态合同用例，输入 `Ref<Node>` 的 Live、初始化和写资格由其导出合同验证，不伪造 Ref 构造原语。

`expected` 内的 capture、call_kind、Hold 等内容是需检查的语义期望；施工时应由独立测试接口读取实际编译产物，不能把这些字符串原样打印成“编译器证据”。

## 导入夹具的构建要求

以上代码只展示人读签名形状，不是可直接编译的完整 extern 声明。T08 必须实现并装载与表格逐项对应的已核 ExportedInterface，含 Live/Done 转换、Give 参数/结果模式、OriginFlow、BindingRoute、Hold/Call/Cleanup/Resume、所有正常/错误/清理出口及效果上界。make/read/take/close 的真实实现、事件计数和此接口须一同验收；不能只复制片段签名后把合同缺失、模块缺失或 Unsupported 计成目标负例通过。
