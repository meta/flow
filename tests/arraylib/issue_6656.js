const a = ["a", "b"].reduce(acc => acc * 2, 1.0); // works
const b = ["a", "b"].reduce(acc => acc * 2, 1.0 as any); // error: `any` matches the `initialValue: T` overload
