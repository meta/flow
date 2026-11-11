declare opaque type CommonInstance;
declare opaque type DivInstance: CommonInstance;
declare opaque type ButtonInstance: CommonInstance;
declare opaque type AInstance: CommonInstance;

type React$JSXIntrinsicElements = {
  [string]: {[string]: any, ref?: React.RefSetter<CommonInstance>},
  div: {ref?: React.RefSetter<DivInstance>, ...},
  button: {ref?: React.RefSetter<ButtonInstance>, ...},
  a: {ref?: React.RefSetter<AInstance>, ...},
}
