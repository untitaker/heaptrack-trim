# heaptrack-trim

A tool to reduce the size of heaptrack profiles

Usage:

```
cargo build --release
cp ./target/release/heaptrack-trim .
zcat large.gz | ./heaptrack-trim 123s | gzip -d > small.gz
```

Now `small.gz` will contain only the allocations `123` seconds after the profiling started.

Note: This does not guarantee _proportionally_ smaller file sizes, for example if your profile is 10 seconds long and you skip the first 5 seconds using `5s`, then that does not mean the output file is exactly half the size (even when comparing uncompressed files)
