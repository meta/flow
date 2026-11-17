export default function fn<A>(arg: (x: {...A, c: number, ...}) => void): (x: A) => void {
  return (a: A) => arg({...a, c: 42});
}
