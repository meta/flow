type G<T> = {v: T, next: H<{v: T}>};
type H<T> = {v: T, next: G<{v: T}>};

declare const g: G<string>;
g as H<string>; // ok
g as H<number>; // error
