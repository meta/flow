type C = <H>(x: H) => {[K in keyof H]: H[K]};
declare const c: C;
c('string')(); // error: string is not an object
