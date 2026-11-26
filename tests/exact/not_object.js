//@flow

let x: {} = 3;
let y: {} = '';
let z: {} = true;

let c: {} = new (class {})();
let f: {} = () => {};

declare const u: {} | {...};
u as {};

declare const v: number | {...};
v as {};

class A {}

0 as $Exact<number>;
'' as $Exact<string>;
true as $Exact<boolean>;
new A() as $Exact<A>;
((): undefined => {}) as $Exact<() => {}>;

let foo = (): undefined => {};
((): undefined => {}) as $Exact<typeof foo>;
let bar = new A();
new A() as $Exact<typeof bar>;
