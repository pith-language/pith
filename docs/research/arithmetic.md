---
schema: design-doc/v1
id: research-arithmetic
title: unbounded integers
summary: where five languages put the bound on an integer — Python after the fact, Nix at 64 bits, Dhall in the binding, CUE and Starlark nowhere — and what a deterministic encoding of an unbounded one has to refuse
kind: research
status: researching
evidence: preliminary
created: 2026-08-11
updated: 2026-08-11
tags:
  - research
  - language
  - values
  - encoding
relations:
  informed_by:
    - research-declarations
    - research-configuration
  depends_on:
    - research-method
  supersedes: []
---

# unbounded integers

a configuration or build language has to answer what happens when two integers are added and the result does not fit. there are three answers in circulation: the result wraps, the evaluation fails, or the type has no boundary to cross. which one a language picks decides whether arithmetic is a total function, and a language whose termination argument rests on totality cannot leave the question to the machine.

this note reads five languages and three encodings at that question. the languages disagree about where the bound lives; the encodings disagree about whether an unbounded integer has one spelling on the wire, which is a separate question and the one a digest depends on.

## Python: the bound came off afterwards, and the reasons are written down

Python had both a fixed-width `int` and an unbounded `long`, and PEP 237 removed the distinction. its motivation is not elegance. the first reason is portability — "Having the machine word size exposed to the language hinders portability. For examples Python source files and .pyc's are not portable between 32-bit and 64-bit machines because of this" — and the second is that the failure arrives late: "Many programs find a need to deal with larger numbers after the fact, and changing the algorithms later is bothersome." the mechanism is the one this note is about. where "all arithmetic operators on short ints except `<<` raise `OverflowError` if the result cannot be represented as a short int," the operators now "return a long int instead," and the error becomes unreachable rather than handled.

the representation is documented in CPython's own header. digits are 30 bits stored in a 32-bit word (or 15 in a short, on platforms that need it): "There are two different sets of parameters: one set for 30-bit digits, stored in an unsigned 32-bit integer type, and one set for 15-bit digits with each digit stored in an unsigned short." the normal form is stated as an invariant — "In a normalized number, ob_digit[ndigits-1] (the most significant digit) is never zero" — and the sign is separate from the digits, in a tag with three states, "0: Positive, 1: Zero, 2: Negative." a sign-magnitude representation with a normalization rule and a distinguished zero is what an implementation converges on, and CPython says so in a comment rather than leaving it to be inferred.

the PEP's one recorded cost is not arithmetic cost. it is the boundary: "The C API remains unchanged; C code will still need to be aware of the difference between short and long ints." the price of an unbounded integer is paid where it meets a fixed-width consumer, and nowhere else.

## Nix: 64 bits, with the overflow undefined for most of the language's life

the Nix manual is one sentence: "An integer in the Nix language is a signed 64-bit integer." it says nothing about overflow, because until recently there was nothing to say. the pull request that changed it, "Ban integer overflow in the Nix language," describes the previous state as undefined behaviour — signed overflow in the C++ evaluator, caught in the Lix fork only because a sanitizer trapped on it — and the release note for Nix 2.25 shows the new behaviour as a diagnostic: `error: integer overflow in adding 9223372036854775807 + 1`. reading JSON with a value too large for the range became an error at the same time.

this is the most useful single data point in the note. a declarative configuration language with fixed-width integers ran for years with an arithmetic operation whose result was undefined, discovered it, and answered with a run-time failure rather than a wider type. the benchmark in the pull request records that the checks cost between 0.2% and 0.7%, so the fixed width was not being kept for the arithmetic's speed. it was kept because changing the representation of a value type in a shipped language is expensive, which is an argument about Nix's position in its life rather than about the design.

## Dhall: unbounded on the wire, machine arithmetic in the semantics, no division at all

Dhall is the interesting mixed case. its standard declines to fix a range: "the details of how to do so are left open to each implementation, including supported integer ranges or how to idiomatically encode unions." the normalization rules for `+` and `*` are stated as "machine addition" and "machine multiplication" of literals, with no statement of what a machine is. but the binary standard, which is what a hash covers, encodes `Natural` and `Integer` literals as CBOR integers below 2^64 and as CBOR bignums above it, in both directions for `Integer` — so the interchange format is unbounded even though the semantics never says the type is.

the operator set is the other half. Dhall has addition and multiplication and no division at all, and its subtraction is `Natural/subtract`, which truncates: the rule reduces to `n - m` when `m <= n` and to `0` when `n < m`. that is a deliberate choice of a total operator over a partial one, made by giving up the ordinary meaning of subtraction on an unsigned type. a signed type does not need the trick, which is worth stating because it is the reason a signed integer is cheaper to make total than an unsigned one.

## CUE: exact by definition, and the widths are constraints rather than types

CUE states it as a property of literals — "Numeric literals are exact values of arbitrary precision" — and then puts the fixed widths where a language with an exact integer has to put them, in the constraint layer. `int8` is `>=-128 & <=127`, `uint32` is `>=0 & <=4_294_967_295`, and the predeclared list continues; these are derived types spelled as bounds, not separate representations. the only fundamental numeric types are `number`, `int`, and `float`.

division is where CUE's exactness stops, and it stops explicitly. there are four builtins — `div` and `mod` for Euclidean division, `quo` and `rem` for truncated — and "a zero divisor in either case results in bottom (an error)." so the language that made addition and multiplication exact still has one partial operation, and it says which one and what it produces.

## Starlark: arbitrary and exact in the specification, split in the implementation

the specification is unambiguous: "Integers may be positive or negative, and arbitrarily large. Integer arithmetic is exact." the specification does not carve out an implementation allowance, and division is where the partiality is admitted, as a dynamic error.

starlark-go's `int.go` is the implementation counterpart and shows the shape a fast unbounded integer takes: a small representation for values in the 32-bit range and a `big.Int` beyond it, with a predicate deciding which one a value *must* use — `n := x.BitLen(); return n < 32 || n == 32 && x.Int64() == math.MinInt32`. the invariant matters more than the optimization. two representations for one value would break equality and hashing unless exactly one is legal per value, so the implementation enforces a normal form even though the language never mentions one.

## the encodings: an unbounded integer needs a stated minimality rule

CBOR is the specification that writes the rule out. tag 2 and tag 3 carry a byte string in network byte order for arbitrarily sized integers, and the preferred serialization "is to leave out any leading zeroes (note that this means the preferred serialization for n = 0 is the empty byte string)." it then covers the case that matters for a digest: "The preferred serialization of an integer that can be represented using major type 0 or 1 is to encode it this way instead of as a bignum," and the deterministic-encoding section makes it a protocol obligation — a protocol whose field can hold bignums "needs to specify whether smaller integers are also expressed using these tags or using major types 0 and 1." the lesson is not the byte order. it is that admitting a second spelling of one number is a decision the format has to make on purpose, because a format with two spellings has no canonical form.

Java's `BigInteger` is the same lesson from the API side. it is "Immutable arbitrary-precision integers," its zero has one form — "A zero-length magnitude array is permissible, and will result in a BigInteger value of 0, whether signum is -1, 0 or 1" — and `toByteArray` returns "the minimum number of bytes required to represent this BigInteger, including at least one sign bit." minimality is in the contract, not in the implementation's discretion. `BigInteger` also keeps the partial operation partial: "division by zero throws an `ArithmeticException`."

protobuf is the other pole and is included because it is what a build system's wire format usually is. varints "allow encoding unsigned 64-bit integers using anywhere between one and ten bytes," and there is no arbitrary-precision type at all — an integer bigger than 64 bits is not a protobuf field, it is a `bytes` field with a private meaning.

## what the disagreement is

the five languages are not five points on one scale. Python removed the bound after shipping with it and recorded portability as the reason; Nix kept it and, twenty years in, converted its undefined case into a run-time error; Dhall left the range to the binding while its own interchange encoding is unbounded, and avoided the partiality question by having no division and a truncating subtraction; CUE and Starlark declare exactness in the specification and put the fixed widths, where they exist at all, in a constraint layer.

two things every one of them agrees on, and both are more useful than the disagreement:

the operations split. addition, subtraction, and multiplication become total the moment the type is unbounded and signed, and nothing else does. division stays partial in every system that has it — bottom in CUE, a dynamic error in Starlark, an exception in `BigInteger` — and Dhall's answer is to not have it. so "arithmetic is total" is a claim about three operators, and a language that wants it should say which three.

the normal form is not optional. CPython states it in a header comment, starlark-go enforces it with a predicate, `BigInteger` puts it in the constructor's contract, and CBOR makes it a protocol obligation with a named failure — because equality, hashing, and any digest over the value all stop working when one number has two representations. for pith this is the load-bearing part: a computation key is a digest over a canonical encoding, so an integer with two encodings would be a value with two cache entries, and the refusal has to be in the decoder rather than in a normalization step, or a stored byte string could name a value it does not equal.

the pressure that decides pith's own answer is 0018's, and none of these systems has it: pith's termination argument for pure evaluation is by construction, and an overflow failure would be a run-time error on a pure path that the record says must not have one. Nix's history is the counterexample worth naming — the fixed width did not stay cheap, it stayed until the day it produced undefined behaviour, and the fix was the error 0018 refuses.

## sources

- [PEP 237: unifying long integers and integers](https://peps.python.org/pep-0237/)
- [CPython `Include/cpython/longintrepr.h`](https://github.com/python/cpython/blob/main/Include/cpython/longintrepr.h)
- [Nix language types](https://nix.dev/manual/nix/latest/language/types)
- [NixOS/nix#11188: ban integer overflow in the Nix language](https://github.com/NixOS/nix/pull/11188) and [Nix 2.25 release notes](https://nix.dev/manual/nix/2.29/release-notes/rl-2.25.html)
- [Dhall standard README](https://github.com/dhall-lang/dhall-lang/blob/master/standard/README.md), [beta normalization](https://github.com/dhall-lang/dhall-lang/blob/master/standard/beta-normalization.md), and [binary encoding](https://github.com/dhall-lang/dhall-lang/blob/master/standard/binary.md)
- [CUE specification](https://cuelang.org/docs/reference/spec/)
- [Starlark specification](https://github.com/bazelbuild/starlark/blob/master/spec.md) and [starlark-go `starlark/int.go`](https://github.com/google/starlark-go/blob/master/starlark/int.go)
- [RFC 8949: CBOR](https://www.rfc-editor.org/rfc/rfc8949.html), sections 3.4.3 and 4.2
- [`java.math.BigInteger`](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/math/BigInteger.html)
- [protobuf encoding](https://protobuf.dev/programming-guides/encoding/)
