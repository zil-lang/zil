# 213 一次性发布与分支归档

本目录只准备了可审核代码；未对 GitHub 执行任何写入。最终源码 SHA 与 bundle SHA256 仍由集成方在独立审核和正式验收后填写。发布成功时，`main` 精确指向 manifest 的 `source.sha`；transport 提交与 archive guard 提交均不会成为生产 main 的祖先。

## 状态与验证

本地独立 bare remote 的 **15 / 15 协议测试通过**，原始日志为 `test-results.log`。覆盖只读 dry-run/prepare、main 快进、归档保真、main/旧 head/新增 head/tag 漂移、checksum、验证收据、NoGo 祖先、非快进拒绝，以及分支保护和 receive-pack 最后时刻并发更新导致整笔原子回滚。测试中的语言门收据是明确标注的模拟输入；这些测试只证明 Git 发布协议，不代表运行了语言全量验收。

`test-first-fixture-failure.log` 保留首轮 14 Pass / 1 fixture failure：并发模拟 hook 没有移除 quarantine 环境变量，导致模拟更新本身被 Git 拒绝。修正只影响测试 hook，随后原始 15 项全部通过。

临时 workflow 在发布前运行现有 `verify-main.yml` 的完整门：fmt、locked workspace tests、no-default-features workspace tests、release CLI + research/208 原生消费者、all-target Clippy。固定 checkout/upload-artifact Action SHA、Rust 1.99.1 与 LLVM 17 保持一致；debug=0、incremental=0、strip=symbols 用于控制 runner 磁盘用量。门收据绑定 source SHA、tree SHA、manifest/bundle SHA256 和每项日志 SHA256，发布前再次检查源码 checkout 没有改变。

使用仓库 `GITHUB_TOKEN` 的 push 不会再触发 main 的 push workflow，所以必须以本次发布 workflow 已执行的完整验证为依据。参见 [GitHub 官方说明](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow)。本方案只使用 checkout 配置的短期仓库凭据，不读取或输出凭据，也不申请额外 token。

## 填值与上传

1. 集成方冻结已经通过独审及正式门的完整 source SHA，并生成从旧远端 main `a0796270cfde23379d1b227f0a8158121d7c91d5` 到该 SHA 的增量 bundle。bundle 必须以 `refs/heads/task/213` 广告该精确 SHA；其历史依赖必须可由远端现有 heads 恢复。
2. 用 `finalize.py` 校核 bundle 广告和 SHA256，生成 manifest。示例：

   ```bash
   python3 finalize.py \
     --template manifest.template.json \
     --bundle source.bundle \
     --source-sha EXACT_REVIEWED_40_HEX_SOURCE_SHA \
     --out manifest.json
   ```

3. 仅把以下文件放入临时远端 `task/213` transport 提交：

   | 本目录文件 | 上传到仓库中的路径 |
   |---|---|
   | `publish.py` | `.publish213/publish.py` |
   | `verify.py` | `.publish213/verify.py` |
   | 生成的 `manifest.json` | `.publish213/manifest.json` |
   | 最终 `source.bundle` | `.publish213/source.bundle` |
   | `publish213.yml` | `.github/workflows/publish213.yml` |

   `finalize.py`、本地测试及本说明不需要上传。transport 的精确 SHA 由 workflow 的 `GITHUB_SHA` 绑定，因此 manifest 不包含循环自引用的 transport SHA。
4. transport 分支 push 触发一次性 workflow。workflow 先把脚本和 payload 复制到 runner 临时目录，再 checkout bundle 内的精确 source SHA；后续所有门及最终 main 更新始终使用此 SHA。严禁把 transport SHA 当成生产 source SHA。
5. 完成后核对 `published.json`、远端 main SHA、归档 tags 和 heads 列表。正式验证/发布收据及小型 bundle 作为 artifact 保留 30 天；不上传编译 target 缓存。

## 三个阶段与失败行为

`publish.py dry-run` 读取远端 heads/tags、检查 bundle checksum 和广告，输出计划，不修改本地 refs 或远端。尚未导入 source 对象时，明确把 ancestry 检查标为延后，不能把 dry-run 视为验收。

`publish.py prepare` 精确匹配完整 heads 快照（含临时 `task/213`），只抓取到本地私有 refs，核验 bundle，证明 main 可快进，验证 manifest 中每项 `merged` / `archive-only` 关系，重新读取远端快照，然后 checkout 精确 source 并记录准备收据。任何未知新增 head 或已知 SHA 漂移都会拒绝继续。该阶段仍不写远端。

`publish.py publish` 要求准备收据与六项完整门收据均匹配且成功。再次检查 source 与远端状态后，先以 absent lease 原子创建缺失的 canonical archive tags，保留已经精确匹配的 tags，然后重新验证所有 tags 与 heads。最终单笔 `git push --atomic` 同时执行 main 快进、逐 ref 精确 lease 删除旧 heads/临时 transport、以及创建 archive guard tag。服务端不支持 atomic、分支保护拒绝或任一 lease 失配时，没有非原子或强行覆盖的备用路径。

若 canonical tags 已创建而最终事务被拒绝，这些保真归档会留下；main 和待删除 heads 保持原状。检查漂移后，可以在相同预期仍成立时重试，不会覆盖已有 tag。

Git 不提供整个 ref 集合的全局 CAS，而且可能不发送已有 tag 的 no-op 更新。因此额外的 `archive/2026-10-29/heads-before-213` guard 在最终删除事务中创建，指向仅用于归档的独立提交；其父提交含全部旧 heads、transport 和 source，保证被删除历史仍有可达锚点。它不是生产合并；136 NoGo 历史只在归档图中保留。脚本最终再次重读所有 heads 与 canonical tags；极窄检查窗口中的外部 tag 变动会明确报错，guard 保留历史，绝不会静默覆盖该变动。并发新建的未知分支从不在删除 refspec 中。

归档命名与具体 SHA 以 `manifest.template.json` 为准；main 旧 tip、132、131 研究、136 NoGo 研究、旧 sync bundle、135、203、transport 各有独立 canonical tag。所有文件及原始测试日志的 SHA256 见 `REVIEWABLE.json`。
