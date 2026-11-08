import protocol;pub fn next<life a,I:protocol.LendingIterator>(cursor:&'a mut I)->Option<&'a I.Item>{return cursor.next()}
