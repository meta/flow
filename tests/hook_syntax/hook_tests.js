hook useCustom<T>(x: T): [T] { return [x]; }

const [v] = useCustom({a: 42});
v.a = 100; // Error, x is not writable

useCustom as <T>(x: T) => [T]; // error, hook and nonhook incompatible
useCustom as hook <T>(x: T) => [T]; // ok

declare const nonhook: <T>(x: T) => [T];
nonhook as typeof useCustom; // error, nonhook and hook incompatible

hook useCustom2<T>(x: T): [T] { return [x];}
useCustom as typeof useCustom2; // error, hooks unique

useCustom as typeof useCustom; // ok

hook useCoolHook(): Array<number> {
    const [foo, ref] = useCoolHook2();
    foo.a = "hi"; // error
    return [];
}

hook useCoolHook2(): [{a: string}, React.RefObject<number>] {
    return 42 as any;
}

import * as React from 'react';
import * as HookReact from './hookReact';

component B() {
    const arr = useCoolHook();
    arr[0] = 42; // error
    const [foo, ref] = useCoolHook2();
    foo.a = "hi"; // error
    HookReact.useEffect(() => {
        ref.current = 42; // ok
    })
    return null;
}

hook useBadHook(x: Array<number>, y: React.RefObject<number>): void {
    x[0] = 42; // error
    HookReact.useEffect(() => {
        y.current = 42; // ok
    })
    const w = HookReact.useRef<?number>(null);
    w.current = 42; // error
}

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
    hook useH() { }
    component C(cond: boolean, s:string) {
        label1: {
            label2: {
                if (cond) break label2;
                useH(); // error
            }
            useH(); // no error
            label3: {
                if (cond) break label1;
            }
            useH(); // error
        }
        useH(); // no error

        switch (s) {
            case 'a':
                useH(); // error
                break;
            default:
                useH(); // error
        }
        useH(); // no error
        switch (s) {
            default:
                useH(); // error
            case 'a':
                useH(); // error, even though technically its ok
        }
        useH(); // no error
        switch (s) {
            case 'a':
                useH(); // error
            default:
                switch (s) {
                    case 'a':
                        useH(); // error
                        break;
                    default:
                        useH(); // error
                }
                useH(); // no error
                if (cond) return null;
        }
        useH(); // error

        return null;
    }

    component D(c: boolean) {
        try {
            {
                42;
                useH(); // no error
                useH(); // error
            }
            if (c) { return 42 }
        } catch (e) {
            useH(); // error
        } finally {
            useH(); // no error;
        }
        useH(); // error
        return 42;
    }
}
