import * as React from "react";

declare function f<T>(x: (v: T) => void): void;
declare class A<T> {
  constructor(cb: (v: T) => void): void;
}

f<string>((v) => { v as string }); // ok
new A<string>((v) => { v as string }); // ok
new A<any>(v => {}); // ok
