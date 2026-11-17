declare const recordObj: {foo: string};
declare const dictObj: {[key: string]: string};
declare const protoDictObj: {__proto__: {[key: string]: string}};
declare const key: string;

recordObj[key] as string; // error: bad key
dictObj[key] as string; // error: void ~> string
dictObj.hahaha as string; // error: void ~> string
dictObj[key] = undefined; // error: void ~> string
dictObj.hahaha = undefined; // error: void ~> string
protoDictObj[key] as string; // error: cannot access, proto ignored
protoDictObj.hahaha as string; // todo: should error, but the flag doesn't apply to proto for now
protoDictObj[key] = undefined; // error: cannot write, proto ignored
protoDictObj.hahaha = undefined; // error: cannot write, proto ignored

declare export const typeTest: (typeof dictObj)[string];
typeTest as string; // ok: the flag should not affect type-level acccess
