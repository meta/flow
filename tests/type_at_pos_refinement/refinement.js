// @flow

declare const a: {
  foo?: () => void,
 ...};

if (a.foo != null) { a.foo; }
//                      ^
if (a.foo != null) { a.foo(); }
//                      ^
