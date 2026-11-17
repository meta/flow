//@flow
declare const x: {x?: {[key: string]: number}, ...};

const z = {...{foo: 3}, ...x.x};
