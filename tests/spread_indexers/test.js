//@flow
const a: {[key: string]: unknown} = {};
const b: {[key: string]: unknown} = {};
const c: {[key: string]: unknown} = {...a, ...b}; // Ok

const d: {} = {...null}; // {} ~> {} errors, so spreading null
const e: {[key: string]: unknown} = {};
const f: {[key: string]: unknown} = {...d, ...e}; // Ok

const g: {[key: string]: number} = {};
const h: {[key: number]: string} = {};
const i = {...g, ...h}; // Error. Keys and values of the indexers fail to subtype

const j: {...} = {};
const k: {[key: string]: number} = {};
const l = {...j, ...k}; // Error, there may be some properties in j that are not overwritten by the indexer

const m: {foo: number} = {foo: 3};
const n: {[key: string]: string} = {foo: 'string'};
const o = {...m, ...n}; // Error, indexer may overwrite properties with explicit keys

const p: {foo: number, ...} = {foo: 3};
const q: {[key: string]: string} = {foo: 'string'};
const r = {...p, ...q}; // Error, indexer may overwrite properties with explicit keys

const v: {[key: string]: number} = {};
const w: {[key: number]: string} = {};
const x: {readonly [key: string | number]: string | number} = v;
const y: {readonly [key: string | number]: string | number} = w;
const z: {[key: string | number]: string | number} = {...x, ...y}; // ok


const a2: {[key: string]: unknown} = {};
const b2: {[key: 'a' | 'b']: unknown} = {};
const c2: {[key: string]: unknown} = {...a2, ...b2}; // Ok

const d2: {[key: string]: number} = {};
const e2: {[key: string]: 1} = {};
const f2: {[key: string]: number} = {...d2, ...e2}; // ok.
