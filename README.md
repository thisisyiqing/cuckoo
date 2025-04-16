# Cuckoo

Concurrent [cuckoo hash table](https://en.wikipedia.org/wiki/Cuckoo_hashing) exploration.

Implemented based on the description in this [paper](https://www.cs.princeton.edu/~mfreed/docs/cuckoo-eurosys14.pdf), but with 1 set associativity, i.e. each bucket has one key-value pair.

Run Loom tests with this command:

```
RUSTFLAGS="--cfg loom" cargo test --test loom_tests --release
```

