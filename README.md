# fcars [![Rust](https://github.com/diracdeltafunk/fcars/actions/workflows/rust.yml/badge.svg)](https://github.com/diracdeltafunk/fcars/actions/workflows/rust.yml)


`Formal Concept Analysis in Rust`

## Installation

In your cargo project, add
```toml
"fcars" = {git = "https://github.com/diracdeltafunk/fcars.git"}
```
to the `[dependencies]` section of your `Cargo.toml` file.

Or, to also enable generating random formal contexts, use
```toml
"fcars" = {git = "https://github.com/diracdeltafunk/fcars.git", features=["random"]}
```
