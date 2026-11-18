declare opaque type CommonInstance;
declare opaque type AInstance: CommonInstance;
declare opaque type MetaInstance: CommonInstance;
declare opaque type InputInstance: CommonInstance;

type React$JSXIntrinsicElements = {
  [name: string]: {[prop: string]: any, ref?: React.RefSetter<CommonInstance>},
  a: {children?: React.Node, ref?: React.RefSetter<AInstance>, ...},
  div: {children?: React.Node, ref?: React.RefSetter<CommonInstance>, ...},
  span: {children?: React.Node, ref?: React.RefSetter<CommonInstance>, ...},
  input: {children?: React.Node, ref?: React.RefSetter<InputInstance>, ...},
  meta: {
    children?: React.Node,
    ref?: React.RefSetter<MetaInstance>,
    [key: StringPrefix<'data-'>]: string | number,
  },
}
