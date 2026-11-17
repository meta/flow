type Valid1<out T> = Readonly<{foo: T}>; // ok
type Valid2<in T> = Readonly<{foo: (x: T) => void}>; // ok
type Invalid1<out T> = Readonly<{foo: (x: T) => void}>; // error
type Invalid2<in T> = Readonly<{foo: T}>; // error

type TriggersEval<out T extends {...}> = Readonly<{...T}>; // no error: the generic EvalT is stuck
