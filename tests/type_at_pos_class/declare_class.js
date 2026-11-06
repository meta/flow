// @flow

declare class C {}
//            ^
var c = { C };
//  ^

declare class P<X> {}
//            ^
var p = { P };
//  ^
