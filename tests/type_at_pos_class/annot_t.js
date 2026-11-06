// @flow

declare class Seq<K, out V> {
  static Keyed: typeof KeyedSeq;
}
declare class KeyedSeq<K, out V> extends Seq<K, V> {
  key: K;
}
declare const a: Seq.Keyed<number, number>
//                    ^
