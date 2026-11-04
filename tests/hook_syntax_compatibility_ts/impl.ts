// A TypeScript implementation calls hooks from functions named `use*`, which
// are hooks under hook compatibility mode.
import {useValue} from './provider';

export function useDouble(): number {
  return useValue() * 2; // OK
}

function getDouble(): number {
  return useValue() * 2; // ERROR: hooks can only be called in components and hooks
}
