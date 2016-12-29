# futures-spawn

An abstraction for values that spawn futures.

[![Crates.io](https://img.shields.io/crates/v/futures-spawn.svg?maxAge=2592000)](https://crates.io/crates/futures-spawn)

[Documentation](https://docs.rs/futures-spawn)

## Overview

[futures-rs](http://github.com/alexcrichton/futures-rs) provides a task
abstraction and the ability for custom executors to manage how future
execution is scheduled across them.

`futures-spawn` provides an abstraction representing the act of spawning a
future. This enables writing code that is not hard coded to a specific
executor.

## Roadmap

Ideally, this crate will be merged into futures-rs proper.

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
futures-spawn = "0.1"
```

Or, to use without pulling in `tokio-core`, add this to your `Cargo.toml` file:

```toml
[dependencies.futures-spawn]
version = "0.1"
default-features = false
features = ["use_std"]
```

Next, add this to your crate:

```rust
extern crate futures_spawn;
```

And then, spawn some futures!

## License

`futures-spawn` is primarily distributed under the terms of both the MIT license
and the Apache License (Version 2.0), with portions covered by various BSD-like
licenses.

See LICENSE-APACHE, and LICENSE-MIT for details.
