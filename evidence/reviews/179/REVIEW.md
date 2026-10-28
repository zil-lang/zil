# 179 独立审核：Pass（限定切片）

审核对象为作者源码 `204fea481d8770bf53f2e81324830b4f375d954e`；作者额外证据提交为 `aeab71735fa538e6a0355694ecb8c9aeba13d50a`。审核分支只新增 `review179_borrows.rs`，未修改任何作者生产文件。独立测试提交 `1b7ecf7813e49ed12e38edc65279045f405d0506` 与作者所有生产目录 Git tree 相同，见 production-identity.json。

## 结论和依据

本片把实际字段访问足迹与整个父壳存活分开，未发现阻断。Analyze 的真实 callback 字段以 LocalId+index 记录私有投影，公共候选 Hold 仍保留整个父壳。Verify 不使用前端安全证明，先校验具体 Package 字段、Closure 定义及自包含环境，再重建私有来源。父 Local 根仍与其所有字段根相交，同字段移动/替换和整个父壳结束仍受约束；用户资源的未知清理保持保守。

在独立 target 下，精确作者源码的 120 / 166 / 174 / 175 / 176 / 177 / 179 八组邻接及本片回归共 **91 通过、0 失败、0 筛除**。作者四模块消费者由审核者重新构建和执行 **18 次 O0/O2 原生程序**，含 **8 次分配失败注入**；所有受测分配=释放且 live=0。每个 Build 仍只执行一次独立 Verify。

审核者另加 **4 组**测试并在干净已提交源码重跑：实际 Copy struct 字段替换、ByteBuffer 兄弟字段替换、live Store 窗口与同一 callback 地址可以同时存在；真实当前调用的 Store view 参数与经两层 helper 写入的描述若同源则拒绝；合法但属于 Copy 的字段索引不能伪装 callback；将整个 Package 的实际清理提前到 callback 地址最后使用前仍拒绝。新增正例 **4 次 O0/O2 原生执行**；三个 raw 候选分别经 raw/Wire 六条入口拒绝，均重算指纹。当前 scoped 总测试 **95 通过**。fmt 和 Analyze/Verify/Driver all-target Clippy `-D warnings` 也独立通过。

## 失败记录和边界

consumer01 使用了错误命令参数 --compiler，未运行产品；consumer02 改为工具实际参数 --cli 后通过。attacks01 的审核夹具误查 DropManaged，正确 IR 是 DropPackage；attacks02 把清理提前后仍保留父壳 Copy 读取，因此先命中更早的初始化拒绝。最终夹具去掉该干扰读取，raw/Wire 均命中 E-BORROW-CONFLICT。全部原始失败命令与诊断保留；dirty 发现阶段不冒充最终干净测试。两版旧审核源码按明确编辑反向重建并如实标记，不称为当时抓取的快照。

当前源码仍不接纳一般混合借入/拥有捕获、Store 元素地址捕获、动态生命周期或外部 callback 关闭。测试中的捕获边界是当前自包含 Closure 字段；当前实参的隐式 helper 写由独立攻击覆盖，未把 Unsupported 的捕获当通过。未知用户清理仍使用整个父根；whole-State 未支付责任替换仍在 Analyze 较早拒绝。这些是既有切片边界，不在本次审核中放宽。

本报告不代表完整语言、产品或性能验收。继承 handoff integrity Fail 单独保留，未改历史状态。完整语言 NotSatisfied、产品 NotRun、B0 Doing；未运行正式全量，未修改预算或阈值。全部原生可执行程序、编译 target 和缓存均未晋升到证据，仅保留一个 O0 阶段/机器表示链和小型 raw/Wire 输入。
