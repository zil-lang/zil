# ZIL 代理入口

先核 checkout、分支/worktree、未提交和未跟踪源码，再读取独立 `goal:goal.md` 及 `goal:workflow.md`。主树承载语言规范、阶段合同、源码、真实测试输入与产品门；任务分配和协调过程在 goal。禁止 reset/clean 或在正在构建的树切分支。

语言权威是 [`_zil_design.md`](_zil_design.md) 及 `spec/`、`policy/`、`validation/product-gates.json` 具名附件。构建见 [`docs/guide/build.md`](docs/guide/build.md)，可用范围见 [`docs/guide/supported-language.md`](docs/guide/supported-language.md)。一任务一分支一 worktree；按 goal 的工作流执行独立 Verify、精确源码复核和真实验收。不得把排期、当前状态、过程报告或任务号文件送入 main。

历史施工和研究文件用固定 Git 提交 `28b3e37bb4d8a1fe8b146c72cf891fc99de32440` 定位，不从主树旧目录推断当前规则。goal 永不合入 main。
