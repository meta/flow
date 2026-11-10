import {useMemo, useState} from 'react';

declare const items: Array<number>;

component CallbackInNestedFunction() {
  function renderItems() {
    return items.map(() => {
      useState(); // error: react-rule-hook-conditional
    });
  }
  renderItems();
  return null;
}

component CallbackInMemoizedObject() {
  useMemo(
    () => ({
      isHidden: () => {
        useState(); // error: react-rule-hook-conditional
      },
    }),
    [],
  );
  return null;
}

hook useCallbackInMemoizedObject() {
  return useMemo(
    () => ({
      isHidden: () => {
        useState(); // error: react-rule-hook-conditional
      },
    }),
    [],
  );
}

component NamedFunctionInCallback() {
  items.forEach(() => {
    function helper() {
      useState(); // error: react-rule-hook-definitely-not-in-component-or-hook
    }
    helper();
  });
  return null;
}

function notAComponentOrHook() {
  items.forEach(() => {
    useState(); // error: react-rule-hook
  });
}

class HookInClass {
  initializer: unknown = useState(); // error: react-rule-hook-class
  arrow = () => {
    useState(); // error: react-rule-hook-class
  };

  constructor() {
    useState(); // error: react-rule-hook-class
    const options = {
      getSubtitle: () => () => {
        useState(); // error: react-rule-hook-class
      },
    };
    options.getSubtitle();
  }

  method() {
    useState(); // error: react-rule-hook-class
    items.forEach(() => {
      useState(); // error: react-rule-hook-class
    });
    function helper() {
      useState(); // error: react-rule-hook-definitely-not-in-component-or-hook
    }
    helper();
  }
}

component ClassInComponent() {
  class Inner {
    method() {
      useState(); // error: react-rule-hook-class
    }
  }
  return null;
}

declare hook use(x: unknown): unknown;

function callsUseInCallback() {
  items.forEach(() => {
    use(items); // error: react-rule-hook-conditional
  });
}
