# heaptrack-trim

A tool to reduce the size of [heaptrack](https://invent.kde.org/sdk/heaptrack) profiles.

Usage:

```
cargo build --release
zcat large-profile.gz | ./target/release/heaptrack-trim --skip-seconds 123 | gzip > small.gz
```


Now `small.gz` will contain only the allocations `123` seconds after the profiling started.

heaptrack-trim does not deal with compression at all, and only filters from stdin to stdout.

## Options

```
Usage: heaptrack-trim --skip-seconds <skip-seconds> [--preserve-time] [--buf-size <buf-size>]

cut out irrelevant parts of heaptrack profiles, to reduce file size

Options:
  --skip-seconds    skip the first N seconds of the profile. required.
  --preserve-time   do not rewrite timestamps, leaving the scale of graphs in
                    heaptrack-gui intact. Makes for easier comparison to the
                    original profile. However, there will be large, ugly gaps in
                    the graphs where data was removed.
  --buf-size        how large should the read and write buffers be? defaults to
                    1e15 bytes
  --help            display usage information
```


## Caveats

* This does not guarantee _proportionally_ smaller file sizes, for example if
  your profile is 10 seconds long and you skip the first 5 seconds using `5s`,
  then that does not mean the output file is exactly half the size (even when
  comparing uncompressed files)

* _Some_ data from the skipped seconds is also included for simplicity, but the
  vast majority should be removed (see source code)

## License

MIT
