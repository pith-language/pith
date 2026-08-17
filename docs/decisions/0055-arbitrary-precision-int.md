---
schema: design-doc/v1
id: decision-0055-arbitrary-precision-int
title: Int is arbitrary precision, with total addition, subtraction, and multiplication
summary: the kernel's integer becomes a normalized sign and magnitude with no bound, written here instead of taken as a dependency; its canonical encoding is a sign byte and a length-prefixed minimal magnitude whose non-minimal spellings the decoder refuses, and division stays absent until it has a consumer that can answer its zero case
kind: decision
status: proposed
created: 2026-08-13
updated: 2026-08-13
tags:
  - values
  - language
  - encoding
  - termination
relations:
  informed_by:
    - research-arithmetic
  depends_on:
    - decision-0018-termination-and-recursion
    - decision-0026-generic-typed-calculus
    - decision-0047-the-declaration-table
    - decision-0048-pre-release-version-pinning
---

# Int is arbitrary precision, with total addition, subtraction, and multiplication

> closes the deferral [0047](0047-the-declaration-table.md) made and named: "arbitrary precision has no consumer — this record says so itself ... it belongs to the record that lands arithmetic, and `Value::Int(i64)` stands until then." 0047 stands unchanged; the argument it made for arbitrary precision is the one below, now landing with the operations that make it measurable rather than prospective.

## context

0026 says `Int` is arbitrary precision. `Value::Int(i64)` implemented sixty-four bits. 0047 examined the gap, agreed with 0026, and deferred anyway on the honest ground that a representation with no consumer is a representation nobody can check: the kernel computed nothing over `Int`, so the totality argument was a claim about arithmetic that did not exist and the round's incidental surface — an arbitrary-precision dependency, a variable-width canonical encoding, digest stability for every `Int`-bearing value — would all have landed unexercised.

the argument that decides it is 0018's, and it is unchanged by the delay. pure evaluation terminates by construction, and 0018 is explicit that a limit "is never the primary defense against non-termination in pure code": a run-time failure on a pure path is what its whole construction exists to avoid. a fixed-width integer makes addition and multiplication partial, so it puts exactly that failure back — an overflow is a pure computation that has no value, and the empty-cache equivalence K-9 states would then hold only for inputs nobody had made too large. an unbounded signed integer closes the operations, and 0018's construction covers arithmetic wholesale rather than by inspection.

the research note beside this record reads five languages at this question and finds the pressure is not hypothetical. Nix's integer is "a signed 64-bit integer" whose overflow was undefined behaviour until 2024, when it became the run-time error 0018 refuses; Python removed the bound after shipping with it and recorded portability as the reason; CUE and Starlark declare exactness in their specifications and put the fixed widths in a constraint layer, which is where 0026 already puts pith's ("bounds on integers remain what 0026 says they are: a library concern via validation"). the note's two agreements across all five matter more than the disagreement, and both are decided below: the operations split, with division partial everywhere it exists at all, and the normal form is not optional.

## proposed decision

### the representation, and why it is not a dependency

`Value::Int` carries an `Int`: a sign and a magnitude in little-endian 32-bit limbs, normalized so that the most significant limb is never zero and zero has an empty magnitude with the sign clear. this is CPython's shape — "In a normalized number, ob_digit[ndigits-1] (the most significant digit) is never zero," with the sign held separately — and Java's, whose `BigInteger` gives zero "a zero-length magnitude array ... whether signum is -1, 0 or 1." the normal form is what makes the derived `PartialEq` and `Hash` agree with numeric equality, and it is the property everything else here rests on.

it is written in this crate instead of taken from `num-bigint` or `ibig`, and the reason is scope, not dependency count. what the kernel needs is four operations and one canonical encoding. what a bignum crate provides is a number-theory surface — division and its remainder conventions, powers, roots, modular arithmetic, bit manipulation, radix conversion, random generation — none of which has a consumer here, and admitting a type whose operations the kernel does not intend to expose invites exactly the drift 0047 diagnosed in `Type::Nominal`: a construct nothing inhabits, followed later by something that inhabits it by accident. the encoding is the second half of the argument: the canonical byte form and its refusals are ours to define whatever the arithmetic underneath, so a dependency would have saved the limb loops and none of the part that decides digest stability.

the lint posture is the third half, and it is the one this record owes an argument for. `arithmetic_side_effects` is denied workspace-wide, and a hand-rolled magnitude is nothing but arithmetic. it needs no escape hatch: every limb operation widens to `u64` first, where `(2^32 - 1)^2` plus two more limbs is `2^64 - 1`, so the products and carries cannot overflow, and they are spelled as `wrapping_` and `checked_` methods rather than operators. the widening is the proof and the method spelling is how the proof is stated to the linter. the workspace's escape-hatch count does not move.

### the encoding, and what the decoder refuses

a value's payload under `TAG_INT` is a sign byte, then the magnitude big-endian behind the same `u64` length prefix `Text` and `Bytes` already carry. big-endian is CBOR's choice for its bignums and Java's for `toByteArray`, and it is the only part of this a second reader would have to agree on.

the part that matters is what the decoder refuses. RFC 8949 states the rule for bignums — the preferred serialization "is to leave out any leading zeroes (note that this means the preferred serialization for n = 0 is the empty byte string)" — and then makes the deeper point in its deterministic-encoding section: a protocol whose field can hold a bignum "needs to specify whether smaller integers are also expressed using these tags," because a format with two spellings of one number has no canonical form. pith's encoding is the input to a digest, so this is not a tidiness question. an integer with two encodings would be a value with two computation keys, and a stored byte string could name a value it does not equal.

so the decoder refuses a leading zero byte and refuses a negative zero instead of normalizing either. normalizing would accept two byte strings for one value and hand back a value whose re-encoding differs from the bytes it came from, which is the property a durable record cannot afford. this is the same posture the record and sum grammars already take, where names out of canonical order are refused rather than sorted.

`ENCODING_VERSION` does not move, under 0048: nothing is released, no reader exists to misread the grammar, and the pre-release database is discarded and rebuilt, which the existing test asserts. every computation key over an `Int`-bearing value moves, which is that discard.

### the operations are three, and division is not among them

addition, subtraction, negation, and multiplication are total, and they are what the type offers. division is absent, and its absence is decided: it is partial in every system that has it — bottom in CUE, a dynamic error in Starlark, an `ArithmeticException` in `BigInteger` — and Dhall, which is the closest system to pith's position on totality, has no division at all and makes subtraction total by truncating it at zero instead. a signed unbounded integer does not need Dhall's trick for subtraction, which is why three operations close and the fourth does not.

what a division would need is the thing this round cannot supply: a consumer that says what `x / 0` evaluates to. under 0018 a pure computation cannot fail arbitrarily, so the answer is either a refusing type (0047's `Unchecked<T>` gate, whose reader does not exist), a declared sum the library owns, or a diagnostic — and picking among those without a caller would repeat exactly the mistake 0047's uncertainty-constructor section names. division arrives with its first consumer, in its own record.

`to_i64` is the boundary the research note says is the only place an unbounded integer costs anything: PEP 237's one recorded cost was that "C code will still need to be aware of the difference between short and long ints." the kernel's own counters, lengths, and budgets stay fixed-width, and the conversion is fallible at that seam. phloem's two integer readers — a record's count field and the resolver's budget — now refuse an out-of-range value from either end rather than from one.

### the projection is a decimal string

`ValueRepr::Int` carries the decimal spelling, not a JSON number. a JSON number is a double to most readers, so a value past 2^53 would be rendered back rounded, which would make the diagnostic surface lie about a value the kernel holds exactly. this is a projection, not an encoding: the wire form above is what the digest covers, and no decimal parser lands with it, because nothing reads one until the surface language exists.

## alternatives considered

### keep the sixty-four-bit integer and check the arithmetic

Nix's answer, arrived at in 2024: keep the width and turn overflow into an evaluation error, `error: integer overflow in adding 9223372036854775807 + 1`.

it is a real option and its cost is measured — the pull request that landed it benchmarks the checks at 0.2% to 0.7%. it is rejected because the failure it produces is the one 0018 excludes by construction: a pure computation that has no value for an input that is otherwise ordinary, which makes K-9's clean-build equivalence conditional on nobody having exceeded the range. Nix's own history is the argument against adopting its position rather than for it: the width was kept for two decades with undefined behaviour underneath, and the fix was available only because Nix had users whose expressions could not be changed. pith has neither the users nor the excuse.

### take `num-bigint` or another arbitrary-precision crate

the ordinary answer, and the one a reviewer should expect.

rejected on surface, not on trust or license. the type this kernel needs has four operations and a canonical encoding; the crate provides a numeric library whose remaining surface has no consumer, and the encoding — the half that decides digest stability — would still have been written here. the hand-rolled magnitude is under four hundred lines of non-test code, checked against `i128` wherever `i128` can answer and against the ring laws past that. if a later round needs division, modular arithmetic, or a fast multiplication for large operands, that is the round where the trade changes and the dependency is the right answer; the operations are named methods rather than operator impls, so swapping the implementation is not a call-site change.

### CBOR bignum tags, or Java's minimal two's complement

encode small integers as fixed-width and large ones under a bignum tag, as Dhall's binary standard does, or drop the sign byte and write a minimal two's-complement magnitude, as `BigInteger.toByteArray` does.

both are established and both were rejected for the same reason. the two-form encoding is precisely what RFC 8949 warns a deterministic protocol has to legislate, and legislating it means a rule about which values may use which form and a decoder that enforces it — two spellings held apart by discipline, where one spelling has nothing to hold apart. two's complement removes the sign byte and replaces it with a sign-bit rule at the magnitude's top, which moves the minimality question from "no leading zero byte" to "no redundant leading sign byte," a rule that is harder to state and harder to test. the sign byte costs one byte per integer and makes the refusal a two-line check.

### starlark-go's small-and-big split

hold values in the 32-bit range as a machine integer and everything else as a heap magnitude, with a predicate deciding which representation a value must use.

the right optimization eventually, and the note reads its implementation for the invariant it enforces: `isSmall` exists because two representations of one value would break equality and hashing unless exactly one is legal per value. rejected for this round because the inline magnitude already covers the old sixty-four-bit range without allocating, and because a second representation doubles the number of places the normal form has to be maintained for a benefit nothing here has measured. if a benchmark ever shows integer allocation on a hot path, this is the change to make, and the invariant to copy with it.

### unsigned naturals with truncating subtraction

Dhall's shape: `Natural` unbounded and unsigned, with `Natural/subtract` saturating at zero instead of going negative.

it makes subtraction total without a sign, which is a genuine solution to the same problem. rejected because truncation makes subtraction stop meaning subtraction — `3 - 5` is `0` — and a build and configuration kernel has ordinary uses for negative numbers that would then need a second scalar. 0026's one-integer position is cheaper, and a signed type gets totality for all three operations without a redefinition.

## consequences

`Value::Int(i64)` is gone, so every construction site in the workspace moves to `Value::int(..)`, which takes any machine integer, and every pattern match on an integer's payload now binds an `Int` rather than an `i64`. the count is 186 sites across five crates, nearly all of them in tests and fixtures, and 0047's estimate that this round "touches no other crate" was wrong in exactly that mechanical way — the type is in `pith-core`, the constructor calls are everywhere.

two fixtures changed meaning as well as spelling, and they are the useful part of the diff. the concurrency suite's batch rule summed its inputs with `saturating_add`, and the pure-step fixtures incremented with it; a saturating add is a fixed-width workaround, and there is nothing left for it to saturate at, so they add now. phloem's `int_field` and its resolver budget stop saying "negative" when they mean "not a count," because an integer that does not fit a `u64` can now miss from above as well as below.

`ValueRepr::Int` carries `decimal` where it carried `n`, a JSON-visible field rename in a pre-release output format.

the pure-computation digest of every `Int`-bearing value moves, which under 0048 is the discard-and-rebuild path and not a version bump.

`CanonicalReader::read_int` keeps its only consumer in its own tests: the value codec no longer reads a fixed-width integer, and the primitive stays because an adapter's storage encoding may still want one. it is the one thing in this round that is now surface without a caller, and it is named here rather than left to be discovered.

### measured

`crates/pith-core/src/int.rs` is 493 lines, 369 of them before its test module, and adds no `allow` for `arithmetic_side_effects` or for anything else: every limb operation widens to `u64` and is spelled as a `wrapping_` or `checked_` method, so the workspace's escape-hatch count is unchanged. the full gate passes: 736 tests at the time of writing, eleven of them added here.

the arithmetic is checked against a wider machine integer wherever one can answer: `arithmetic_agrees_with_a_wider_machine_integer` takes two arbitrary `i64` operands and compares the sum, difference, product, negation, ordering, and `to_i64` against `i128`, which cannot overflow on those operands, so no case is skipped for the oracle's own limits. past that range there is no oracle, and `the_ring_laws_hold_past_the_machine_range` is what replaces it over products of up to four `i64` factors: commutativity and associativity of both operations, distributivity, that adding and then subtracting returns the original, the two identities, the annihilator, and that a value plus its negation is zero.

totality is measured as the absence of a failing case: `a_product_beyond_the_machine_range_is_exact` computes 2^64 by doubling, which no `i64` holds, and the square of `i64::MAX`, which no `i128` holds either, and both render exactly. there is no overflow path to test because the operations return `Self` rather than an `Option`, which is the shape of the totality argument 0018 wanted.

the digest claim is a property, not a fixture: `one_integer_has_one_encoding` asserts that two generated integers encode to equal bytes exactly when they are equal, that the encoding decodes back, and that a value reached by a different route — zero produced by multiplication, a sign produced by multiplying by `-1` — is byte-identical to the same value built directly. the refusals are held from the other side in `non_canonical_integer_bytes_are_rejected`, which hands the decoder a leading zero byte, a leading zero byte under a negative sign, and a negative zero, and gets `NonCanonicalInteger` for each while the empty magnitude still decodes to zero.

the byte layout is pinned in `version_one_value_bytes_are_stable`, which gained zero, a negative, a two-byte magnitude, and 2^64 — the last being a value no `i64` fixture could have written, and the reason the fixture list is where the encoding's grammar is read from.

no value in the engine got wider. `Value` is 48 bytes before and after, because the sum variant's three fields were already wider than an integer is, and `an_arbitrary_precision_integer_does_not_widen_the_value_it_lives_in` asserts that relation from the types and not from a number, so it fails if a later change makes the integer the variant that decides the size. two limbs are inline, so the whole range the type used to hold costs no allocation.

## unresolved

division and its zero case, which arrive together with the first consumer that can say what the result is. the same record decides remainder's convention, where CUE's split between Euclidean `div`/`mod` and truncated `quo`/`rem` is the precedent to argue against.

decimal parsing. rendering landed because the diagnostic and JSON surfaces read it; nothing writes an integer as text into the kernel until the surface language does, and landing the inverse now would be the unread constructor 0047 warns about.

whether `Int` ever needs a bounded companion at the type layer. 0026 says bounds are a library concern via validation, and CUE's `int8` as `>=-128 & <=127` is the same position; nothing has needed it, and the first domain that does will be arguing about validation rather than about scalars.

the comparison surface: `Int` implements `Ord`, and 0040's declared orderings are domain-level. whether the kernel's ordering is ever the one a domain's range constructors use is open, and answering it early would prejudge 0040's protocol.
