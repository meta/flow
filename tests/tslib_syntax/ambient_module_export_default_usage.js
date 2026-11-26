import value from 'export-default-value';
import qualified from 'export-default-qualified';
import C from 'export-default-class';
import other from 'export-default-other-block';
import Foo from 'export-default-plain-class';
import overloaded from 'export-default-overloads';
import D from 'export-default-heritage';
import implementation from 'export-default-implementation';

value as string;
value as number; // ERROR
qualified as number;
qualified as string; // ERROR
new C().x as number;
new C().x as string; // ERROR
other as number;
other as string; // ERROR
new Foo().x as number;
new Foo().x as string; // ERROR
overloaded(1) as number;
overloaded('') as string;
new D().b as number;
new D().b as string; // ERROR
implementation(); // ERROR
