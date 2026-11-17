//@flow
const React = require('react');

function Component<T>(props: {foo?: T, ...}): React.Node { return null; }

let x = <Component />; // No error
let y: React.Node = <Component />; // No error

function Wrapper(props: {children: React.Node, ...}) { return null }

const z = (
  <Wrapper>
    <Component />
  </Wrapper>
);

function ComponentWithBound<T extends number>(pprops: {a:T, ...}): React.Node {}
<div><ComponentWithBound a={true} /></div>; // error: bool ~> number

component ComponentSyntaxNoRenders() {
  return <Component />; // ok
}

declare component NeedsExplicitTargs<T>(f: (x: T) => void);

<NeedsExplicitTargs<string> f={(v) => { v as string;}} />; // ok

declare const PolyObjectComponent: {
  <T>(props: {foo: T, fn: (x: T) => void, ...}): React.Node,
 ...};
declare const PolyFnComponent: <T>(props: {foo: T, fn: (x: T) => void, ...}) => React.Node;

<PolyObjectComponent foo={3} fn={(v) => {v as string; /* should error on cast */ }} />;
<PolyFnComponent foo={3} fn={(v) => {v as string; /* should error on cast */ }} />;

declare opaque type DataSource<T>;
declare function makeDataSource<T>(items: ReadonlyArray<T>): DataSource<T>;
component Table<T>(dataSource: DataSource<T>, render: (row: T) => void) {
  return null;
}

type Row = {id: string};
declare const rows: ?ReadonlyArray<Row>;
<Table
  dataSource={makeDataSource(rows?.map(row => ({id: row.id})) ?? [])}
  render={(item: Row) => { item.id; }}
/>; // okay
