function block_scope() {
  let a: number = 0;
  var b: number = 0;
  {
    let a = ""; // ok: local to block
    var b = ""; // error: banned redeclaration
  }
}

function switch_scope(x: string) {
  let a: number = 0;
  var b: number = 0;
  switch (x) {
    case "foo":
      let a = ""; // ok: local to switch
      var b = ""; // error: banned redeclaration
      break;
    case "bar":
      let a = ""; // error: a already bound in switch
      break;
  }
}

// a switch is a single lexical scope, so lets and non-disjoint
// cases can mix in odd ways. our support for edge cases is not
// yet perfect.
function switch_scope2(x: number) {
  switch (x) {
    case 0:
     a = "";     // error: assign before declaration
     break;
    case 1:
     var b = a;  // error: use before declaration
     break;
    case 2:
      let a = "";
      break;
    case 3:
      a = "";     // error: skipped initializer
      break;
    case 4:
      var c:string = a;  // error: skipped initializer
      break;
  }
  a = ""; // error: a no longer in scope
}

function try_scope() {
  let a: number = 0;
  try {
    let a = ""; // ok
  } catch (e) {
    let a = ""; // ok
  } finally {
    let a = ""; // ok
  }
}

function for_scope_let() {
  let a: number = 0;
  for (let a = "" /* ok: local to init */;;) {}
}

function for_scope_var() {
  var a: number = 0;
  for (var a = "" /* error: banned redeclaration */ ;;) {}
}

function for_in_scope_let(o: any) {
  let a: number = 0;
  for (let a /* ok: local to init */ in o) {}
}

function for_in_scope_var(o: any) {
  var a: number = 0;
  for (var a /* error: banned redeclaration */ in o) {}
}

function for_of_scope_let(xs: string[]) {
  let a: number = 0;
  for (let a /* ok: local to init */ of xs) {}
}

function for_of_scope_var(xs: string[]) {
  var a: number = 0;
  for (var a /* error: banned redeclaration */ of xs) {}
}

function default_param_1() {
  // function binding in scope in default expr
  function f(
    x: () => string = f // error: number ~> string
  ): number {
    return 0;
  }
}

function default_param_2() {
  // fn body bindings not visible from param scope
  let a = "";
  function f0(x: () => string = () => a): number {
    let a = 0;
    return x(); // error: string ~> number
  }
  function f1(x: number = b /* error: cannot resolve b */): number {
    let b = 0;
    return x;
  }
}

function function_type_annotation_scope() {
  const x: string = "";
  const y: number = 0;
  const z: boolean = true;
  // Function parameter annotations are in the scope of the function.
  function f(x: number, y: boolean, z: string, a: typeof x, b: typeof y, c: typeof z) {
    a as number; // OK
    a as string; // error a ~> number
    b as number; // error boolean ~> number
    c as boolean; // error string ~> boolean
  }

  function invalid_self_reference(
    selfRef: typeof selfRef // Error cannot resolve selfRef
  ) {}

  function invalid_forward_reference_typeof(
    x: typeof forwardRef, // Error: cannot resolve forwardRef
    forwardRef: number,
  ) {}

  function annotations_in_default(
    x: number,
    y: typeof x,
    f: (n: number) => void = (z: typeof x) => { z as number }
  ) {
    y as string; // error number ~> string
  }
}

function unannotated_param_as_type(x) { // error missing-local-annot
  function foo(
    x: x, // error value-as-type
    y: x, // ideally an error, but 'x' is already an 'AnyT Error' type
  ) {}
}

function empty_var_as_type() {
  declare var e: empty;
  function foo(x: e) {} // error value-as-type
}

function redeclare_param_with_default_1(
  x: number,
  y: number = 1,
) {
  let x = 1; // error redeclaration
}

function redeclare_param_with_default_2(
  x: number = 1,
) {
  let x = 1; // error redeclaration
}

function redeclare_param_with_default_3(
  x: number,
  y: number = 1,
) {
  var x = 1; // okay
}

function refinement_in_func_with_default_regression()  {
  function foo() {}

  function bar(
    x: ?string,
    param_with_default: number = 1,
  ): void {
    if (x == null) {
      x = "";
    };
    foo();
    x as string; // okay
  }
}

function refinement_in_default(
  x: null | number,
  y: number = (x = 1, 1),
) {
  x as number; // error null ~> number
}

function refinement_in_nested_default(
  x: null | number,
  { y = (x = 1, 1) }: { y: number },
) {
  x as number; // error null ~> number
}
