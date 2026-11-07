# 语言边界与独立核验

语言选择以当前规范、真实正例和最近反例裁决；缺实现不改写成永久 Unsupported，旧方案不形成额外兼容义务。生成器只产生候选代码，属性不授予访问能力；独立 Verify 不调用 Analyze/Types 求证安全许可。安全核边界见 `policy/dependencies.json`。不通过转 Rust 调宿主借用检查器替代安全核，不因编译失败隐式引入 RC、锁、堆盒或全局表。

闭包的捕获时责任、调用时消费和目标地址存活分别判定；持有资源不自动等于 FnOnce，目标写不自动等于环境写。Analyze 与独立 Verify 各自重建来源、责任、Desc 存活、转交和间接写效果，不能由同一候选事实互相背书。FnMut 的正常与错误返回须恢复仍由调用者持有的完整拥有环境；已移出的字段不可重复清理，剩余字段按责任清理。失败清理须与成功构造和转交顺序一致。当前支持范围由 `docs/guide/supported-language.md` 和实际测试决定，旧 `.agent/closure-c1-boundary.md` 的 draft12/schema9 描述只属历史，不作当前支持矩阵。

语言接受性、ABI、错误/清理顺序及预算变更须列出差异、最近反例，并同步规范/接口/诊断和验收。候选语法与提案未采纳前不当能力。历史来源：`.agent/closure-c1-boundary.md`、`.agent/design-and-quality.md`。
