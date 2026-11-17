//@flow

type A = {[key: string]: number};
declare const a: A;
a as $Exact<A>;
