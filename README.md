# Concurrent Cuckoo Hash Table
A Cuckoo hash table functions like a traditional hash table but uses a unique strategy to resolve collisions, inspired by the behavior of the cuckoo bird which lays its eggs in other birds’ nests. In our implementation, each key hashes to two possible buckets. If both buckets are occupied during insertion, the new key evicts one of the current occupants, which is then recursively reinserted into its alternative bucket.

## Hashing Algorithm
To hash each key to two different buckets, we need two distinct hash values. You can achieve this by using a Rust crate that supports seeded hashing, passing in different seeds to generate two hashes for the same key. We used the [xxhash-rust](https://crates.io/crates/xxhash-rust) crate in our solution. Alternatively, you can use two different hashing crates to produce the two hash functions separately.

## Insert
We provide a `KeyVal<K, V>` struct representing a key-value pair to be inserted into the hash table. If either of the two buckets the key hashes to is empty, the pair can be inserted directly. However, if both buckets are occupied, we must evict one of the existing entries to make room.

In extreme cases such as when the table is full, blindly evicting and reinserting entries can lead to infinite loops, since no spot is actually available. To prevent this, we impose an upper bound on the number of displacements allowed during an insertion. This ensures we avoid both infinite loops and long chains of evictions.

We also shouldn't start evicting entries unless we know the insertion will ultimately succeed. Instead, we first attempt to find an insertion path, which is a sequence of displacements that leads to a free slot. If such a path exists, we then perform the insertions along that path. However, if no valid path is found within the upper bound limit, we resize the hash table to make room and try again.

### Insertion Steps
1. Attempt direct insertion into either of the two buckets the key hashes to.
2. If both buckets are occupied, search for an insertion path (a sequence of displacements) that will lead to a free bucket. This can be implemented using either DFS or BFS.
3. If we find a path, perform the displacements and the insertion along the discovered path.
4. If we cannot find a path, resize the hash table and repeat the process.

## Resize
To resize the hash table, choose a larger capacity and reinsert all existing key-value pairs into the new table. This is necessary because the keys will now hash to different buckets based on the new size.

## Lookup
Lookup is very fast (O(1) time) and simple to implement. Since each key can only reside in one of its two hashed buckets, we just check both buckets. If the key is not found in either, it is not in the table.

## Remove
Remove is also very fast (O(1) time) and can be implemented similarly as lookup.

## Start Implementing
You can now begin implementing a non-concurrent version of the Cuckoo hash table based on the descriptions above. Alternatively, feel free to keep reading the sections below for more details before getting started.

## Concurrency
To support safe concurrent access in the Cuckoo hash table, we adopt Rust’s shared mutability pattern. This enables multiple threads to share ownership of the data while ensuring that only one thread can mutate a given part at a time.

### Locking Granularity
Your implementation should use fine-grained locking by associating a separate lock with each bucket, rather than applying a single global lock across the entire table. This approach improves concurrency by allowing multiple threads to operate on different buckets simultaneously.

### Insert
In step 2, when you try to find a path, it is not necessary to lock all buckets along the path. In read-heavy workloads, the structure of the table is unlikely to change significantly during the search, so we proceed optimistically. The path is validated later during the actual displacement process in step 3.

In step 3, we also avoid locking the entire path upfront. Instead, we perform displacements incrementally, validating each step as we go. A displacement is considered valid if the key in the current bucket can still be hashed to the alternative bucket specified in the path. If any displacement is found to be invalid, the path is considered outdated, and the insertion attempt is aborted. We then return to step 2 to search for a new path. Importantly, even if we abort mid-way, we do not need to revert the partial displacements already made because the displaced keys are still in one of their hashed buckets! If all displacements complete successfully, then an empty slot opens up for our new key and we can complete the insertion.

Hint: After implementing, think about whether a deadlock can happen!

### Resize
Resizing requires rehashing and relocating all elements in the table, so no other threads should access the table during the process.

## Testing
You can write some simple unit tests in main while you implement and run them by `cargo run`.

If you are fairly confident about your implementation, run the unit tests and loom tests we provided by `cargo test` to check the correctness.

## References
Algorithm inspired by the description in this [paper](https://www.cs.princeton.edu/~mfreed/docs/cuckoo-eurosys14.pdf), but with 1 set associativity, i.e. each bucket has one key-value pair.
