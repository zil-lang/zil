pub resource R{n:i32}impl R(self:Self){pub fn read()->i32=>.n;pub fn finish(give self:Self)->i32=>.n}pub fn make()->R contract{result(create)}{return R{n:42}}
