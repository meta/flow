// A lower bound checked against a repositioned application of a mapped type, here the property
// type `P<T>['value']` of `Pick`, still gives `T` a lower bound, even when `T` already has the
// lower bound `empty` that `null ~> ?T` adds.
type P<T> = {selectedValue: ?T, value: T};
declare function f<T>(p: Pick<P<T>, keyof P<T>>): T;
const r = f({selectedValue: null, value: 'a'});
r as string; // ok
r as empty; // error: string ~> empty
