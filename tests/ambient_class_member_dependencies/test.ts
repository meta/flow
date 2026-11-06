import {Visitor, Node, Container, Dictionary} from './defs';

// Abstract methods preserve their parameter and return annotations.
declare const visitor: Visitor;
const methodValue: number = visitor.visit(new Node()).value; // OK
const wrongMethodValue: string = visitor.visit(new Node()).value; // ERROR

// Abstract properties preserve their value annotations.
declare const container: Container;
const propertyValue: string = container.node.value; // OK
const wrongPropertyValue: number = container.node.value; // ERROR

// Class index signatures preserve their value annotations.
declare const dictionary: Dictionary;
const indexValue: boolean = dictionary.example.value; // OK
const wrongIndexValue: number = dictionary.example.value; // ERROR
