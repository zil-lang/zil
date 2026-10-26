# draft18 Win64 兼容执行环境

本次环境是 Linux x86_64，实际执行的对象为 ZIL→LLVM→COFF→PE 生成的 Win64 程序。Wine 9.0（Ubuntu `9.0~repack-4build3`）在独立 Xvfb `:187` 显示上运行；xdotool 发送真实 XTest 键鼠输入，ImageMagick 捕获实际窗口，Pillow 读取像素。它不等同原生 Windows、真实显卡驱动、桌面集成或跨平台验收。

编译器使用 Rust 1.99.1 和 Ubuntu LLVM apt 的 clang/lld 23.1.5 开发快照；后者不能称为上游稳定版。原始 `--version`、进程参数、二进制和依赖摘要随执行收据保存。

## 已做的准备

系统未执行 apt install。Ubuntu noble、noble-updates、noble-security 的 main/universe 索引在项目内下载，31 个软件包和两个字体包按原 deb 字节保存，使用 `dpkg-deb -x` 解到项目目录。包名、版本、字节数和 SHA256 在 `packages.json` 与 `font-packages.json` 中；三个候选 Wine prefix、启动错误、补齐依赖和最终执行分别保留原日志。

本地搬移依赖还包括：

- Wine 的 Windows zlib1.dll 来自 `libz-mingw-w64`，复制到本地 builtin DLL 目录及最终 prefix 的 system32。
- `fonts-wine` 与 `fonts-wqy-zenhei` 的字体复制到该 prefix 的 Windows/Fonts。首窗显式选择 WenQuanYi Zen Hei，避免把缺字框当中文通过。
- 本地 Debian `wineserver` shell wrapper 的原件保存为 `wineserver.ubuntu-original`；本地 wrapper 改为运行同目录实际 wineserver64 并使用 `-p0`，没有修改 Wine 二进制。
- Xvfb 发行包的 xkbcomp 查找位置固定为 `/usr/bin/xkbcomp`。本机该位置原先不存在，创建了指向本地已校验 xkbcomp 的符号链接。这是本轮唯一的系统路径调整；在已有 xkbcomp 的机器不需要这个步骤，不能覆盖已有程序。

实际 prefix 是 `wine-prefix-003`；每个 prefix 都是本任务创建，不借用用户 Windows 环境。Wine/X11 的本地 Unix socket 在普通 sandbox 中被禁止，执行验证使用获准的本地 IPC 扩权。没有外部 UI 登录、网络发布或消息发送。

## 重建边界

审计附件的独立环境包保留原 deb、导入库、索引配置、版本摘要、必要日志和此说明。它不打包完整 prefix、系统映像或可重建的 Cargo/LLVM 中间对象，也不声称可在任意 Linux 发行版无依赖运行。

在 Ubuntu 24.04 兼容主机可以安装对应包，或按原 deb 解到新的本地目录；建立独立 prefix 并运行 wineboot，设置 WINESERVER/WINELOADER/WINEDLLPATH/WINEDATADIR，补齐上述 zlib 和字体。启动仅监听本地的 Xvfb（本轮 1024×768×24，`-nolisten tcp -noreset -ac`，使用包中 misc 字体），然后设置 DISPLAY。原 `wine-env.sh` 保存的是历史绝对路径，迁移时必须指向新的位置，不把旧路径当可用依赖。

导入库可由当前 `imports.py` 与同版 llvm-dlltool 重新建立。正式命令是：

```bash
python3 tools/validate-draft.py \
  --win64-imports /absolute/path/imports \
  --wine /absolute/path/wine64
```

也可显式设置 `ZIL_WIN64_IMPORTS` 与 `ZIL_WINE`，并为单次运行准备 PATH、LD_LIBRARY_PATH、WINEPREFIX 和 DISPLAY。缺少兼容执行环境会使新增平台门明确失败，不静默跳过。原生 Windows 验证是下一独立平台任务，不能用此环境包替代。
