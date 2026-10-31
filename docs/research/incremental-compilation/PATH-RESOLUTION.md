# 第四阶段：每次路径检查内复用共同父目录

从阶段3的5e0613cc继续。只对一次项目依赖计划构造、或一次ArtifactPlan::validate调用，记住最后一个成功解析的parent。原destination函数保留为miss路径；绝对词法parent保留`..`的路径语义，最终filename仍作为目录项拼接，不用输出symlink的目标替代发布位置。不能完整绝对化的Windows drive-relative路径保守回原函数。

缓存只持有一个父目录，额外状态不随项目模块数增长；在下一次检查前销毁，不存入Session或ArtifactPlan。每个文件依然读取当前metadata，现存文件的canonicalize/inode、dangling symlink、源间别名、输出冲突及提交前验证都保留。它没有提供原系统未承诺的原子文件系统快照，外部并发修改的既有边界不扩大为新的保证。

同一序列将缓存解析器与未改的原解析器逐项比较，覆盖普通/缺失/非目录/相对/`..`/目录symlink/dangling路径。对同一个ArtifactPlan在检查间更改hardlink、最终symlink和父目录类型，仍必须拒绝当前冲突；既有Source保护、逐阶段取消和发布回滚控制继续运行。Linux限定495测试、all-targets Clippy及格式检查通过；native Windows仍未测。

本阶段完整计量固定10个交错区组（此前5个），并在所有比较臂使用同源probe增加Linux进程CPU时间诊断。该CPU值包含编译器线程，不包含子链接器/原生控制进程，不能称为完整构建树CPU；墙钟主指标及native控制位置不变。不根据这些诊断选择性删除不利样本。

本阶段生产实现9fe5f55e已完成独立审核Pass（696组destination及684组身份/诊断差分）。两套10区组各3120请求完整窗口通过：main直接对照512模块暖check 106.917→13.443 ms，私有修改107.113→94.652 ms；从阶段3隔离为23.154→13.896 ms、105.896→97.618 ms。主/隔离仍分别有5/4个墙钟不利单元，全量见[LATEST-MEASUREMENTS.md](LATEST-MEASUREMENTS.md)，总报告见[CURRENT.md](CURRENT.md)。阶段2/3窗口保留原身份，不相乘。
