declare interface Callback<T> {
  (lock: string | null): T;
}

declare function request<T>(name: string, callback: Callback<T>): Promise<T>;

// A function against an interface with a call signature: the call signature.
const p = request('k', lock => 1); // ok
p as Promise<number>; // ok
p as Promise<string>; // error
