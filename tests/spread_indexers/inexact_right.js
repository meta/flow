const s: {[key: string]: number} = {};
const t: {foo: number, ...} = {foo: 3};
const u = {...s, ...t}; // Error

declare class Base {}
declare class Child extends Base {
  foo: number;
}
declare const child: Child;
const w = {...s, ...child}; // Error
