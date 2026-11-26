
import * as React from 'react';
import {forwardRef, memo} from 'react';

declare hook useState(...args: any): any;
declare hook use(x: any): any;
declare hook useEffect(...args: any): any;
declare hook useLayoutEffect(...args: any): any;
declare hook useCallback(...args: any): any;
const ReactWithHooks = { useState, use, useEffect, useLayoutEffect, useCallback };

declare function doSomething(): void

declare const cond: boolean;

// Valid because components can use hooks.
component ComponentWithHook() {
  useHook();
  return null;
}

// Valid because components can use hooks.
function createComponentWithHook() {
  component ComponentWithHook() {
    useHook();
    return null;
  }
  return ComponentWithHook;
}

// Valid because hooks can use hooks.
hook useHook1(arg?: unknown) {
  useHook();
}

// Valid because hooks can use hooks.
function createHook1() {
  hook useHookWithHook() {
    useHook();
  };
  return useHookWithHook;
}

// Valid because components can call functions.
component ComponentWithNormalFunction() {
  doSomething();
  return null;
}

// Valid because functions can call functions.
function normalFunctionWithNormalFunction() {
  doSomething();
}

// Valid because functions can call functions.
function normalFunctionWithConditionalFunction() {
  if (cond) {
    doSomething();
  }
}

// Valid because functions can call functions.
declare const userFetch: any;
function functionThatStartsWithUseButIsntAHook() {
  if (cond) {
    userFetch();
  }
}

// Valid although unconditional return doesn't make sense and would fail other rules.
// We could make it invalid but it doesn't matter.
hook useUnreachable() {
  return;
  useHook(); // flow unreachable error
}

// Valid because hooks can call hooks.
hook useHook() {
  useState();
}

// Valid because hooks can call hooks.
hook useHookA() {
  useHook1();
  useHook2();
}

// Valid because hooks can call hooks.
function createHook2() {
  hook useHook() {
    useHook1();
    useHook2();
  };
  return useHook;
}

// Valid because hooks can call hooks.
hook useHook2() {
  useState() && a;
}

// Valid because hooks can call hooks.
hook useHookB() {
  return [useHook1(), useHook2()];
}

// Valid because hooks can call hooks.
hook useHookC() {
  return useHook1(useHook2());
}


// Valid because hooks can be used in anonymous arrow-function arguments
// to forwardRef.
const FancyButton1 = React.forwardRef((props: any, ref: any) => {
  useHook(); // error only in noncompat
  return <button {...props} ref={ref} />;
});

// Valid because hooks can be used in anonymous function arguments to
// forwardRef.
const FancyButton2 = React.forwardRef(function (props: any, ref: any) {
  useHook();// error only in noncompat
  return <button {...props} ref={ref} />;
});

const FancyButton3 = forwardRef(function (props: any, ref: any) {
  useHook();// error only in noncompat
  return <button {...props} ref={ref} />;
});

const MemoizedFunction1 = React.memo((props: any) => {
  useHook(); // ok: called in fn contextually typed as component
  return <button {...props} />;
});


const MemoizedFunction2 = memo(function (props: any) {
  useHook(); // ok: called in fn contextually typed as component
  return <button {...props} />;
});

// Valid because classes can call functions.
// We don't consider these to be hooks.
class S {
    useHook() { }
}

class C extends S {
  useHook() { }
  m() {
    this.useHook();
    super.useHook();
  }
}

declare const jest: any;
declare const beforeEach: any;
declare const fooState: any;
declare const _use: any;
declare const _useState: any;
declare const use_hook: any;
declare const each: any;
declare const pixelsWithInferredEvents: any;

// Valid -- this is a regression test.
jest.useFakeTimers();
beforeEach(() => {
  jest.useRealTimers();
});

// Valid because they're not matching use[A-Z].
fooState();
_use();
_useState();
use_hook();
// also valid because it's not matching the PascalCase namespace
jest.useFakeTimer();

// Regression test for some internal code.
// This shows how the "callback rule" is more relaxed,
// and doesn't kick in unless we're confident we're in
// a component or a hook.
declare const foo: any;
declare const extendedButton: any;

function useExtendedSelector(id: string) { return true }

function makeListener(instance: any) {
  each(pixelsWithInferredEvents, (pixel: any) => {
    if (useExtendedSelector(pixel.id) && extendedButton) {
      foo();
    }
  });
}

function useNotAHook(x: any) { }

declare const Obj: any;
declare const unknownFunction: any;

// This is valid because "use"-prefixed functions called in
// unnamed function arguments are not assumed to be hooks.
Obj.unknownFunction((foo: any, bar: any) => {
  if (foo) {
    useNotAHook(bar);
  }
});

// This is valid because "use"-prefixed functions called in
// unnamed function arguments are not assumed to be hooks.
unknownFunction(function (foo: any, bar: any) {
  if (foo) {
    useNotAHook(bar);
  }
});

// Regression test for incorrectly flagged valid code.
declare const a: any;
declare const b: any;
component RegressionTest1() {
  const foo = cond ? a : b;
  useState();
  return null;
}

// Valid because exceptions abort rendering
declare const page: any;
component RegressionTest2() {
  if (page == null) {
    throw new Error('oh no!');
  }
  useState();
  return null;
}

// Valid because the loop doesn't change the order of hooks calls.
component RegressionTest3() {
  const res = [];
  const additionalCond = true;
  for (let i = 0; i !== 10 && additionalCond; ++i) {
    res.push(i);
  }
  ReactWithHooks.useLayoutEffect(() => {});
  return null;
}

// Is valid but hard to compute by brute-forcing
declare const c: boolean;
component MyComponent() {
  // 40 conditions
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }
  if (c) {
  } else {
  }

  // 10 hooks
  useHook();
  useHook();
  useHook();
  useHook();
  useHook();
  useHook();
  useHook();
  useHook();
  useHook();
  useHook();
  return null;
}

// Valid because the neither the conditions before or after the hook affect the hook call
// Failed prior to implementing BigInt because pathsFromStartToEnd and allPathsFromStartToEnd were too big and had rounding errors
hook useSomeHook() {};

declare const FILLER: any;
declare const anyConditionCanEvenBeFalse: any;
component SomeName() {
  const filler = FILLER ?? FILLER ?? FILLER;
  const filler2 = FILLER ?? FILLER ?? FILLER;
  const filler3 = FILLER ?? FILLER ?? FILLER;
  const filler4 = FILLER ?? FILLER ?? FILLER;
  const filler5 = FILLER ?? FILLER ?? FILLER;
  const filler6 = FILLER ?? FILLER ?? FILLER;
  const filler7 = FILLER ?? FILLER ?? FILLER;
  const filler8 = FILLER ?? FILLER ?? FILLER;

  useSomeHook();

  if (anyConditionCanEvenBeFalse) {
    return null;
  }

  return (
    <React.Fragment>
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
      {FILLER ? FILLER : FILLER}
    </React.Fragment>
  );
};

// Valid because the neither the condition nor the loop affect the hook call.
component App1(...props: {}) {
  const someObject = {propA: true};
  for (const propName in someObject) {
    if (propName === '') {
    } else {
    }
  }
  const [myState, setMyState] = useState(null);
  return null;
}

declare const Text: any;

component App2() {
  const text = use(Promise.resolve('A'));
  return <Text text={text} />;
}

declare const shouldShowText: boolean
declare const thing: any;
declare const thing2: any;
declare const backupQuery: any;
declare const shouldFetchBackupText: any;
declare const query: any;
component App3() {
  if (shouldShowText) {
    const text = use(query);
    const data = ReactWithHooks.use(thing);
    const data2 = ReactWithHooks.use(thing2);
    return <Text text={text} />;
  }
  return (
    <Text
      text={shouldFetchBackupText ? use(backupQuery) : 'Nothing to see here'}
    />
  );
}

declare const queries: Array<any>;
declare const Child: any;
declare const item: any;
component App4() {
  let data = [];
  for (const query of queries) {
    const text = use(item);
    data.push(text);
  }
  return <Child data={data} />;
}
declare const someCallback: any;
component App5() {
  const data = someCallback(x => use(x));
  return <Child data={data} />;
}


// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
component ComponentWithConditionalHook() {
  if (cond) {
    useHook(); // error
  }
  return null;
}
/* [conditionalError('useHook')] */

hook use42() { }

const Hook = {
    useState: useState,
    _useState: _useState,
    use42: use42,
    useHook: useHook,
    use_hook: use_hook
}

Hook.useState(); // error
Hook._useState();
Hook.use42(); // error
Hook.useHook(); // error
Hook.use_hook();
/* [
    topLevelError('Hook.useState'),
    topLevelError('Hook.use42'),
    topLevelError('Hook.useHook'),
  ] */

const This = { useHook: useHook };
const Super = { useHook: useHook };

class Cy {
  m() {
    This.useHook(); // error
    Super.useHook(); // error
  }
}
/* [classError('This.useHook'), classError('Super.useHook')] */


// With the ESLint rule this is a false positive (it's valid) that unfortunately
// we cannot avoid. Prefer to rename it to not start with "use"
// With Flow, it's ok because we know useFeatureFlag is not a hook
const FooStore = {
    useFeatureFlag() { }
}
class Foo extends React.Component<any> {
  render(): undefined {
    if (cond) {
      FooStore.useFeatureFlag();
    }
  }
}

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
hook useConditionalHook() { }
const Namespace = {
    useConditionalHook: useConditionalHook
}
component ComponentWithConditionalHookA() {
  if (cond) {
    Namespace.useConditionalHook(); // error
  }
  return null;
}
/* [conditionalError('Namespace.useConditionalHook')] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
function createComponent() {
  component ComponentWithConditionalHook() {
    if (cond) {
      useConditionalHook(); // error
    }
    return null;
  };
  return ComponentWithConditionalHook;
}
/* [conditionalError('useConditionalHook')] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
hook useHookWithConditionalHook() {
  if (cond) {
    useConditionalHook(); // error
  }
}
/* [conditionalError('useConditionalHook')] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
function createHook() {
  hook useHookWithConditionalHook() {
    if (cond) {
      useConditionalHook(); // error
    }
  };
  return useHookWithConditionalHook;
}
/* [conditionalError('useConditionalHook')] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
hook useTernaryHook() { }
component ComponentWithTernaryHook() {
  cond ? useTernaryHook() : null; // error
  return null;
}
/* [conditionalError('useTernaryHook')] */

// Invalid because it's a common misunderstanding.
// We *could* make it valid but the runtime error could be confusing.
hook useHookInsideCallback() { }
component ComponentWithHookInsideCallback() {
  ReactWithHooks.useEffect(() => {
    useHookInsideCallback(); // error
  });
  return null;
}
/* [genericError('useHookInsideCallback')] */

// Invalid because it's a common misunderstanding.
// We *could* make it valid but the runtime error could be confusing.
function createComponent2() {
  component ComponentWithHookInsideCallback() {
    ReactWithHooks.useEffect(() => {
      useHookInsideCallback(); // error
    });
    return null;
  };
  return ComponentWithHookInsideCallback;
}
/* [genericError('useHookInsideCallback')] */

const ComponentWithHookInsideCallback1 = React.forwardRef((props: any, ref: any) => {
  ReactWithHooks.useEffect(() => { // error only in noncompat
    useHookInsideCallback(); // error only in noncompat
  });
  return <button {...props} ref={ref} />;
});
/* [genericError('useHookInsideCallback')] */

const ComponentWithHookInsideCallback2 = React.memo((props: any) => {
  ReactWithHooks.useEffect(() => { // error only in noncompat
    useHookInsideCallback(); // error only in noncompat
  });
  return <button {...props} />;
});
/* [genericError('useHookInsideCallback')] */

// Invalid because it's a common misunderstanding.
// We *could* make it valid but the runtime error could be confusing.
component ComponentWithHookInsideCallback3() {
  function handleClick() {
    useState(); // error
  }
  return null;
}
/* [functionError('useState', 'handleClick')] */

// Invalid because it's a common misunderstanding.
// We *could* make it valid but the runtime error could be confusing.
function createComponentA() {
  component ComponentWithHookInsideCallback() {
    function handleClick() {
      useState(); // error
    }
    return null;
  };
  return ComponentWithHookInsideCallback
}
/* [functionError('useState', 'handleClick')] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
hook useHookInsideLoop() { }
component ComponentWithHookInsideLoop() {
  while (cond) {
    useHookInsideLoop(); // error
  }
  return null;
}
/* [loopError('useHookInsideLoop')] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
function renderItem() {
  useState(); // error
}

component List(...props: any) {
  return props.items.map(renderItem);
}
/* [functionError('useState', 'renderItem')] */

// Currently invalid because it violates the convention and removes the "taint"
// from a hook. We *could* make it valid to avoid some false positives but let's
// ensure that we don't break the "renderItem" and "normalFunctionWithConditionalHook"
// cases which must remain invalid.
hook useHookInsideNormalFunction() { }
function normalFunctionWithHook() {
  useHookInsideNormalFunction(); // error
}
/* [
    functionError('useHookInsideNormalFunction', 'normalFunctionWithHook'),
  ] */

// These are neither functions nor hooks.
function _normalFunctionWithHook() {
  useHookInsideNormalFunction(); // error
}
function _useNotAHook() {
  useHookInsideNormalFunction(); // error
}
/* [
    functionError('useHookInsideNormalFunction', '_normalFunctionWithHook'),
    functionError('useHookInsideNormalFunction', '_useNotAHook'),
  ] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
function normalFunctionWithConditionalHook() {
  if (cond) {
    useHookInsideNormalFunction(); // error
  }
}
/* [
    functionError(
      'useHookInsideNormalFunction',
      'normalFunctionWithConditionalHook'
    ),
  ] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
declare const d: any;
hook useHookInLoops() {
  while (a) {
    useHook1(); // error
    if (b) return;
    useHook2(); // error
  }
  while (c) {
    useHook3(); // error
    if (d) return;
    useHook4();  // error
  }
}
/* [
    loopError('useHook1'),
    loopError('useHook2'),
    loopError('useHook3'),
    loopError('useHook4'),
  ] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
hook useHookInLoops1() {
  while (a) {
    useHook1();  // error
    if (b) continue;
    useHook2(); // error
  }
}
/* [loopError('useHook1'), loopError('useHook2', true)] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
hook useLabeledBlock() {
  label: {
    if (a) break label;
    useHook(); // error
  }
}
/* [conditionalError('useHook')] */

hook useBreakInUnlabeledForLoop() {
  for (let i = 0; i < 10; i++) {
    break;
  }
  useState(); // ok
}

// Currently invalid.
// These are variations capturing the current heuristic--
// we only allow hooks in PascalCase or useFoo functions.
// We *could* make some of these valid. But before doing it,
// consider specific cases documented above that contain reasoning.
function ax() {
  useState(); // error
}
const whateverx = function b() {
  useState(); // error
};
const cx = () => {
  useState(); // error
};
let dx = () => useState(); // error
let ex;
ex = () => {
  useState(); // error
};
({
  f: () => {
    useState(); // error
  },
});
({
  g() {
    useState(); // error
  },
});
const {
  j = () => { // unrelated flow error
    useState(); // error
  },
} = {};
let k;
({
  k = () => {
    useState(); // error
  },
} = {});
/* [
    functionError('useState', 'a'),
    functionError('useState', 'b'),
    functionError('useState', 'c'),
    functionError('useState', 'd'),
    functionError('useState', 'e'),
    functionError('useState', 'f'),
    functionError('useState', 'g'),
    functionError('useState', 'j'),
    functionError('useState', 'k'),
  ] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
hook useHook3() {
  if (a) return;
  useState(); // error
}
/* [conditionalError('useState', true)] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
hook useHook4() {
  if (a) return;
  if (b) {
    console.log('true');
  } else {
    console.log('false');
  }
  useState(); // error
}
/* [conditionalError('useState', true)] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
hook useHook5() {
  if (b) {
    console.log('true');
  } else {
    console.log('false');
  }
  if (a) return;
  useState(); // error
}
/* [conditionalError('useState', true)] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
hook useHook6() {
  a && useHook1(); // error
  b && useHook2(); // error
}
/* [conditionalError('useHook1'), conditionalError('useHook2')] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
declare const f: any;
hook useHook7() {
  try {
    f();
    useState(); // error
  } catch {}
}
/* [
    // NOTE: This is an error since `f()` could possibly throw.
    conditionalError('useState'),
  ] */

// Invalid because it's dangerous and might not warn otherwise.
// This *must* be invalid.
hook useHook8(bar: any) {
  let foo1 = bar && useState(); // error
  let foo2 = bar || useState(); // error
  let foo3 = bar ?? useState(); // error
}
/* [
    conditionalError('useState'),
    conditionalError('useState'),
    conditionalError('useState'),
  ] */

hook useCustomHook() { }
const FancyButton4 = React.forwardRef((props: any, ref: any) => {
  if (props.fancy) {
    useCustomHook(); // error only in noncompat
  }
  return <button ref={ref}>{props.children}</button>;
});
/* [conditionalError('useCustomHook')] */

const FancyButton5 = forwardRef(function (props: any, ref: any) {
  if (props.fancy) {
    useCustomHook(); // error only in noncompat
  }
  return <button ref={ref}>{props.children}</button>;
});
/* [conditionalError('useCustomHook')] */

const MemoizedButton = memo(function (props: any) {
  if (props.fancy) {
    useCustomHook(); // error only in noncompat
  }
  return <button>{props.children}</button>;
});
/* [conditionalError('useCustomHook')] */

// Invalid because it's dangerous.
// Normally, this would crash, but not if you use inline requires.
// This *must* be invalid.
// It's expected to have some false positives, but arguably
// they are confusing anyway due to the use*() convention
// already being associated with Hooks.
useState(); // error
if (foo) {
  const foo = ReactWithHooks.useCallback(() => {}); // error
}
useCustomHook(); // error
/* [
    topLevelError('useState'),
    topLevelError('ReactWithHooks.useCallback'),
    topLevelError('useCustomHook'),
  ] */

// Valid in Flow, because no ambiguity about hookness of useBasename
declare const useBasename: any;
declare const createHistory: any;
const browserHistory = useBasename(createHistory)({
  basename: '/',
});

hook useFeatureFlag() { }
class ClassComponentWithFeatureFlag extends React.Component<any> {
  render(): undefined {
    if (foo) {
      useFeatureFlag(); // error
    }
  }
}
/* [classError('useFeatureFlag')] */

class ClassComponentWithHook extends React.Component<any> {
  render(): undefined {
    ReactWithHooks.useState(); // error
  }
}
/* [classError('ReactWithHooks.useState')] */

(class {
  useHook = () => {
    useState(); // error
  };
});
/* [classError('useState')] */

(class {
  useHook() {
    useState(); // error
  }
});
/* [classError('useState')] */

(class {
  h = () => {
    useState(); // error
  };
});
/* [classError('useState')] */

(class {
  i() {
    useState(); // error
  }
});
/* [classError('useState')] */

// not allowed in compatibility
async function AsyncComponent() {
  useState(); // error because the return type is not React.Node
}
/* [asyncComponentHookError('useState')] */

function notAComponent() {
  use(promise); // error
}
/* [functionError('use', 'notAComponent')] */
declare const promise: any;
const text = use(promise); // error
component App() {
  return <Text text={text} />;
}
/* [topLevelError('use')] */

class Cx {
  m() {
    use(promise); // error
  }
}
/* [classError('use')] */

// export const notAComponent = (): () => void => {
//   return () => {
//     useState();
//   };
// };
//     `,
//     // TODO: this should error but doesn't.
//     // errors: [functionError('use', 'notAComponent')],
//   },

//       export default () => {
//         if (isVal) {
//           useState(0);
//         }
//       }
//     `,
//     // TODO: this should error but doesn't.
//     // errors: [genericError('useState')],
//   },

//       function notAComponent() {
//         return new Promise.then(() => {
//           useState();
//         });
//       }
//     `,
//     // TODO: this should error but doesn't.
//     // errors: [genericError('useState')],
//   },

function FakeComponent(props: string) {
  useState(); // error: definitely not component
}
