declare var arr: Array<{data: ?{[key: string]: string}}>;

// The label error does not keep the annotation from typing `item`.
function f(): Array<{[string]: string}> { // error
  return arr.map(item => item.data ?? {}); // ok
}
