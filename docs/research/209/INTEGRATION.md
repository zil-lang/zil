# 209 主线整合决定

## 合并而不是覆盖

远端main在2026-10-28读取为`ab2aaa7aa25e3c10d2908801d5e06dff1079df25`，最近成功CI为40330123694。从该运行的artifact 12653433445恢复`repository.bundle`，校验外层SHA256 `a1e998fe905d5720009ae8581a857c7d9ce01dcf84d3d1f5151c9851669f10ca`。

共同祖先为`1d7216f0c72f055a4e484f786a1a75a11f1cd12f`。源码squash将208带到远端基线上，保留远端字节截断修复、任务记录和verify-main.yml。5处冲突仅涉及schema/wire18与21及相应测试/接口说明，取208的21版本。此步crates逐字等同208；8100项历史证据未进入新commit。GitHub旧历史保留，实验历史继续由旧交付保管，不导入大体积数据。

## 研发默认策略

用户明确要求积极集成与终态主路线。选择：不把208长期保留为旁路探针，标准CLI构建启用llvm-api，实际build/run默认使用API；--backend clang保留差分，无特性参考CLI需--no-default-features明确构建。底层库默认配置保持，入口负责主路线策略。源码checker和Verify没有改写；LLVM结构核验仍在优化前后执行。

主线CI保留原contents:read和禁持久化凭据，添加LLVM17.0.6安装校验、API默认构建、无特性完整回归、真实API对象消费者/分配失败测试。未运行远端CI前不登记为通过。当前Linux x64开发宿主验证不等于Windows宿主验收。

这是用户授权的研发默认变更；规范19.5已明确与产品默认推荐区分，产品量化门未放宽。直接Builder、结构化CGU、稳定符号、对象缓存、ThinLTO都没有被此默认开关实现。后续在现有LIR→单元→对象接口逐族替换，不为维持实验并行而长期建设第二套编译器。

## 验证与供应链边界

208本轮重新运行API-enabled workspace1660项通过。209新增CLI真实选择测试，最终新数据见交付报告。完整workspace包含既有Clang库级回归，不全部冒称API测试。性能未新增测量、未宣称更快；默认CGU仍为1。

只允许正常fast-forward push。若origin/main不再是冻结基线，停止并重新整合，禁止force push覆盖他人修改。只读GitHub连接器与当前沙箱无GitHub直连不代表用户仓库没有写权限；本轮无法推送时交付绑定该基线的bundle，不以本地提交替代远端同步成功。
