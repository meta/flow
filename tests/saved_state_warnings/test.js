// @flow

import {E, foo} from './dep';

switch (foo) { // incomplete enum switch (cross-module)
  default:
}

const bar: E = E.A;
switch (bar) { // incomplete enum switch (local)
  default:
}
