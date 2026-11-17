const useZ = require('./exp');

const useX = 42;

import { useW } from './exp2';
import useU from './exp2';
import * as useV from './exp2';

function useT<T>() {}
declare const useS: {
    <T>(x: T): T;
}

const bag_o_hooks = {
    useR() { useT(); } // no error
};

component C(...{ useY }: { useY: () => void}) {
    useY(); // error: unsupported hook compatibility
    useX(); // error for calling number but no hook error
    useZ(); // no error
    useW(); // no error
    useU(); // no error
    useV(); // error for calling module but no hook error
    useT(); // no error
    useT<unknown>(); // no error
    useS(); // no error
    useS<unknown>(); // no error
    bag_o_hooks.useR(); // error: unsupported hook compatibility
    return 42;
}

import * as React from 'react';
hook useH() { };
declare function id<T>(x: T): T;

const C1 = () => { useH() }; // no error
const c1 = () => { useH() }; // error
const C2 = React.forwardRef<_, unknown, _>(({}: {}, ref: unknown) => { useH() }); // no error
const C3 = id(({}: {}, ref: unknown) => { useH() }); // error

const myHOC = () => () => { useH() }; // no error;
function myHOC2() {
    return function() {
        useH(); // no error
    }
}

declare const useXA: ((() => void) & () => number)

const [useXB, {w: useXC}] = 42 as any as [() => void, { w: () => void }]


{
    component AC(
        useNodeActionItems: (n: number) => Array<string>,
        useValidator?: (s: string) => boolean,
        w: { useXD: () => void}
      ) {
        const actionItems = useNodeActionItems(1); // error: params won't get `MakeHookLike` transform. Higher order hooks are not supported.
        const validator = useValidator?.(""); // // error: params won't get `MakeHookLike` transform. Higher order hooks are not supported.
        useXA(); // no error
        useXB(); // no error
        useXC(); // no error
        w.useXD(); // error: unsupported hook compatibility
        return null;
      }
}

{
    const tests = {
        render() {
            const x = useH(); // no error
        }
    }
    declare const renderHook: any;
    renderHook(() => useH()); // no error
    declare const renderHookAsync: any;
    renderHookAsync(() => useH()); // no error
}
