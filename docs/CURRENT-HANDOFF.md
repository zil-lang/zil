# 当前接续入口：施工8 / 222–224

当前真实 GitHub `main` 是开发基准，接续前重新读取远端 ref。施工8从 `a8e8831a9e28ac2ec83be6eac740a8f04a00af7d` 接续已验收的218–221；本批222–224的实现与返修正在独审/正式集成，当前状态以 `.task/impl/222.md`–`224.md`、`data/construction-progress.json` 和 `evidence/integration/222-224/` 为准。

## 本批范围

222为有限源级 `#for`，223为完整嵌套 Product 的 Store 元素资格和失败清理，224为完整 Product 的同步共享借入。组合可用行为、真实审查缺陷、原字节重放和剩余边界见 [LANGUAGE-222-224.md](LANGUAGE-222-224.md)。不得用作者限定通过替代独审、全量或实际远端合入。

下一范围仍包括完整 Type/Syntax / `#emit` / 类型声明生成、更深拥有容器组合、一般 trait/lending、长期外部回调关闭/注销/重入，以及挂起、取消和并发。完整1.4、B0/181、全部ZUI和Rust产品门保持未完成；不降低 `validation/product-gates.json`。

## 接续约束

一单一branch/worktree/target，禁止跨工作树复用 Cargo target。源码及测试提交由 Git 保留，作者、非作者和正式CI身份分别记录。GitHub验收必须读取实际checkout/tree及全部workflow步骤，不能只看默认workspace一项绿色。主线合并使用保留审核提交的merge，不改写历史身份。

冷启动运行 `node tools/handoff.mjs --check`，再读取主树规范、总目标及派定工单。历史handoff缺项与本批新增问题分列；稀疏checkout不展开旧evidence时可能额外报告缺文件，不能据此重标历史成绩。规范输入注册表使用 `python3 tools/update-indexes.py` 生成；已合入旧片仍锁其实际implementation_commit规范，不追改旧hash。

`research/compile-performance` 为另一独立施工线；此语言批次不合并其未经本线验收的进度，也不声明编译性能收益。旧209/136/137交接内容保存在Git历史，已不代表当前主线。

施工期间只更新 [zil-log](https://chatgpt.com/space/page_5799fccae05c8191b25b68c26ec7a4ab) 的 **zil施工8** 一行，时间为北京时间；收尾明确标记阶段完成。日志是实际活动记录，不承诺会话结束后继续后台运行。
