interface MergedAcrossFiles {
  m(x: string): 'later file';
  n(x: number): 'later file';
}

interface String {
  split(separator: 'magic'): Array<number>;
}
