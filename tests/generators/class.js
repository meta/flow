class GeneratorExamples {
  *stmt_yield(): Generator<number, void, void> {
    yield 0; // ok
    yield ""; // error: string ~> number
  }

  *stmt_next(): Generator<void, void, number> {
    var a = yield;
    if (a) {
      a as number; // ok
    }

    var b = yield;
    if (b) {
      b as string; // error: number ~> string
    }
  }

  *stmt_return_ok(): Generator<void, number, void> {
    return 0; // ok
  }

  *stmt_return_err(): Generator<void, number, void> {
    return ""; // error: string ~> number
  }

  *infer_stmt(): Generator<number, string, void> {
    var x: boolean = yield 0; // error: void ~> boolean
    return "";
  }

  *widen_next(): Generator<number, void, void> {
    var x = yield 0;
    if (typeof x === "number") {
    } else if (typeof x === "boolean") {
    } else {
      x as string // nope
    }
  }

  *widen_yield(): Generator<(number | string | boolean), void, void> {
    yield 0;
    yield "";
    yield true;
  }

  *delegate_next_generator(): Generator<unknown, undefined, string> {
    function *inner(): Generator<unknown, undefined, string> {
      var x: number = yield; // error: string ~> number
    }
    yield *inner();
  }

  *delegate_yield_generator(): Generator<string, void, void> {
    function *inner() {
      yield "";
    }

    yield *inner();
  }

  *delegate_return_generator(): Generator<void, void, void> {
    function *inner() {
      return "";
    }

    var x: number = yield *inner(); // error: string ~> number
  }

  // only generators can make use of a value passed to next
  *delegate_next_iterable(xs: Array<number>): Generator<number, void, string> {
    yield *xs;
  }

  *delegate_yield_iterable(xs: Array<number>): Generator<number, void, void> {
    yield *xs;
  }

  *delegate_return_iterable(xs: Array<number>): Generator<number, void, void>  {
    var x: void = yield *xs // ok: Iterator has no yield value
  }

  *generic_yield<Y>(y: Y): Generator<Y,void,void> {
    yield y;
  }

  *generic_return<R>(r: R): Generator<void,R,void> {
    return r;
  }

  *generic_next<N>(): Generator<void,N,N> {
    return yield undefined;
  }
}

var examples = new GeneratorExamples();

for (var x of examples.infer_stmt()) { x as string } // error: number ~> string

var infer_stmt_next = examples.infer_stmt().next(0).value; // error: number ~> void
if (typeof infer_stmt_next === "undefined") {
} else if (typeof infer_stmt_next === "number") {
} else {
  infer_stmt_next as boolean // error: string ~> boolean
}

examples.widen_next().next(0) // err number -> void
examples.widen_next().next("") // err number -> void
examples.widen_next().next(true) // err number -> void

for (var x0 of examples.widen_yield()) {
  if (typeof x0 === "number") {
  } else if (typeof x0 === "boolean") {
  } else {
    x0 as string // ok, sherlock
  }
}

examples.delegate_next_generator().next("");

for (var x1 of examples.delegate_yield_generator()) {
  x1 as number // error: string ~> number
}

examples.delegate_next_iterable([]).next(""); // error: Iterator has no next value

for (var x2 of examples.delegate_yield_iterable([])) {
  x2 as string // error: number ~> string
}
