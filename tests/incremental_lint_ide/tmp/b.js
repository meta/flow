// @flow

import type {Fn} from "./a";

declare const f: Fn;

f(x => {
  switch (x) { default: }
});
