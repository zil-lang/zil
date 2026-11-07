# 仓库与文件职责

`goal:goal.md` 是唯一实时任务看板；本分支的 `workflow.md` 和 `coordination/` 管派工、审查和交付。main 承载 [`_zil_design.md`](https://github.com/zil-lang/zil/blob/main/_zil_design.md)、[`spec/`](https://github.com/zil-lang/zil/tree/main/spec)、[`policy/`](https://github.com/zil-lang/zil/tree/main/policy)、[`validation/`](https://github.com/zil-lang/zil/tree/main/validation)、源码、测试、示例及必要用户构建说明。任务分支 `.task/<ID>.md` 是临时合同。goal 不合入 main，源码分支不合入 goal。

永久路径表达能力、场景或不变量，不以施工号命名。任务号只用于 goal、任务分支/临时合同和历史来源。协议版本、诊断码、稳定 case ID 和提交摘要保留其真实身份。生效语义归设计或具名附件，实际 Source 归验证输入或测试，必要精简验收绑定精确源码；施工方案、运行过程、旧研究与失效提案走 goal 或固定 Git 历史。旧目录的存在不增加规范权威。

迁移前列出真实消费者：Rust mod/include、Cargo target、测试选择/断言、脚本、CI、打包、Markdown 和机器 JSON 路径。按消费者闭包同批迁移，保持输入字节、行为、预算、拒绝条件和最近反例。主树不放重复看板、CURRENT/HANDOFF、原始日志或按轮次增加的归档。
