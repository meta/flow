// @flow

import {exportedForward} from "./exporter";
import type {
  ExportedBadDefault,
  ExportedChain,
  ExportedCycle,
  ExportedMixedCycle,
  ExportedNestedBadCheckedDefault,
  ExportedNestedCycle,
  ExportedShadow,
} from "./exporter";

exportedForward("ok", ""); // OK
exportedForward<string, number>("bad", 0); // ERROR: string does not satisfy number

const chain: ExportedChain<1, number, number> = [1, 2, 3]; // OK
const badChain: ExportedChain<"bad", number, number> = ["bad", 2, 3]; // ERROR: "bad" does not satisfy number

const recoveredBadDefault: ExportedBadDefault<> = [true, "ok"]; // OK: invalid default is recovered as any

const badShadow: ExportedShadow<number> = 1; // ERROR: number does not satisfy the function bound

const recoveredNestedBadDefault: ExportedNestedBadCheckedDefault<> = [true, "ok"]; // ERROR: default remains a function

const badCycle: ExportedCycle<number, string> = [1, "bad"]; // OK: unlike local.js, the type sig recovers both cycle members as any

const badNestedCycle: ExportedNestedCycle<number[], string[]> = [[1], ["bad"]]; // ERROR: number[] does not satisfy Array<string[]>, nor string[] Array<number[]>

const recoveredMixedCycle: ExportedMixedCycle<number> = [1, 1]; // OK: recovered cycle members are any

import {exportedInferForward} from "./exporter";

exportedInferForward("ok"); // OK

type NestedA = Array<NestedB>;
type NestedB = Array<NestedA>;
const nestedCycle: ExportedNestedCycle<NestedA, NestedB> = [[], []]; // OK

import {exportedMax} from "./exporter";
import type {Comparable, ExportedSelfBound} from "./exporter";

declare class Num implements Comparable<Num> {
  compareTo(other: Num): number;
}
exportedMax(new Num(), new Num()) as Num; // OK
exportedMax(1, 2); // ERROR: number is not a Comparable<number>

type Nested = ReadonlyArray<Nested>;
const selfBound: ExportedSelfBound<Nested> = []; // OK
const badSelfBound: ExportedSelfBound<ReadonlyArray<number>> = []; // ERROR: number does not satisfy ReadonlyArray<number>
