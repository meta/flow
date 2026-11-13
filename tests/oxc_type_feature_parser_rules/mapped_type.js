type Mapped<O extends {...}> = {
  [K in keyof O]: O[K], // error
};
