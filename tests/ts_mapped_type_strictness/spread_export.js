// @flow

import {tsAB} from 'repro';

// Annotation inference computes the type of an unannotated export.
export const spread = {...tsAB};
