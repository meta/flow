//@flow

function f1<T extends string>(x: T) { return {[x]: x} } // error: Flow requires a return annotation on poly functions; f3 is the annotated case
function f2<T>(x: T) { return {[x]: x} } // error. mixed can't be used as a computed prop
function f3<T extends string>(x: T): {[key: T]: T} { return {[x]: x} } // ok: the key stays T
