// A generic function against a generic function type with as many type parameters: both bodies
// with the type parameters of the function in place of those of both, as in subtyping, so `R` is
// `ListSchema<C, M>` with the type parameters of the callback.
declare class Schema<C, M> {}
declare class ListSchema<C, M> extends Schema<Array<C>, Array<M>> {
  constructor(element: Schema<C, M>): void;
}
interface SchemaPromise {
  then<R>(f: <C, M>(schema: Schema<C, M>) => R | Promise<R>): Promise<R>;
}
declare const promise: SchemaPromise;

promise.then(<C, M>(schema: Schema<C, M>): ListSchema<C, M> => new ListSchema(schema)); // was an error

// With fewer type parameters, the function is instantiated against the body of the type.
promise.then(<C>(schema: Schema<C, string>): ListSchema<C, string> => new ListSchema(schema)); // error
