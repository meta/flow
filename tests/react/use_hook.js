import {use} from 'react';

declare opaque type Theme;
declare const ThemeContext: React.Context<Theme>;

declare const AnswerPromise: Promise<42>;

declare const CustomThennable: {
  then(onFulfil: (value: string) => void): void;
 ...};

function Test() {
  // expected to not error
  const theme: Theme = use(ThemeContext);
  const answer: 42 = use(AnswerPromise);
  const customThen: string = use(CustomThennable);

  // expected to error - invalid return type
  const notAny: 0 = use(ThemeContext);
  const notAny2: 0 = use(AnswerPromise);
  const notAny3: 0 = use(CustomThennable);

  // expected to error - invalid argument
  use(null);
  use(undefined);
  use(0);
  use({});

  return null;
}
