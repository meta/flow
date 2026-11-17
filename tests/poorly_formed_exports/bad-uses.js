// @flow

exports.foo; // ERROR
module.exports.foo; // ERROR

// These functions are provided just to illustrate the danger of aliasing `module` and
// `module.exports`.
function addsAProp(x) {
  x.foo = '42';
}

function addsAPropToExports(x) {
  x.exports.bar = '42';
}

addsAProp(module.exports); // ERROR
addsAProp(module['exports']); // ERROR
addsAProp(exports); // ERROR
addsAPropToExports(module); // ERROR

function h() {
  exports.foo; // ERROR
  module.exports.foo;
  exports['foo'] = 42; // ERROR
  exports.foo = 5; // ERROR
}

module.exports['foo'] = 42; // ERROR
module['exports'] = {}; // ERROR
exports['foo'] = 42; // ERROR

(5, exports).foo = 1; // ERROR
(5, module).exports.foo = 34; // ERROR

function g() {
  // We don't know when this function may be called, clobbering `module` and `exports`, so we can't
  // allow this.
  module = {}; // ERROR
  exports = {}; // ERROR
}

function f() {
  let module: {[key: string]: {[key: string]: unknown}} = {};
  let exports: {[key: string]: unknown} = {};
  // These should all be fine, since `module` and `exports` have been shadowed.
  module.exports = {} as {[key: string]: unknown}; // OK
  exports = {} as {[key: string]: unknown}; // OK
  exports.foo = 42; // OK
  module.exports.foo = 42; // OK
  addsAPropToExports(module); // OK
}

// This is a pattern recommended by Node, and shold be allowed.
if (require.main === module) { } // OK
// This doesn't need to be allowed, the above can be special-cased.
if (require.main === (5, module)) { } // ERROR

// This is okay since `module` is rebound. Previously it errored due to a (now fixed) bug in
// scope_builder.
switch ('') { case '': const module = ''; module; }; // OK

module.id; // OK
