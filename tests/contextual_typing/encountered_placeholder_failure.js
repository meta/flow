// This test suite asserts that we will emit missing-local-annot,
// when there are placeholders in the final solution.

declare function id<T>(x: T): T;
declare const str: string;

// Decomp_FuncParam
id((item) => 1); // error
// Decomp_FuncRest
id((...item) => 1); // error
// Decomp_FuncReturn
id(() => (item) => 1); // error
// Decomp_Await
async () => id(await ((item) => 1)) // error
// Decomp_Promise
id(async () => (item) => 1); // error

// Decomp_ObjProp
id({foo: (item) => 1}); // error
// Decomp_ObjComputed
id({[str]: (item) => 1}); // error
// Decomp_ObjSpread
id({...{foo: (item) => 1}}); // error
// Decomp_ArrElement
id([(item) => 1]); // error
// Decomp_ArrSpread
id([...(item) => 1]); // error
// Decomp_SentinelRefinement
id({type: 'literal', foo: (item) => 1}) // error

// Nested
id(id((item) => 1)); // error

//
{
  // ResolvedAnySpreadArg
  declare function debounce<TArgs extends ReadonlyArray<unknown>>(f: (...args: TArgs) => void): (...args: TArgs) => void;
  debounce((foo) => {}); // error
}
