function *stmt_yield(): Generator<number, void, void> {
  yield 0; // ok
  yield ""; // error: string ~> number
}

function *stmt_next(): Generator<void, void, number> {
  var a = yield;
  if (a) {
    a as number; // ok
  }

  var b = yield;
  if (b) {
    b as string; // error: number ~> string
  }
}

function *stmt_return_ok(): Generator<void, number, void> {
  return 0; // ok
}

function *stmt_return_err(): Generator<void, number, void> {
  return ""; // error: string ~> number
}

function *infer_stmt() {
  var x: boolean = yield 0; // nope
  return "";
}
for (var x of infer_stmt()) { x as string } // error: number ~> string
var infer_stmt_next = infer_stmt().next(0).value; // error: number ~> void
if (typeof infer_stmt_next === "undefined") {
} else if (typeof infer_stmt_next === "number") {
} else {
  infer_stmt_next as boolean // error: string ~> boolean
}

function *widen_next() : Generator<number, void, number | boolean | string> {
  var x = yield 0;
  if (typeof x === "number") {
  } else if (typeof x === "boolean") {
  } else {
    x as string // ok, sherlock
  }
}
widen_next().next(0)
widen_next().next("")
widen_next().next(true)

function *widen_yield(): Generator<number | string | boolean, void, void> {
  yield 0;
  yield "";
  yield true;
}
for (var x1 of widen_yield()) {
  if (typeof x1 === "number") {
  } else if (typeof x1 === "boolean") {
  } else {
    x1 as string // ok, sherlock
  }
}

function *delegate_next_generator() {
  function *inner() {
    var x: number = yield; // error: void ~> number
  }
  yield *inner();
}
delegate_next_generator().next(""); // err string -> void

function *delegate_yield_generator() {
  function *inner() {
    yield "";
  }

  yield *inner();
}
for (var x2 of delegate_yield_generator()) {
  x2 as number // error: string ~> number
}

function *delegate_return_generator() {
  function *inner() {
    return "";
  }

  var x: number = yield *inner(); // error: string ~> number
}

// only generators can make use of a value passed to next
function *delegate_next_iterable(xs: Array<number>) {
  yield *xs;
}
delegate_next_iterable([]).next(""); // error: Iterator has no next value

function *delegate_yield_iterable(xs: Array<number>) {
  yield *xs;
}
for (const x of delegate_yield_iterable([])) {
  x as string // error: number ~> string
}

function *delegate_return_iterable(xs: Array<number>) {
  var x: void = yield *xs // ok: Iterator has no yield value
}

function *generic_yield<Y>(y: Y): Generator<Y,void,void> {
  yield y;
}

function *generic_return<R>(r: R): Generator<void,R,void> {
  return r;
}

function *generic_next<N>(): Generator<void,N,N> {
  return yield undefined;
}

function *multiple_return(b?: boolean) {
  if (b) {
    return 0;
  } else {
    return "foo";
  }
}
let multiple_return_result = multiple_return().next();
if (multiple_return_result.done) {
  multiple_return_result.value as void; // error: number|string ~> void
}

type YieldResult = {readonly type: "PENDING"} | {readonly type: "DONE", value: number};

function *yield_hint(): Generator<YieldResult, void, void> {
  yield {type: "PENDING"}; // ok
  yield {type: "DONE", value: 1}; // ok
  yield {type: "OTHER"}; // error: "OTHER" ~> YieldResult
}

function *yield_hint_literal(): Generator<"a" | "b", void, void> {
  yield "a"; // ok
}

function *yield_hint_function(): Generator<(x: number) => void, void, void> {
  yield (x) => { x as number; }; // ok
}

async function *async_yield_hint(): AsyncGenerator<YieldResult, void, void> {
  yield {type: "PENDING"}; // ok
}

function *return_hint(): Generator<YieldResult, YieldResult, void> {
  return {type: "PENDING"}; // ok
}

async function *async_return_hint(): AsyncGenerator<YieldResult, "DONE", void> {
  return "DONE"; // ok
}

function *delegate_hint(): Generator<YieldResult, void, void> {
  yield* [{type: "PENDING"}]; // ok
  yield* [{type: "OTHER"}]; // error: "OTHER" ~> YieldResult
}

function *delegate_hint_function(): Generator<(x: number) => void, void, void> {
  yield* [(x) => { x as number; }]; // ok
}

function *delegate_inner<T>(x: T): Generator<T, void, void> {
  yield x;
}

function *delegate_hint_call(): Generator<YieldResult, void, void> {
  yield* delegate_inner({type: "PENDING"}); // ok
}

async function *async_delegate_inner<T>(x: T): AsyncGenerator<T, void, void> {
  yield x;
}

async function *async_delegate_hint(): AsyncGenerator<YieldResult, void, void> {
  yield* [{type: "PENDING"}]; // ok
  yield* async_delegate_inner({type: "PENDING"}); // ok
}

declare function run_generator<A>(f: () => Generator<unknown, A, void>): A;
declare function delegate_to<T>(x: T): Generator<unknown, T, void>;
declare function identity<T>(x: T): T;

function unannotated_generator(text: string) {
  return run_generator(function *() {
    return yield* delegate_to(identity(text)); // ok
  });
}
