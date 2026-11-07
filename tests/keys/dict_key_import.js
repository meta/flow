import type {DictKey} from './dict_key_export';

const o = {a: 1};
declare const k1: DictKey;
declare const k2: DictKey;
o[k1]; // error at k1, not at the indexer key in dict_key_export.js
o[k2]; // error at k2
