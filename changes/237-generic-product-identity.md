# 237 泛型Product完整身份

原235非作者Success用例发现：直接callback Store与无callback的泛型Root分别可用，Bag<T>{Store<T>}、Root<T>{Bag<T>}及wrap<T:StoreElement>组合却失败。仅转发词法bound会进入Types全局聚合解析，仍缺caller owner；按名字接受会跨binder授信，替换字段不重建family身份又会错配direct与retained实例。原失败不得改为预期Unsupported。

按规范§8.1.4冻结两张有限Core表和recipe、精确owner、原family及完整实参。复用202完整实例键，保留phantom，不借State callback字段metadata伪造普通参数归属。IR/Expand/CT生产传播与Types独立前置/具体化并行；公共字段与版本由root冻结，默认预算及产品门不变。

最近反例：不同函数同拼写T/F、错误owner、混合或遗漏参数、相同布局不同名义/phantom、未用非法表、递归和超额元数据。真实输入经过Types/Analyze/独立Verify和O0/O2；原235限定Pass不等于237完成。
