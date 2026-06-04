# fcars [![Rust](https://github.com/diracdeltafunk/fcars/actions/workflows/rust.yml/badge.svg)](https://github.com/diracdeltafunk/fcars/actions/workflows/rust.yml)

`fcars` stands for **Formal Concept Analysis in Rust**.

`fcars` is both a binary (which just computes formal concepts from the command line) and a library (which you can use as a tool for arbitrary FCA computations).

## Usage

### Binary

To install the `fcars` binary on your system, you need to have `cargo` installed. Then, just run

```console
> cargo install --git https://github.com/diracdeltafunk/fcars.git
```

#### Example Binary Usage

A classic example in Formal Concept Analysis is the "Lives in Water" lattice. Using the `.cxt` file provided [here](https://upriss.github.io/fca/examples.html) by Uta Priss, we can have `fcars` enumerate the concepts in this lattice:

```console
> fcars --cxt lives_in_water.cxt
```

<details>
<summary>Expected Output</summary>

```text
Extent: ["fish leech", "bream", "frog", "dog", "water weeds", "reed", "bean", "corn"], Intent: ["needs water to live"]
Extent: ["fish leech", "bream", "frog", "water weeds", "reed"], Intent: ["needs water to live", "lives in water"]
Extent: ["frog", "dog", "reed", "bean", "corn"], Intent: ["needs water to live", "lives on land"]
Extent: ["water weeds", "reed", "bean", "corn"], Intent: ["needs water to live", "needs chlorophyll"]
Extent: ["fish leech", "bream", "frog", "dog"], Intent: ["needs water to live", "can move"]
Extent: ["frog", "reed"], Intent: ["needs water to live", "lives in water", "lives on land"]
Extent: ["water weeds", "reed"], Intent: ["needs water to live", "lives in water", "needs chlorophyll", "monocotyledon"]
Extent: ["fish leech", "bream", "frog"], Intent: ["needs water to live", "lives in water", "can move"]
Extent: ["reed", "bean", "corn"], Intent: ["needs water to live", "lives on land", "needs chlorophyll"]
Extent: ["frog", "dog"], Intent: ["needs water to live", "lives on land", "can move", "has limbs"]
Extent: ["water weeds", "reed", "corn"], Intent: ["needs water to live", "needs chlorophyll", "monocotyledon"]
Extent: ["bream", "frog", "dog"], Intent: ["needs water to live", "can move", "has limbs"]
Extent: ["reed"], Intent: ["needs water to live", "lives in water", "lives on land", "needs chlorophyll", "monocotyledon"]
Extent: ["frog"], Intent: ["needs water to live", "lives in water", "lives on land", "can move", "has limbs"]
Extent: ["bream", "frog"], Intent: ["needs water to live", "lives in water", "can move", "has limbs"]
Extent: ["bean"], Intent: ["needs water to live", "lives on land", "needs chlorophyll", "dicotyledon"]
Extent: ["reed", "corn"], Intent: ["needs water to live", "lives on land", "needs chlorophyll", "monocotyledon"]
Extent: ["dog"], Intent: ["needs water to live", "lives on land", "can move", "has limbs", "breast feeds"]
Extent: [], Intent: ["needs water to live", "lives in water", "lives on land", "needs chlorophyll", "dicotyledon", "monocotyledon", "can move", "has limbs", "breast feeds"]
```

</details>

`fcars -h` displays full usage info.

### Library

To use the `fcars` library, add

```toml
fcars = {git = "https://github.com/diracdeltafunk/fcars.git"}
```

to the `[dependencies]` section of your `Cargo.toml` file.

Or, to also enable generating random formal contexts, use

```toml
fcars = {git = "https://github.com/diracdeltafunk/fcars.git", features=["random"]}
```

#### Example Library Usage

With the "random" feature enabled, you can write:

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
