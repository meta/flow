import {
  intersectionKeysProp,
  intersectionIndexerKeysProp,
  recursiveFirstProp,
  recursiveInUnionProp,
  recursiveInUnionLastProp,
  conditionalFirstProp,
  conditionalLastProp,
  accessedByIntersectionProp,
  sharedMembersProp,
  selfKeyedProp,
} from './export_values';

intersectionKeysProp as number; // OK
intersectionKeysProp as string; // ERROR
intersectionIndexerKeysProp as number; // OK
intersectionIndexerKeysProp as string; // ERROR
recursiveFirstProp as string; // ERROR
recursiveInUnionProp as empty; // OK: unsupported key
recursiveInUnionLastProp as empty; // OK: unsupported key
conditionalFirstProp as string; // ERROR
conditionalLastProp as string; // ERROR
accessedByIntersectionProp as empty; // OK: unsupported key
sharedMembersProp as string; // ERROR
selfKeyedProp as string; // ERROR
