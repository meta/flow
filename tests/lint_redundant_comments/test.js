//Comments Redundant with Default (Error.)

//flowlint-line unnecessary-optional-chain:off
//flowlint-next-line unnecessary-optional-chain:off

//flowlint unnecessary-optional-chain:off


//Comments Redundant with Earlier Range Comments (The first in a triple is valid; the others are errors.)

//flowlint unnecessary-optional-chain:error
//flowlint unnecessary-optional-chain:error
//flowlint-line unnecessary-optional-chain:error

/*flowlint unnecessary-optional-chain:off*/ //Unused suppression
//flowlint unnecessary-optional-chain:off
//flowlint-line unnecessary-optional-chain:off


//Comments with Duplicate Arguments (Error.)

//flowlint unnecessary-optional-chain:error, unnecessary-optional-chain:error
//flowlint-line unnecessary-optional-chain:off, unnecessary-optional-chain:off

//flowlint unnecessary-optional-chain:off, unnecessary-optional-chain:off
//flowlint-line unnecessary-optional-chain:error, unnecessary-optional-chain:error


//Comments with Self-Contradicting Arguments (Error.)

//flowlint unnecessary-optional-chain:error, unnecessary-optional-chain:off
//flowlint-line unnecessary-optional-chain:error, unnecessary-optional-chain:off


//Comments with Shadowing Arguments (Error.)

//flowlint unnecessary-invariant:error, unnecessary-assertion:off
//flowlint-line unnecessary-invariant:error, unnecessary-assertion:off


//Comments with Overlapping but Non-Shadowing Arguments (Not an error.)

/*flowlint sketchy-null:error, sketchy-null-bool:off*/ //Unused suppression
/*flowlint-line sketchy-null:error, sketchy-null-bool:off*/ //Unused suppression


//Comments Both Redundant with Themselves and Earlier Settings (Should only show one kind of error.)

/*Clearing the settings; not an error.*/ //flowlint unnecessary-optional-chain:error
//flowlint unnecessary-optional-chain:error, unnecessary-optional-chain:error
//flowlint-line unnecessary-optional-chain:error, unnecessary-optional-chain:error

/*Clearing the settings; not an error.*/ /*flowlint unnecessary-optional-chain:off*/ //Unused suppression
//flowlint unnecessary-optional-chain:off, unnecessary-optional-chain:off
//flowlint-line unnecessary-optional-chain:off, unnecessary-optional-chain:off


//Piecewise Shadowing (Error.)

//flowlint sketchy-null:error, sketchy-null-bool:off, sketchy-null-string:off, sketchy-null-number:off, sketchy-null-mixed:off, sketchy-null-bigint:off
//flowlint-line sketchy-null:error, sketchy-null-bool:off, sketchy-null-string:off, sketchy-null-number:off, sketchy-null-mixed:off, sketchy-null-bigint:off


//Piecewise Almost Shadowing (Not an error.)

/*flowlint sketchy-null:error, sketchy-null-bool:off, sketchy-null-string:off, sketchy-null-number:off*/ //Unused suppressions
/*flowlint-line sketchy-null:error, sketchy-null-bool:off, sketchy-null-string:off, sketchy-null-number:off*/ //Unused suppressions
