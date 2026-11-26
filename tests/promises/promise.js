/**
 * @flow
 */

//////////////////////////////////////////////////
// == Promise constructor resolve() function == //
//////////////////////////////////////////////////

// Promise constructor resolve(T) -> then(T)
new Promise<number>(function(resolve, reject) {
  resolve(0);
}).then(function(num) {
  var a: number = num;

  // TODO: The error message that results from this is almost useless
  var b: string = num; // Error: number ~> string
});

// Promise constructor with arrow function resolve(T) -> then(T)
new Promise<number>((resolve, reject) => resolve(0))
  .then(function(num) {
    var a: number = num;

    // TODO: The error message that results from this is almost useless
    var b: string = num; // Error: number ~> string
  });

// Promise constructor resolve(Promise<T>) -> then(T)
new Promise<number>(function(resolve, reject) {
  resolve(new Promise(function(resolve, reject) {
    resolve(0);
  }));
}).then(function(num) {
  var a: number = num;
  var b: string = num; // Error: number ~> string
});

// Promise constructor resolve(Promise<Promise<T>>) -> then(T)
new Promise<number>(function(resolve, reject) {
  resolve(new Promise(function(resolve, reject) {
    resolve(new Promise(function(resolve, reject) {
      resolve(0);
    }));
  }));
}).then(function(num) {
  var a: number = num;
  var b: string = num; // Error: number ~> string
});

// Promise constructor resolve(T); resolve(U); -> then(T|U)
new Promise<number | string>(function(resolve, reject) {
  if (Math.random()) {
    resolve(42);
  } else {
    resolve('str');
  }
}).then(function(numOrStr) {
  if (typeof numOrStr === 'string') {
    var a: string = numOrStr;
  } else {
    var b: number = numOrStr;
  }
  var c: string = numOrStr; // Error: number|string -> string
});

/////////////////////////////////////////////////
// == Promise constructor reject() function == //
/////////////////////////////////////////////////

// TODO: Promise constructor reject(T) -> catch(T)
new Promise<empty>(function(resolve, reject) {
  reject(0);
}).catch(function(num) {
  var a: number = num;

  // TODO
  var b: string = num; // Error: number ~> string
});

// TODO: Promise constructor reject(Promise<T>) ~> catch(Promise<T>)
new Promise<empty>(function(resolve, reject) {
  reject(new Promise<empty>(function(resolve, reject) {
    reject(0);
  }));
}).catch(function(num) {
  var a: Promise<number> = num;

  // TODO
  var b: number = num; // Error: Promise<Number> ~> number
});

// TODO: Promise constructor reject(T); reject(U); -> then(T|U)
new Promise<empty>(function(resolve, reject) {
  if (Math.random()) {
    reject(42);
  } else {
    reject('str');
  }
}).catch(function(numOrStr) {
  if (typeof numOrStr === 'string') {
    var a: string = numOrStr;
  } else {
    var b: number = numOrStr;
  }

  // TODO
  var c: string = numOrStr; // Error: number|string -> string
});

/////////////////////////////
// == Promise.resolve() == //
/////////////////////////////

// Promise.resolve(T) -> then(T)
Promise.resolve(0).then(function(num) {
  var a: number = num;
  var b: string = num; // Error: number ~> string
});

// Promise.resolve(Promise<T>) -> then(T)
Promise.resolve(Promise.resolve(0)).then(function(num) {
  var a: number = num;
  var b: string = num; // Error: number ~> string
});

// Promise.resolve(Promise<Promise<T>>) -> then(T)
Promise.resolve(Promise.resolve(Promise.resolve(0))).then(function(num) {
  var a: number = num;
  var b: string = num; // Error: number ~> string
});

////////////////////////////
// == Promise.reject() == //
////////////////////////////

// TODO: Promise.reject(T) -> catch(T)
Promise.reject<number>(0).catch(function(num) {
  var a: number = num;

  // TODO
  var b: string = num; // Error: number ~> string
});

// TODO: Promise.reject(Promise<T>) -> catch(Promise<T>)
Promise.reject<Promise<number>>(Promise.resolve(0)).then(function(num) {
  var a: Promise<number> = num;

  // TODO
  var b: number = num; // Error: Promise<number> ~> number
});

//////////////////////////////////
// == Promise.prototype.then == //
//////////////////////////////////

// resolvedPromise.then():T -> then(T)
Promise.resolve(0)
  .then(function(num) { return 'asdf'; })
  .then(function(str) {
    var a: string = str;
    var b: number = str; // Error: string ~> number
  });

// resolvedPromise.then():Promise<T> -> then(T)
Promise.resolve(0)
  .then(function(num) { return Promise.resolve('asdf'); })
  .then(function(str) {
    var a: string = str;
    var b: number = str; // Error: string ~> number
  });

// resolvedPromise.then():Promise<Promise<T>> -> then(T)
Promise.resolve(0)
  .then(function(num) { return Promise.resolve(Promise.resolve('asdf')); })
  .then(function(str) {
    var a: string = str;
    var b: number = str; // Error: string ~> number
  });

// TODO: resolvedPromise.then(<throw(T)>) -> catch(T)
Promise.resolve(0)
  .then(function(num) {
    throw 'str';
  })
  .catch(function(str) {
    var a: string = str;

    // TODO
    var b: number = str; // Error: string ~> number
  });

///////////////////////////////////
// == Promise.prototype.catch == //
///////////////////////////////////

// rejectedPromise.catch():U -> then(U)
Promise.reject<string>(0)
  .catch<string>(function(num) { return 'asdf'; })
  .then(function(str) {
    var a: string = str;
    var b: number = str; // Error: string ~> number
  });

// rejectedPromise.catch():Promise<U> -> then(U)
Promise.reject<string>(0)
  .catch<string>(function(num) { return Promise.resolve('asdf'); })
  .then(function(str) {
    var a: string = str;
    var b: number = str; // Error: string ~> number
  });

// rejectedPromise.catch():Promise<Promise<U>> -> then(U)
Promise.reject<string>(0)
  .catch<string>(function(num) { return Promise.resolve(Promise.resolve('asdf')); })
  .then(function(str) {
    var a: string = str;
    var b: number = str; // Error: string ~> number
  });

// resolvedPromise<T> -> catch() -> then():T | void
Promise.resolve(0)
  .catch(function(err) {})
  .then(function(num) {
    var a: number | void = num;
    var b: string = num; // Error: string ~> number
  });

// Promise<T> -> catch() with throw
function catchDefaultWithThrow<T>(p: Promise<T>) {
  const r = p.catch(() => {
    throw "";
  })
  r as Promise<T>; // okay
}
