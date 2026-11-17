const f = function (this: number) {
  this as number; // OK
};
f.a = 1;

f.a as number; // OK
f.a as empty; // ERROR

const g = function m() {};
g.v = true;

g.v as boolean; // OK

f.c = ""; // OK
f.c as string; // OK

g.w = 1; // OK
g.w as number; // OK

declare function debounce<Args>(
  func: (...args: Args) => unknown,
): (...args: Args) => void;
{
  const d = debounce((s: string, n: number): void => {});
  const f = function (s: string) {
    return function (n: number) {
      d(s, n); // OK
      d(n, s); // ERROR
    }
  };
  const g = function n(s: string) {
    return function m(n: number) {
      d(s, n); // OK
      d(n, s); // ERROR
    }
  };
}

// Annotated
{
  const f: {(): number, a: string, ...} = function () { return 1}; // ERROR on assignment
  f.a = "hi"; // OK
  f.a = 1; // ERROR
  f.xxx; // ERROR
}
