export interface Zippable {
  [key: string]: ZippableFile;
}
export type ZippableFile =
  | string
  | Zippable;

declare const z: Zippable;
const f: ZippableFile = z['x'];
