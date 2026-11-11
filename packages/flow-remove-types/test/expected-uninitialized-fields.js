/*       */
// @nolint

// Regular import
import {
  Something,
                
                       
} from 'some-module';

// Regular import with types only
import {
                     
                         
} from 'some-module';

// Mixed default and named type only imports
import DefaultImport, {
                         
                             
} from 'some-module';

// Import types
                                                  

// Typed function
async function test(x      , y /*.*/   /*.*/ , z /*.*/   /*.*/                = 123)         {
  // Typed expression
  return await (x     );
}

// Interface
               
            

                  
 

// Exported interface
                         
                 
 

// Interface extends
                                
                
 

// Implements interface
class Bar extends Other            /*.*/                 {
  // Class Property with default value
  answer         = 42;

  // Class Property with default value and variance
   covariant         = 42;

  // Class Property
            

  // Class Property with variance
                  

  method()        {
    return;
  }
}

// Class expression implements interface
var SomeClass = class BazClass                {
            

  method()        {
    return;
  }
};

// Parametric class
class Wrapper           {
  get()           {
    return this.value;
  }

  map         ()                   {
    // do something
  }
}

// Extends Parametric class
class StringWrapper extends Wrapper         {
  // ...
}

// Declare class
                   
                  
 

// Declare function
                                  

// Declare interface
                              
                 
 

// Declare module
                     
                                                   
 

// Declare type alias
                         
              
             
  

// Declare variable
                               

// Type alias
                         

// Export type
                           

// Export type *
                                 

// Regular export
export { Wrapper };

// Exported type alias
                                  

// Object with types within
var someObj = {
  objMethod()       {
    // do nothing.
  }
}

// Example from README
import SomeClassImport from 'some-module'
                                                

export class MyClass              extends SomeClassImport                          {

                    

  constructor(value             ) {
    this.value = value
  }

  get()              {
    return this.value
  }

}

// Test async/await functions
async function asyncFunction           (input           )                     {
  return await t;
}

// Test read-only data
                             
                             
   

// Test covariant type variant class with constraint and default.
export class TestClassWithDefault                                              {

  constructor() {}
}

var newline_arrow = () => 
          42;

var newline_arrow_2 = () =>  
        42;

// Test calling a function with explicit type arguments
doSomething        (3);
doSomething                           (3);

// Test invoking a constructor with explicit type arguments
new Event        ();

// Test type union and intersection syntax with leading "operator"
var union                                ;
var intersection                                  ;

// Test generic async arrow funcion
const asyncArrow = async            ()            => {};

// Comment type annotations are preserved
var X /*: {
  version: string,
} */ = { version: '42'};

function method(param /*: string */) /*: number */ {
  // ...
}

// declared class fields
class MyClassWithDeclare {
                       
}

// Comment type includes are not emptied out
class MyClassWithComment {
  /*:: prop: string; */
}

// Inferred predicate
function inferredPredicateWithType(arg       )                  {
  return !!arg;
}

function inferredPredicateWithoutType(arg       )          {
  return !!arg;
}

// Type guards
function typeGuardFunction(x       )               {
  return typeof x === "boolean";
}

const typeGuardArrow = (x       )               => (typeof x === "boolean");

function typeGuardInComments(x /*: mixed */) /*: x is boolean */ {
  return typeof x === "boolean";
}

function typeAssertsFunction1(x       )                       {
  if (typeof x !== "boolean") throw new Error;
}

function typeAssertsFunction2(x       )            {
  if (!x) throw new Error;
}

// Test function with default type parameter
function functionWithDefault                                       () {}

// Opaque types
                             
                                     
                            
                                    
                                    

// Declare export
                                   
                                               
                                  

// `this` params

                                                    
                                       
function thisParam1 (             ) {}
function thisParam2 (               ...a) {}
function thisParam3 (             
     ...a) {}
function thisParam4 (    
          

    ) {}
function thisParam5 (    
          

    
   ...a) {}
function thisParam6(
          
) {}
function thisParam7(
          
  a        
) {}

function thisParam8(
          
  a        
) {
  function thisParam9(         a        ) {}
}

const thisConst1 = function(            ) {}
const thisConst2 = function(              ...a) {}
const thisConst3 = function(    
         
...a) {}
const thisConst4 = function(    
        

 ) {}
const thisConst5 = function(    
         
a        ,) {}

// `as` cast
const asAny = 'any'       ;
const asArray = [1, 2, 3]            ;
const asBigIntLiteral = 1n      ;
const asBigInt = 1n          ;
const asBooleanLiteral = true        ;
const asBoolean = true           ;
const asComponent = (() => {})                                    ;
const asComponentGeneric = (() => {})                      ;
const asComponentGenericWithDefault = (() => {})                            ;
const asEmpty = {}         ;
const asExists = 'exists'     ;
const asFunction = (() => {})              ;
const asFunctionGeneric = (() => {})                     ;
const asFunctionGenericWithDefault = (() => {})                                ;
const asKeyof = 'a'                                  ;
const asMixed = 'mixed'         ;
const asNullable = null           ;
const asNullLiteral = null        ;
const asNumberLiteral = 1     ;
const asNumber = 1          ;
const asObject = { a: 'a' }                 ;
const asParametrizedGeneric = 'generic'                         ;
const asStringLiteral = 'literal'             ;
const asString = 'string'          ;
const asSymbol = Symbol('symbol')          ;
const asTuple = ['a', 1]                    ;
const asTypeof = 'typeof'                   ;
const asUnion = 'union'                   ;
const asVoid = undefined        ;

                                                                                           
const asConditional = 'conditional'                           ;

                                                 
const asInterface = { a: 'a', b: 1 }                 ;

                                                                              
const asInfer = 'infer'                     ;

                                                      
const asIntersection = { a: 'a', b: 1 }                    ;

const asIndexed = 'indexed'                       ;

// `as const`
's'         ;
['s']         ;

// chained `as`
const chain1 = '1'                     ;
const chain2 = '1'                ;

                                              
const templateLiteral                     = `prefix-${42}`;
const asTemplateLiteral = templateLiteral                      ;

export          /*.*/ class AbstractBase    {
                       
                                  
                                     
                                   

  concrete(value   )    {
    return value;
  }

  static create()         {
    return 1;
  }
}

         class AbstractDerived extends AbstractBase                        {
                                     
                                           
}

class ConcreteDerived extends AbstractDerived {
           /*.*/ property         = 42;
                                 

           method   (value   )    {
    return value;
  }

  static /*.*/          create()         {
    return 2;
  }

           ['computed'](value        )         {
    return value;
  }

           get result()         {
    return this.property;
  }

           set result(value        )       {
    this.property = value;
  }
}

const AbstractExpression =          class {
                            
                          
};

export default          class {
                          
}

                                         
                          
 

class ModifierNames {
  abstract         = 'abstract';
  override()         {
    return 'override';
  }
}

const satisfiesObject = {value: 42} /*.*/           /*.*/                ;
const satisfiesGeneric = []                        ;
const satisfiesFunction = ((value        )         => value)                             ;
const satisfiesConst = {value: 42}                                   ;
const satisfiesChain = 42                                            ;
const satisfiesTemplate = 'prefix-42'                             ;
const satisfiesPredicate = ((value       )          => true)                                            ;
const asPredicate = ((value       )          => true)                                     ;

class OptionalFields extends ConcreteDerived {
                    
  initialized          = 42;
                                       
                                       
                    
                                  
                                     
                             
                                   
}

let definiteVariable         ;
let definiteFirst /*.*/   /*.*/         , definiteSecond         ;

class DefiniteFields extends ConcreteDerived {
                    
                                       
                                       
                            
                                     
                                     
}

                                       
                                                                                                        
export {                                  Something as InlineFirst} from 'some-module';
export {Something as InlineLast,                                } from 'some-module';
export {Something as InlineTrailing,                                     } from 'some-module';
export {Something as InlineMiddle,                                    SomeOtherThing as InlineOther} from 'some-module';
export {
                                       
  Something as InlineCommented,
                                                       /*.*/ 
} from 'some-module';
export {} from 'side-effect-module';

const type = 42;
export {type as ContextualType};
export {Something as type} from 'some-module';

                                    

                                                  
                                            
                                                                          
                                                           
                                                              

// Type-only default exports are erased without leaving a runtime wrapper.
                                           
                
 
