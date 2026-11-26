/* @flow */

declare function invariant(condition?: unknown, message?: string): asserts condition; // raises

function foo1(c: boolean): string {
  const y = c ? 5 : invariant();
  return "default string";
}


function foo2(c: boolean): string {
  c ? 5 : invariant(false);
  return "default string";
}


function foo3(c: boolean): string {
  const y = c ? invariant() : invariant(false);
  return "default string"; // OK: reachable (invariant in ternary arm does not throw)
}


function foo4(c: boolean): string {
  const y = false ? 5 : invariant(false);
  return "default string";
}


function foo5(c: boolean): string {
  invariant()
  return "default string"; // OK: reachable (no-arg invariant() can return)
}


function foo6(c: boolean): string {
  invariant(false)
  return "default string"; // Error: unreachable
}

function foo7(c: boolean): string {
  invariant(c)
  return "default string";
}

function foo8(c: boolean): string {
  return c ? 'a' : invariant(); // Error: invariant must be a top-level statement call
}

function foo9(c: boolean): string {
  return c ? 1 : invariant(); // Error: invariant must be a top-level statement call
}


function foo10(c: boolean): string {
  return c ? invariant() : 'a'; // Error: cannot use a void result as a string
}

function foo11(): string {
  return invariant() ? 1 : 2; // Error: a void result cannot be tested for truthiness
}

// `||`
function foo12(c: boolean): string {
  c || invariant();
  return "default string";
}

function foo13(c: boolean): string {
  c || invariant(false);
  return "default string";
}

function foo14(c: boolean): string {
  invariant() || c; // Error: invariant must be a top-level statement call
  return "default string"; // OK: reachable (invariant in || operand does not throw)
}

function foo15(c: boolean): string {
  return c || invariant(); // Error: invariant must be a top-level statement call
}

function foo16(c: boolean): string {
  return invariant() || invariant(); // Error: invariant must be a top-level statement call
}

// `&&`
function foo17(c: boolean): string {
  c && invariant();
  return "default string";
}

function foo18(c: boolean): string {
  c && invariant(false);
  return "default string";
}

function foo19(c: boolean): string {
  invariant() && c; // Error: invariant must be a top-level statement call
  return "default string"; // OK: reachable (invariant in && operand does not throw)
}

function foo20(c: boolean): string {
  return c && invariant(); // Error: invariant must be a top-level statement call
}

function foo21(c: boolean): string {
  return invariant() && invariant(); // Error: invariant must be a top-level statement call
}

// `??`
function foo22(c: boolean): string {
  c ?? invariant();
  return "default string";
}

function foo23(c: boolean): string {
  c ?? invariant(false);
  return "default string";
}

function foo24(c: boolean): string {
  invariant() ?? c; // OK: `??` does not test the result for truthiness
  return "default string"; // OK: reachable (invariant in ?? operand does not throw)
}

function foo25(c: ?boolean): string {
  return c ?? invariant(); // Error: invariant must be a top-level statement call
}

function foo26(c: ?string): string {
  return c ?? invariant(); // Error: invariant must be a top-level statement call
}

function foo27(c: boolean): string {
  return invariant() && invariant(); // Error: invariant must be a top-level statement call
}
