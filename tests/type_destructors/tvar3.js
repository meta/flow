// @flow

declare opaque type A;
declare opaque type B;
declare opaque type C;
declare const a: A;
declare const b: B;
declare const c: C;

declare function fn1<T>(x: T): T;
declare function fn2<T>(x: (t: T) => void): (t: T) => void;
declare function fn3<T>(x: T, y: (t: T) => void): T;
declare function fn4<T>(x: T, y: (t: T) => void): (t: T) => void;

declare function fn5<T>(x: T): NonNullable<T>;
declare function fn6<T>(x: (t: T) => void): (t: NonNullable<T>) => void;
declare function fn7<T>(x: T, y: (t: T) => void): NonNullable<T>;
declare function fn8<T>(x: T, y: (t: T) => void): (t: NonNullable<T>) => void;

fn1(a) as B; // Error
fn2((a: A) => {}) as (b: B) => void; // Error
fn3(a, (b: B) => {}) as C; // Error
fn4(a, (b: B) => {}) as (c: C) => void; // Error

fn5(a) as B; // Error
fn6((a: A) => {}) as (b: B) => void; // Error
fn7(a, (b: B) => {}) as C; // Error
fn8(a, (b: B) => {}) as (c: C) => void; // Error
