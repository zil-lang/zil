# 336–339 组合源

`component.zil`保留第一次组合输入：fnptr的枚举返回值绑定没有显式类型，当前在Expand以E-GENERIC-ARGUMENT（record enum pattern type cannot be inferred）拒绝。这是待补的结果推断，不是永久禁止，也不计为通过。

`component-explicit.zil`仅给该结果绑定补`:Message`，贯通默认trait返回fnptr、大Copy记录式枚举的间接调用/返回、mutable life和子线程共享实际局部地址。main内部核对child=43、parent=42、join后value=44，成功返回0。两后端O0/O2与真实API多个CGU由replay.py验证。

root初次脚本把默认main误当成打印值的显式入口，后又对单函数CT源强求多CGU而拒绝合法的one-linkage-group提示。最终runner显式`--entry main`，组合源请求4CGU、两个CT控制请求1CGU；实际backend、API builder、pipeline及fallback均核对。这些脚本失败不作为编译器或语言失败，精确记录见本批集成JSON。
