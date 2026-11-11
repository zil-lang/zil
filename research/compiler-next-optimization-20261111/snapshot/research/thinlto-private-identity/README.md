# 私有 ThinLTO 的请求内身份核验去重

## 为什么

私有 postlink 候选 `1cc866a08bac100012e41726067711c94caf1899` 的两个 pure base case 已通过完整 6 次编译／54 次运行质量对照。原驱动每个请求的八次 unit wrapper 和一次 link wrapper 都完整核验工具镜像，逻辑读取至少 9.279 GB；原完整请求 8.286／14.631 秒仍真实保留，不据运行质量通过声称编译提速。

本对照只去除同一次请求内的重复工具身份核验。首次完整核验仍发生在 B 的实际编译计时内部，仍保留原实现对 clang 与构建 compiler 别名的重复完整读取。没有从计时结果减掉任何开销，也没有把首次核验移到时钟边界之外。

## 实现

同一新建请求目录里的各 wrapper 通过 Linux flock 串行完成一次原 validate。只有全部成功且文件读前后精确 stat 一致，才发布绑定当前 protocol SHA、当前 Rust receipt 路径和 SHA 的身份记录。允许登记的对象仅为固定九种 tool role 与插件构建 compiler 镜像；后续 wrapper 仍执行原 validate，仅这组固定镜像的 SHA 读取在精确 inode、mode、size、mtime、ctime、symlink/realpath 一致时复用。每次实际工具前后再次核 stat，变化或错请求直接失败。

Protocol、脚本、构建记录、当前 Verify/机器证明及动态 IR／BC／最终 ELF 不进入该身份缓存。全部当前独立 Verify、原始／分区／八 unit 机器预检、32 次外部 LLVM verify、实际 prelink/.3 可见性、原成本上界、八 hook 和前后全文证明保持原样。正式 C++ 插件及二进制直接复用已通过的 a22／585a，没有再改内联策略或函数体。每次 LLVM native cache 仍全新。

新 runner 必须使用不存在的输出目录，每轮/每 arm 也独占新目录，B 启动前 wrapper 目录必须不存在。`validate_request` 单独面对任意既存目录不是通用缓存准入接口；该限定不可省略。

## 固定筛选

`compile_screen.py` 在 N64、N256 的同字节 base 语料上执行原六个 ABR 排列，共 36 次完整冷编译。A 是 main990 LLVM API，B 是复用2549导出器和1cc私有插件的新驱动，R 是固定Rust1.99.1；O2、overflow、8CGU、两CPU/两任务保持原规则。每个实际产物执行 00／ff／41，合计 108 个原生进程，其中 36 个 41 输入样本供原运行门 B/A≤1.10、Rust/B≥0.90 再核。

逐 case 报告完整编译 wall 与 CPU、运行质量、所有原始样本和 ELF 身份；不删除异常或长尾，不合成加权总体通过。这个有界 cold screen 尚未执行，没有 many-edit、并发、warm cache 或生产采用结论。首次失败与修复后运行身份分别保留；后续是否扩展由实际净成本决定。
