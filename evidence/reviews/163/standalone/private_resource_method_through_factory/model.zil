pub resource R{n:i32}impl R(self:Self){fn hidden()->i32=>.n}pub fn make()->R contract{result(create)}{return R{n:42}}
