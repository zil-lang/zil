# 211 使用、同步与复现

## 同步到GitHub
本轮实际远端基线为 `6aa9b9b2336d5bb754f6c17cd9054da565c5c559`（210）。使用旁置 `zil-llvm-builder-211.bundle` 和原有通用 `SYNC-ZIL.ps1`：

```powershell
powershell -ExecutionPolicy Bypass -File .\SYNC-ZIL.ps1 -Bundle .\zil-llvm-builder-211.bundle
```

不需要源码ZIP、Python或Rust来执行同步。bundle只有一个交付分支，基线210是必要前置历史；已包含210的完整克隆可导入。脚本应重新读取远端，若已前进/分叉则按已有逻辑停止或创建审核分支，不得强推。本轮未推送GitHub。同步后以实际远端回读为准，不把本说明当上传收据。

## 构建和运行
源码ZIP根目录 `zil/` 为源码快照；`SOURCE-MANIFEST-211.json`提供逐文件SHA/模式。工具链不附在包中。开发宿主仍为Linux x64，Rust1.99.1、LLVM/Clang17.0.6，与210一致。

```sh
export ZIL_LLVM_LIB_DIR=/path/to/llvm17/lib
export LD_LIBRARY_PATH="$ZIL_LLVM_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export ZIL_CLANG=/path/to/llvm17/bin/clang
cargo build --offline --locked --release -p zil-cli
./target/release/zil build examples/mainline/zui_language177/main.zil \
  -o out/state --clang "$ZIL_CLANG" --opt O0 --codegen-units 8 --codegen-jobs 4
printf '3' | ./out/state
```

预期stdout `DEFHZMNOPK`、退出0；输入ASCII字符3，不是字节0x03。默认后端仍是直接LLVMBuilder，Clang仅作链接驱动。使用 `--backend clang` 可显式运行文本对照。单CGU仍为默认，主实验的8CGU/4工人设置需要显式指定；O2多CGU旧运行质量风险尚未解除。

## 复现新主实验
先独立构建210基线与211候选，将二进制固定保存在不同路径。每次使用全新输出目录，源树在prepare前必须已提交。

```sh
python3 tools/research/211/bench.py prepare --out /tmp/perf211-new \
  --baseline /path/to/zil210 --candidate /path/to/zil211 --clang "$ZIL_CLANG"
python3 tools/research/211/bench.py preflight --plan /tmp/perf211-new/plan.json \
  --out /tmp/perf211-new/preflight
python3 tools/research/211/bench.py measure --plan /tmp/perf211-new/plan.json \
  --preflight /tmp/perf211-new/preflight/result.json --out /tmp/perf211-new/measure --quiet-window
python3 tools/research/211/bench.py summarize --plan /tmp/perf211-new/plan.json \
  --records /tmp/perf211-new/measure/records.jsonl --out /tmp/perf211-new/summary.json
```

四步的PATH/LD_LIBRARY_PATH/ZIL_CLANG必须完全相同，不重复叠加工具链路径。不要与其它本地编译/测试重叠计时。准备时冻结工具二进制和夹具；变化必须开新计划，不覆盖旧失败。测量始终独立Verify，未变源码仍启动编译器，不是构建no-op。

完整语言、安全产品门、非作者审核和远端CI都未因本轮局部结果升级。外部参考见REFERENCES.md，正负数据与分析假设修正见RESULTS.md。完整初始辅助脚本/计划/失败日志位于旁置证据，后来的注释限定审计是明确事后分析。
