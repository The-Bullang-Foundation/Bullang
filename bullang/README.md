# Bullang

Bullang is a language you write once and transpile into several target
languages. Its unit of code is the **bullet**: one input list, one operation,
one named result, on one line.

```
let calculus(a: i32, b: i32, c: i32) -> result: i32 {
    (a, b)   : a + b   -> {ab};
    (ab, c)  : ab * c  -> {result};
}
```

This repository holds the language definition: grammar, AST, parser, formatter
and the core standard library catalogue. **Bullang describes the language; it
does not run it.** Turning a `.bu` file into Rust, Python, C, C++, Go or Java is
[Bullarchy](https://github.com/The-Bullang-Foundation/Bullarchy)'s job, and
compiling or running the result is the target toolchain's.

For a scripting language with its own interpreter, see
[BullScript](https://github.com/The-Bullang-Foundation/Bullscript) — a separate
language with a separate grammar, running `.busc` files. It borrows Bullang's
bullet syntax for familiarity and shares none of its code.

---

## Prerequisite

Cargo v1.92.0 or later (edition 2024).

## Installation

```bash
cargo install --git https://github.com/The-Bullang-Foundation/Bullang.git
```

Add `--force bullang` when reinstalling over an existing version.

Most of the time you want Bullarchy instead — it depends on this crate and
brings the transpiler, project tooling and LSP with it.

## The binary

```bash
bullang stdlib     # browse the core builtin catalogue
```

That is all it does. Updating is `bullarchy update`.

---

## Design

Bullang aims at code that reads the same in every language it becomes. Two
rules follow from that.

**One operation per bullet.** `a + b` is a bullet; `a + b + c` is two. There is
no operator precedence to remember and no parentheses to trace, because the
shape of the code is the order of evaluation. A call cannot be nested inside
another call for the same reason — write the inner call as its own bullet and
pass its binding.

**Only what translates faithfully.** A feature earns its place by having an
obvious, honest counterpart in all six backends. References (`&T`), function
types, closures and generic arguments did not, and were removed rather than
approximated. Fixed-size arrays looked primitive but were the worst offender:
C cannot return one, and value-versus-alias semantics split the targets down
the middle.

Collections will come back as one designed feature — literals, indexing, length
and iteration together — rather than as a type with no way to build a value.

---

## Types

| Type | Description |
|---|---|
| `i32` `i64` | signed integers |
| `f64` | floating point |
| `bool` | true / false |
| `String` | UTF-8 text |
| `Tuple[A, B]` | tuple |
| `()` | no value |

Structs and enums are declared in `inventory.bu` and are usable by name in the
same folder and one rank above.

---

## Bullets

```
(inputs) : expression -> {binding};
```

| Part | Meaning |
|---|---|
| `(inputs)` | values passed in — identifiers or literals |
| `: expression` | the operation: arithmetic, a call, or a builtin |
| `-> {binding}` | names the result; `{}` discards it |

The last bullet's binding is the function's return value.

```
(a, b)      : a + b               -> {sum};
(sum)       : builtin::to_string  -> {text};
(1, text)   : builtin::out        -> {};
```

Operators: `+ - * / %`, `== != < <= > >=`, `&& ||`, unary `!` and `-`.

Strings interpolate with `{name}`, and `{{` / `}}` write a literal brace:

```
(name, age) : "hello {name}, you are {age}" -> {greeting};
```

---

## The standard library

Bullang's core is deliberately small — see `bullang stdlib` for the full list.
Anything beyond it lives in a package installed through Bullarchy:

```bash
bullarchy add mathlib
```

```
#use: mathlib;
```

`bull-mathlib` supplies `abs`, `pow`, `powf`, `sqrt`, `clamp`, `log`, `exp` and
four sorting builtins. They are not core, and never were meant to be.

---

## Escape blocks

When a function needs something only one target can express:

```
let add(a: i32, b: i32) -> result: i32 {
    @rust
        let result = a + b;
    @end
}
```

An escape block is a macro. Bullang decides three things about it — where it
starts, where it ends, and which backend it names — and copies everything
between into the generated file **byte for byte**, indentation included. It is
never parsed, never reindented, never validated.

One block per function, and a function cannot mix bullets and a block.
Backends: `@rust` `@python` `@c` `@cpp` `@go` `@java`.

---

## inventory.bu

Each folder in a project carries one, declaring its rank, its target language,
its dependencies, its types and its files.

```
#rank: tactic;
#lang: rs;
#lib: stdio.h;
#use: mathlib;

struct Point {
    x : i32,
    y : i32,
}

enum Color {
    Red,
    Green,
    Blue,
}

math   : add, subtract;
shapes : area, perimeter;
```

`#lib` and `#use` are different things: `#lib` names a native header or system
library of the target language, `#use` names a Bullang package.

Project layout, ranks and the rest of the tooling are documented in Bullarchy.
