# 独立语义对照的边界

生产的 parser、types、analyze、verify、lower、LLVM 不得计算独立 oracle 的预期值。标量模型用 JavaScript BigInt 表示有界 i32/i64，整数输入用十进制字符串保存；生成真实 ZIL 源码、经 CLI 编译后执行 native 程序。资源闭包模型以独立 Python 状态表达捕获与调用，再对照真实源码与机器行为。热/冷会话一致只证明缓存透明性，不能替代语义 oracle；模型通过也不代替独立 verify 的安全职责。

边界值、自检变异、最近反例、超时/信号/编译失败辨别以及工具身份必须保留。对照只证明登记域和冻结提交，不能外推完整语言、ABI 或 OS。实际执行数与结论由当轮收据提供，旧计数不作新验收。

验证入口：`tests/oracle/model-check.mjs`、`tests/integration/closures.py`、`tests/integration/borrowed_closures.py`。历史来源：`.agent/oracle-independence.md`。
