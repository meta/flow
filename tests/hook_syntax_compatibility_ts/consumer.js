// @flow
//
// A file outside hook compatibility mode calls the hooks declared in
// TypeScript. They are hooks, as if their files were in hook compatibility
// mode.

import {Hooks, getValue, useIdentity, useValue} from './provider';
import {useDouble} from './impl';
import {useFlowValue} from './flow_provider';

hook useConsumer(): number {
  return (
    useValue() + // OK
    useIdentity(1) + // OK
    Hooks.useNamespaced() + // OK
    useDouble() + // OK
    useGlobalValue() + // OK
    getValue()
  );
}

component Component() {
  useValue(); // OK
  return null;
}

function notAHook(): number {
  return useValue(); // ERROR: hooks can only be called in components and hooks
}

hook useFlowConsumer(): number {
  // A Flow function named `use*` outside hook compatibility mode is not a hook.
  return useFlowValue(); // ERROR
}
