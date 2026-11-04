declare function invariant(condition: unknown, message?: string): asserts condition;
declare const x: 'a' | 'b';

function expressionAfterReturn(): string {
  return 'a';
  return match (x) { // ERROR
    'a' => 'a', // ERROR
    _ => 'b', // ERROR
  };
}

function expressionAfterAssertion(): string {
  invariant(false, 'unreachable');
  return match (x) { // ERROR
    'a' => 'a', // ERROR
    _ => 'b', // ERROR
  };
}

function statementAfterThrow(): void {
  throw x;
  match (x) { // ERROR
    'a' => {} // ERROR
    _ => {} // ERROR
  }
}
