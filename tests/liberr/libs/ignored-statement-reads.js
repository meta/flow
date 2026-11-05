// Ignored statements are still checked after inference, so the names they read
// must resolve.
declare function f(): void;
f(); // error: expression statements are ignored
if (f()) {} // error: if statements are ignored
declare module 'reads-in-ignored-statements' {
  declare function g(): void;
  g(); // error: expression statements are ignored
}
