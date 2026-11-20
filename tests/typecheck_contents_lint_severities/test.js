/* @flow */

/* This test ensures that lint severity cover information from dependencies is
 * merged before filtering errors in typecheck-contents. If we don't include
 * dependency cover information, error filtering will raise. */

import {f} from "./dep";

f((x) => {
  // This is an incomplete enum switch, but the relevant location is in dep.js
  switch (x) { default: }
});
