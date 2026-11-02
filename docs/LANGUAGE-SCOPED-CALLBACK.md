# 结构化外部回调阶段验收

266–272经[PR21](https://github.com/zil-lang/zil/pull/21)合入`a7f0783a56dce71642ef3980d554950a9c421f79`。受测源码`fa34160bacb8ca5bd4c998467be6e6177e3a5aec`与合入版本的生产代码、测试及工具相同，只有四份Type/Syntax文档差异。[命令和精简结果](validation/scoped-callback.json)。

同线程Direct `c_with_callback`现在贯通实际拥有F登记、公共泛型wrapper、具体本地body、重入拒绝、关闭确认与环境释放。独立Analyze/Verify核验实际F独占窗口、body消费及真实角色流；runtime schema/Wire32，CT artifact1。此结构化登记在调用返回前关闭，不等于异步、跨线程或可返回登记对象。

本地正式默认3904/0、reference3796/0、text-reference64/0，均零忽略与过滤；格式、全仓全部目标Clippy、迁移9项通过。两套PR CI均成功。独立审核重算原日志并核对精确提交。

真实源码经过生产Lower到LLVM API及clang参考后端，各后端原消费者66次原生执行，加不相交借用与本地body各2次；原66次含34次真实分配失败，所有正常返回live=0。13个非法源码保留准确阶段/code，重放不累计为新覆盖。手填LIR的后端单测单列，不代替源到机器证据。

独审暴露并修复了普通存活F别名漏判、符号body声明消费及具体Copy重复使用判据；原失败不改写。显式Address<Closure>捕获仍有旧profile缺口。273–275继续泛型函数内具体资源准备，277–280接具体handler和工厂body组合。

本批让已有拥有Event/Reply语言能力进入真实外部回调生命周期；未建立相对Rust的运行、内存或编译性能优势。完整1.4/ZUI、B0、产品门与原生Windows仍未完成。
