# 验收定义、输入与结果

| 要查什么 | 位置 |
| --- | --- |
| 正式静态工程义务 | [engineering-contracts.json](engineering-contracts.json) |
| 登记的测试计划与案例 | [task-test-plans.json](task-test-plans.json) · [conformance](../conformance/index.json) |
| 产品发布的量化条件 | [product-gates.json](product-gates.json) |
| 固定 Source/外部输入 | `inputs/<capability>/` |
| 精确交付的必要验收和独审 | `results/<capability>/` |
| 原文档迁移/历史位置 | [documentation-provenance.json](documentation-provenance.json) |
| 原任务资产迁移与字节绑定 | [asset-provenance.json](asset-provenance.json) |

当前有限语言验收见[普通拥有与共享派发](results/ownership-and-dispatch/ordinary-ownership-and-shared-dispatch-acceptance.json)，前一轮工程迁移见[仓库组织](results/repository-workflow/repository-organization-acceptance.json)。本次[文档组织与工作流验收](results/documentation-layout/acceptance.json)附有[独立复核](results/documentation-layout/review.json)。文档或工具验证不升级语言验收。

`results` 中的记录绑定各自受测提交，内部路径、命令和哈希按那个提交解释；搬迁不重新测量、不改写旧失败。需要找旧路径时先查来源索引，再读取固定 Git blob。历史过程/中间重试不作为当前目录中的另一份能力状态。

新收据只保留复核所需的范围、源码身份、命令、计数/失败、工具链、原始证据位置和未测边界；持续原始日志写唯一 `out/runs/`，按[交付协议](../docs/development/delivery.md)选择归档。状态与排期仅在 goal，详见[工作流](../docs/development/workflow.md)。
