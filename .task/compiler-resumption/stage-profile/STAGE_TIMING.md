# 临时真实 CLI 阶段计时

当前入口是 `stage-timing.patch`；之前的公开 Session API probe 草稿不采用，也无需编译。
补丁基于 resource 的 tree `1c16744d838064cb1c687ba56ef3a0d17ed3130c`，与当前 main99035931 同树。
生成器只读取源码，在内存生成补丁；原工作树没有被修改。准备时已通过 `git apply --check`。root 后续已在独立诊断工作树 `/workspace/scratch/abf690ce880c/profile` 和独立 `target-profile` 构建并跑完第一份三请求诊断；原始结果位于 `shared4096-first/`，汇总位于 `shared4096-first-analysis.txt`。

在 root 分配的新诊断工作树里应用，沿当前已加载工具链环境构建；只使用独立 target：

```bash
git apply --check /workspace/scratch/abf690ce880c/out/profile/stage-timing.patch
git apply /workspace/scratch/abf690ce880c/out/profile/stage-timing.patch
CARGO_TARGET_DIR=/workspace/scratch/abf690ce880c/out/profile/target-stage-timing cargo build --locked --offline --release -p zil-cli
```

固定三个新 CLI 进程，依次 cold/base、warm/base、private/private；不改原语料字节，warm 不重写同字节源。
保留原 LLVM API、O2、8CGU/2jobs、语义缓存、对象缓存、incremental-o2、after 配置。
新编译器身份要求新缓存目录；runner 拒绝覆盖输出目录。

```bash
python3 /workspace/scratch/abf690ce880c/out/profile/run_stage_profile.py \
  --cli /workspace/scratch/abf690ce880c/out/profile/target-stage-timing/release/zil \
  --base-source /workspace/scratch/beabdcf5ebf4/evidence/resource-acceptance/primary/corpus/shared4096/base/zil/main.zil \
  --private-source /workspace/scratch/beabdcf5ebf4/evidence/resource-acceptance/primary/corpus/shared4096/private/zil/main.zil \
  --clang "$ZIL_CLANG" \
  --cpu 0,1 \
  --out /workspace/scratch/abf690ce880c/out/profile/shared4096-stage-run1
python3 /workspace/scratch/abf690ce880c/out/profile/read_stage_profile.py \
  /workspace/scratch/abf690ce880c/out/profile/shared4096-stage-run1
```

runner 自动设置 `ZIL_PROFILE_STAGE_TIMING=1`；stderr 增加 JSONL `zil_profile:1`，stdout 原报告不变。
保留 argv、CLI/clang/source/ELF/sidecar 哈希、实际单位/命中、当前 Verify=1 和每请求完整 stderr/stdout。
runner 现使用 `copy2` 保存 ELF 与 sidecar，保留可执行权限。首轮旧版本曾用 `copyfile` 导致取证 ELF 副本丢失 `+x`；这影响副本后续执行，未改变编译成功、字节或原哈希。旧 raw 保持不变，由 root 单独修复旧取证副本权限并核对字节和输出；本修正不重新执行测量。
可把 `--cli` 换成未加计时的当前同树 CLI，在另一个唯一新目录执行同一三请求历史，逐快照核对完整 ELF。
这只是定位实验，各请求一次，不构成性能收益或运行质量验收，也不覆盖原失败。

所有 span 使用同一个单调 epoch，附 pid/thread/start/end。主线程 span 是包含子区间的 wall；reader 的 uncovered 仅减去**同线程内部 span 的区间并集**。
`backend.object_workers.wall` 是工人开始到全部 join 的真实整体 wall；逐对象时长与 object_reports 内部 optimize/emit 等总和单列，不能从完整请求 wall 相减。
`driver.query.*` 包含数据库与阶段发布工作；`driver.discovery.parse` 是文件发现时的真实解析；observer 是阶段完成事件，不宣称其间隔为独占 phase 时间。
额外全模型 preflight 标为 `backend.transform.full_preflight`，命中对象的当前 preflight 单列；二者均保留原完整核验。
`driver.filesystem_discovery` 包含真实文件读取和可选已解码缓存恢复，`driver.semantic_cache.restore` 单列实际 Store.load。
计时日志自身有少量开销，因此应先找数量级大头；不要把几微秒差异解读为优化成果。
