# 028 普通类型参数实例化

依据现有普通泛型规则，先实现同模块、无额外 trait/生命周期/const bound 的参数化函数与普通 struct。实例化是有界、卫生的未核 AST 展开，仍完整进入类型、候选与独立核验。相同模板和结构类型实参只产生一个实例；生成名不在源码标识符字母表内。所有类型替换作用于树，保留源码 span。

未实施的 trait/where/生命周期/const 约束不得丢弃；参数化正文在实例化前拒绝依赖未知类型的运算或成员操作，不能借某个实例偷偷引入未声明的约束。具名值的类型参与直接调用推导，缺少足够信息时要求既有显式 `::<...>`，不猜泛型类型。

跨模块泛型定义、泛型 impl 和一般 trait 解算继续独立施工。这里没有改变规范，也没有把模板展开本身当作已核代码。

## Review repair (028-R1 through 028-R4)

The first exact candidate was returned with five executable probes: dependent generic field defaults gained constraints from an instance; unused template bodies ignored selected work/depth limits; an invalid concrete boolean addition was erased; and an ordinary/generic function name collision disappeared during specialization. The repair keeps the original NeedsFix record and changes authorship: `/root/aggregates` implements these fixes and another agent must independently review the new exact candidate.

Generic field defaults are conservatively Unsupported until symbolic default checking is implemented. Original declarations are checked for collisions before templates are removed. A private borrowed AST traversal reserves caller-selected work/depth before recursive cloning, substitution or type-key construction; parameter checking and expansion share the same budget. Concrete unary/binary operations in template bodies must have admitted operand types even when unused. These are fail-closed repairs, not new generic bounds or capabilities.

Repair verification: all five original failing probes now pass as source-level regressions. Three additional tests cover the depth of the substituted type tree (both input trees individually fit), selected analysis work, unused lifetime signatures and concrete unary/binary type errors. The selected driver/expand test command passed 144 tests, including all 8 repair tests, 6 original driver generic tests, 2 original source/Core generic tests and the previous regressions selected by Cargo's `--tests` flag. Two generic programs still execute at O0/O2. Targeted clippy with warnings denied passed.

Rust goal reflection remains unchanged in its claims: this repair closes real generic-definition and compiler-budget defects, and keeps specialization behind independent verification. It does not establish full Rust safety, runtime/memory parity, simpler complete-task maintenance, lower compilation cost or a measured breakthrough. No paired performance result is claimed.
