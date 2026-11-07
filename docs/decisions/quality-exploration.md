# 质量探索工具与语言验证的分工

Roam 是独立开发工具，不进入语言关键词、类型系统、运行时或生产 Cargo 依赖。`zil qc` 提供质量入口；有限状态路径记录具体动作、seed、property、原始失败、缩减与冻结工具身份。相同源码可能来自不同编辑/取消/缓存历史，不能仅用源码作通用状态键；缩减不能宣称全局最短反例。invalid、error、unsupported、incomplete 和预算耗尽不能计成功。

独立语义模型负责程序含义，状态探索负责历史透明性；二者都不替代独立 Verify。持续探索只有在已登记的有限 profile、停止条件与资源边界内运行；没有后台服务或新领域的默认授权。发现归属必须据真实首发现记录，受控注错和独审发现不能冒称自然 hunt 成果。

用户提供的 TypeScript 原件逐文件保留并校验摘要；现无公开发布许可，不发布至 npm/crates.io。工具协议见 `tools/roam-rs/`、`tools/roam/`，已实施范围见实际测试和 `docs/ROBUSTNESS.md`。历史来源：`.agent/roam-boundary.md`、`.agent/design-and-quality.md`。
