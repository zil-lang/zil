fn id<T:Copy>(x:T)->T=>x;fn main()->i32{#for(n in [2]){let a=id::<[i32;n]>([4,8]);return a[1]-8}}
