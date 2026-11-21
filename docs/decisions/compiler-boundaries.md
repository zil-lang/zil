# 语言边界与独立核验

语言选择以当前规范、真实正例和最近反例裁决；缺实现不改写成永久 Unsupported，旧方案不形成额外兼容义务。生成器只产生候选代码，属性不授予访问能力；Verify 不信任 Analyze/Types 的候选结论，用共同的 OpSpec 与语义内核核验证书后才授予安全许可。规则只定义一次，独立性体现在核验职责而非重复实现，见 [`spec/semantic-kernel.md`](../../spec/semantic-kernel.md)。安全核边界见 [`policy/dependencies.json`](../../policy/dependencies.json)。不通过转 Rust 调宿主借用检查器替代安全核，不因编译失败隐式引入 RC、锁、堆盒或全局表。

闭包的捕获时责任、调用时消费和目标地址存活分别判定；持有资源不自动等于 FnOnce，目标写不自动等于环境写。Analyze 求解来源、责任、Desc 存活、转交和间接写效果并给出证书；Verify 逐条核验证书的转移、边与出口义务，候选事实不能自我背书。FnMut 的正常与错误返回须恢复仍由调用者持有的完整拥有环境；已移出的字段不可重复清理，剩余字段按责任清理。失败清理须与成功构造和转交顺序一致。当前支持范围由 [`docs/guide/supported-language.md`](../guide/supported-language.md) 和实际测试决定。

语言接受性、ABI、错误/清理顺序及预算变更须列出差异、最近反例，并同步规范/接口/诊断和验收。候选语法与提案未采纳前不当能力。
