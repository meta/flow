type Field<T> = {x: T}; // OK
type FieldNeg<in T> = {x: T}; // Error: in T in neutral position
type FieldPos<out T> = {x: T}; // Error: out T in neutral position
type PosField<T> = {readonly p: T}; // OK
type PosFieldNeg<in T> = {readonly p: T}; // Error: in T in positive position
type PosFieldPos<out T> = {readonly p: T}; // OK
type FlipPosFieldNeg<in T> = (o: {readonly x: T}) => void; // OK
type FlipPosFieldPos<out T> = (o: {readonly x: T}) => void; // Error: out T in negative position
type NegField<T> = {writeonly p: T}; // OK
type NegFieldNeg<in T> = {writeonly p: T}; // OK
type NegFieldPos<out T> = {writeonly p: T}; // Error: out T in negative position
type FlipNegFieldNeg<in T> = (o: {writeonly x: T}) => void; // Error: in T in positive position
type FlipNegFieldPos<out T> = (o: {writeonly x: T}) => void; // OK
type Get<T> = {get p(): T}; // OK
type GetNeg<in T> = {get p(): T}; // Error: in T in positive position
type GetPos<out T> = {get p(): T}; // OK
type Set<T> = {set p(x:T): void}; // OK
type SetNeg<in T> = {set p(x:T): void}; // OK
type SetPos<out T> = {set p(x:T): void}; // Error: out T in negative position
