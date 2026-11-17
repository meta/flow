declare hook useCustom<T>(x: T): [T];

const [v] = useCustom({a: 42});
v.a = 100; // Error, x is not writable

useCustom as <T>(x: T) => [T]; // error, hook and nonhook incompatible
useCustom as hook <T>(x: T) => [T]; // ok

declare const nonhook: <T>(x: T) => [T];
nonhook as typeof useCustom; // error, nonhook and hook incompatible

declare hook useCustom2<T>(x: T): [T];
useCustom as typeof useCustom2; // ok, hook annotations compatible

useCustom as typeof useCustom; // ok

import * as React from 'react';
import * as HookReact from './hookReact';

declare hook useBadHook(x: Array<number>, y: React.RefObject<number>): void;

function nonHook() {
    useBadHook([], 42 as any); // error
    const badName = useBadHook;
    badName([], 42 as any); // error

    function nonHookInternal() { }
    const useNonHookInternal = nonHookInternal;
    useNonHookInternal(); // ok
}

component C() {
    const badName = useBadHook;
    badName([], 42 as any); // error

    if (true) {
        useBadHook([], 42 as any); // error
    }

    let f;
    if (true) {
        f = useBadHook
    } else {
        function badhook(x: Array<number>, y: React.RefObject<number>): void { }
        f = badhook
    }
    f([], 42 as any) // error

    const useBadName = nonHook;
    useBadName(); // error
    return null;
}

hook useC() {
    const badName = useBadHook;
    badName([], 42 as any); // error

    if (true) {
        useBadHook([], 42 as any); // error
    }

    let f;
    if (true) {
        f = useBadHook
    } else {
        function badhook(x: Array<number>, y: React.RefObject<number>): void { }
        f = badhook
    }
    f([], 42 as any) // error

    const useBadName = nonHook;
    useBadName(); // error
}

{
  hook useFoo() {
    Bar?.useFoo(); // should be react-rule-hook-conditional error instead of react-rule-hook-naming-convention
  }

  declare const Bar: ?{useFoo: typeof useFoo}
}

{
  type NonTransitive<out T> = {o: T}; // expected variance error

  declare hook useReturn(): NonTransitive<''>;
  useReturn as hook () => NonTransitive<string>; // ok: ReactDRO is applied at calling time, so it won't force unwrap the TypeAppT
}
