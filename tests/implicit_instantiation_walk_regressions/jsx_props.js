import * as React from 'react';

function case1() {
  type Props<P extends {...}> = Readonly<{a?: (x: P) => P, data: ReadonlyArray<P>}>;

  declare component Chart<P extends {...}>(...props: Props<P>);
  declare function id<P>(x: P): P;
  declare const series: ReadonlyArray<{time: number}>;

  <Chart a={id} data={series} />;
}

function case2() {
  declare opaque type Action<out TD>;
  type Config<TD> = {f?: (x: TD) => void, v: number};
  type Props<TD> = Readonly<{action: Action<TD>, actionConfig: Omit<Config<TD>, 'v'>}>;

  declare component Modal<TD extends {...} = {...}>(...props: Props<TD>);
  declare const action: Action<{done: boolean}>;
  declare const props: Pick<React.PropsOf<Modal<>>, 'actionConfig'>;

  <Modal action={action} actionConfig={props.actionConfig} />;
}

function case3() {
  class Token<T = void> extends React.Component<{entry?: T}> {
    props: {entry?: any};
  }

  declare const s: string;

  <Token entry={s} />;
}

function case4() {
  type Props<T> = {data: T, onClick?: (data: T) => void, ...};

  class MenuItem<T> extends React.PureComponent<Props<T>> {
    static defaultProps: {data: null} = {data: null};
  }

  declare const onClick: (data: {id: string, ...}) => void;
  declare const id: string;

  <MenuItem data={{id}} onClick={onClick} />; // error: `T` includes the `null` of `defaultProps`
}
