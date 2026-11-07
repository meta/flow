// A bare decorator call cannot be followed by another member or call.
@decorator().member class Member {} // ERROR
@decorator()() class Call {} // ERROR
