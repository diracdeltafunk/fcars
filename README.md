# fcars [![Rust](https://github.com/diracdeltafunk/fcars/actions/workflows/rust.yml/badge.svg)](https://github.com/diracdeltafunk/fcars/actions/workflows/rust.yml)


`Formal Concept Analysis in Rust`

## Installation

In your cargo project, add
```toml
fcars = {git = "https://github.com/diracdeltafunk/fcars.git"}
```
to the `[dependencies]` section of your `Cargo.toml` file.

Or, to also enable generating random formal contexts, use
```toml
fcars = {git = "https://github.com/diracdeltafunk/fcars.git", features=["random"]}
```

## Example

With the "random" feature enabled:

```rust
use fcars::*;

fn main() {
    let context = FormalContext::random_with_density(10, 12, 0.8);
    println!("Context:\n{}", context);
    let concepts = context.all_concepts();
    println!("Reduced? {}\n", context.is_reduced());
    for concept in concepts {
        println!("{}", concept);
    }
}
```
