// `Values` of an object type with markers inside is evaluated: `T` is `Row`, from the hint, which
// gives `row` its type.
type Column<T> = {getValue: (v: T) => ?number};
type Columns<T> = {a: Column<T>, b: Column<T>};
declare function column<T>(getValue: (v: T) => ?number): Values<Columns<T>>;
type Row = {x: number};
const columns: Array<Column<Row>> = [column(row => row.x)]; // ok
