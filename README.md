# Cuckoo

Concurrent [cuckoo hash table](https://en.wikipedia.org/wiki/Cuckoo_hashing) exploration.

Run Loom tests with this command:

```
RUSTFLAGS="--cfg loom" cargo test --test loom_tests --release
```

