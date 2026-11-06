import api

pub struct ArrayCursor<T:Copy>{values:[T;2];index:usize}

pub fn make<T:Copy>(first:T,second:T)->ArrayCursor<T>{
    return ArrayCursor::<T>{values:[first,second],index:0usize}
}

impl<T:Copy> api.LendingIterator for ArrayCursor<T>{
    type Item=T;
    fn next<life p>(self:&'p mut Self)->Option<&'p T>{
        if self.index>=2usize{return Option::<&'p T>.None()}
        let i=self.index;
        self.index+=1usize;
        return Option::<&'p T>.Some(&self.values[i]);
    }
}
