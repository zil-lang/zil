# ZIL GitHub 同步与并行施工入口

## 已同步的来源

组织：zil-lang。私有仓库：https://github.com/zil-lang/zil 。本次初始化来源为 `ea782c91007cf356bf99f3c51d55482c38e134ee`，即 draft24 已集成版本。生产/测试受测提交为 `ac60b2866df211a1db0f39c9e60db64da6088331`；其后只有审计/文档整理。完整 workspace 1232 passed、0 failed、0 ignored，冷恢复另有独立构建和33项特性测试。这些是绑定原提交的已有证据，不是 GitHub Actions 新运行。

当前语言范围、图研究结果和限制见 `docs/DRAFT24-LANGUAGE-GRAPH.md`、`docs/RUST-GOALS.md`。123 研究为 Inconclusive / NotEligible，生产已回退；不得将探索期较好点值或本次同步当作性能准入。128–130 是随后启动的新工作，待独立审核后再同步；本快照不声称含未完成结果。

## 为什么远端有独立起点

原 Git 历史包含三个超100MiB原始证据对象（101、107、108），普通 GitHub 推送会拒绝。因此本仓先导入完整非证据源码和紧凑JSON审计记录；大型/二进制原始证据及原始提交图继续完整保存在已交付归档。没有 force-push、重写或删除原施工历史。本仓新提交使用自己的真实SHA，`GITHUB-SNAPSHOT.json`绑定原源码身份、导入文件内容与唯一AGENTS协作入口覆盖；原AGENTS另存于`transport/ORIGINAL-AGENTS.md`。

完整原始 Git 历史不是这个新源码仓的可达祖先；`node tools/handoff.mjs --check`的历史对象缺失不能解释成历史审核通过。`python3 tools/check-github-snapshot.py`只核对导入文件字节/模式，既不证明语言安全，也不运行性能门。

## 直接交给 Codex 的做法

选择私有 `zil-lang/zil`，从main创建独立分支，例如`research/identity-codex`或`impl/owned-container`。先读`AGENTS.md`、`_zil_goal.md`、`_zil_design.md`、`docs/research/EXTERNAL-CODEX-HANDOFF.md`以及本文件。其他模型同时工作时约定写集、基线和任务目标；通过Git提交、分支、PR及证据文件共享信息，聊天和进程内记忆不会自动共享。研究应报告实际commit、完整命令、功能/额度/增长/性能的原始结果及失败，不只发结论。

源码可直接使用仓库锁定Rust工具链构建：`cargo test --locked --workspace`；原生测试还需与`tools/toolchain-profile.json`一致的Clang/LLVM。缺工具链、未执行、Unsupported、ResourceLimit不得记为Pass。正式性能计时应使用独占窗口，完整候选与基线保持相同工具链、目标和固定场景权重。

独立贡献完成后保留提交并提供PR/分支地址。原施工环境取回差异，在原历史上审查、应用和集成，再把已验收状态同步回来；不把本源码镜像的历史缺失作为绕过原审核的理由。现有私有主仓足够开始，ZUI可继续独立项目，无需为本轮新建图研究仓库。

## 原历史和证据恢复

已保存文件总索引（含SHA256与下载地址）：https://chatgpt.com/library?file_id=libfile_faad020070308191abc3f86dbf43bdaf&account_id=personal

当前源码：`zil-draft24-20261026-source.zip`；SHA256 `64c23acea6d0a064754faca63ecb1a25c748160e2ee4a07cf6bf5e2a8d43a2cd`。

原draft22历史三片按part01→part02→part03拼成base，校验SHA256 `0b1ab9de9684c52767c32d34596dbe7e5244948c110442a1cebf02a80fd39593`。当前draft24 delta累计相对该base，无需先装draft23。delta SHA256 `9abd7368a5f46169fe12230b3b810fae474290066521a23a54e4e37367794da8`。restore.py SHA256 `a9809cc84270b843089624a2617f690b1912ab4e61585fd263444061664fe051`。

下载这些文件后，在新目录运行索引中的恢复命令；不要覆盖当前工作目录或已有分支。历史权限与当前GitHub权限独立，其他参与者若不能读取用户的归档，由项目所有者提供归档文件。原恢复已校验1536个可达提交；这不等于这些原始提交已上传GitHub。
