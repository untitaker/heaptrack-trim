use std::cmp::max;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::os::fd::{FromRawFd, IntoRawFd};

use argh::FromArgs;

#[derive(FromArgs)]
#[argh(description = "cut out irrelevant parts of heaptrack profiles, to reduce file size")]
struct Cli {
    /// skip the first N seconds of the profile. required.
    #[argh(option)]
    skip_seconds: u64,

    /// do not rewrite timestamps, leaving the scale of graphs in heaptrack-gui intact.
    ///
    /// Makes for easier comparison to the original profile. However, there will be large, ugly
    /// gaps in the graphs where data was removed.
    #[argh(switch)]
    preserve_time: bool,

    /// how large should the read and write buffers be? defaults to 1e15 bytes
    #[argh(option, default = "1 << 15")]
    buf_size: usize,
}

fn main() {
    let cli: Cli = argh::from_env();

    // hacks to get large stdio buffer
    let stdin = unsafe { File::from_raw_fd(0) };
    let stdout = unsafe { File::from_raw_fd(1) };
    let buf_size = cli.buf_size;

    let mut reader = BufReader::with_capacity(buf_size, stdin);
    let mut writer = BufWriter::with_capacity(buf_size, stdout);

    run_main(
        cli.skip_seconds * 1000,
        cli.preserve_time,
        &mut reader,
        &mut writer,
    )
    .unwrap();

    // do not close stdio
    let _ = reader.into_inner().into_raw_fd();
    let _ = writer.into_inner().unwrap().into_raw_fd();
}

fn run_main(
    skip_timestamp: u64,
    preserve_time: bool,
    mut input: impl BufRead,
    mut output: impl Write,
) -> Result<(), io::Error> {
    // the layout of a heaptrack profile was mostly reverse-engineered from C++ sourcecode of
    // heaptrack-gui
    // relevant files:
    // https://github.com/KDE/heaptrack/blob/6b4ca14b78e46783750943afef71e4cb3cade537/src/analyze/accumulatedtracedata.cpp
    // https://github.com/KDE/heaptrack/blob/6b4ca14b78e46783750943afef71e4cb3cade537/src/analyze/gui/parser.cpp
    //
    // parser.cpp is a subclass of accumulatedtracedata, and implements a few callbacks for
    // GUI-specific datastructures. The meat of the parser is in accumulatedtracedata.
    //
    // tldr: after decompression, the file is plaintext, and each line in the file is a command.
    // the command type is the first character on the line.
    //
    // * "c ..." command sets the current time since the start of the profile in milliseconds, in
    //   hex. For example, "c deadbeef" means we are 0xdeadbeef milliseconds into the profile.
    // * Allocations are registered with lines starting with "a ...", and get a sequential ID
    //   starting from 0 called "allocation index."
    // * Lines starting with "+ ..." or "- ..." refer to these allocations using their index in
    //   hex. + means alloc, - means free.
    // * There are a few other commands like "s ..." to index strings that then are referenced in
    //   allocations. But this actually doesn't matter to us, because all that we are trying to do
    //   is to reduce the file-size drastically, not make the smallest file possible. Since most
    //   lines are + and -, it is actually sufficient to remove _only those_ and leave the rest
    //   as-is, leaving potentially unused strings in the file.
    // * However, the first reference to an allocation in -/+ lines MUST be 0.
    //   If there is an allocation index N > 0 referenced in a -\+ command, an allocation index N - 1
    //   must have been used before. Otherwise, heaptrack-gui segfaults as it tries to access
    //   some internal array out of bounds. This means that we have to "rebase" all unfiltered
    //   allocations to start at 0, and remove extraneous "a ..." lines.
    let mut allocation_index_correction = 0u64;
    let mut largest_written_allocation_index = 0u64;

    let mut line_buf = Vec::new();

    let mut is_skipping = true;
    // duration since the start of the input profile
    let mut current_abs_timestamp_ms = 0u64;

    loop {
        line_buf.clear();
        let read_bytes = input.read_until(b'\n', &mut line_buf)?;
        if read_bytes == 0 {
            break;
        }

        let line = line_buf.as_slice();

        let instruction = line[0];

        match instruction {
            b'c' => {
                let mut args = line.trim_ascii_end().split(|x| *x == b' ').skip(1);
                current_abs_timestamp_ms = parse_hex(args.next().unwrap()).unwrap();

                if is_skipping && current_abs_timestamp_ms > skip_timestamp {
                    eprintln!(
                        "stopped skipping at profile timestamp {}, writing all data now",
                        current_abs_timestamp_ms
                    );
                    is_skipping = false;
                }

                if !is_skipping {
                    if preserve_time {
                        output.write(line)?;
                    } else {
                        output.write(b"c ")?;
                        write_hex(&mut output, current_abs_timestamp_ms - skip_timestamp)?;
                        output.write(b"\n")?;
                    }
                }
            }
            b'+' | b'-' => {
                let mut args = line.trim_ascii_end().split(|x| *x == b' ').skip(1);
                let allocation_index = parse_hex(args.next().unwrap()).unwrap();
                if allocation_index > allocation_index_correction {
                    if is_skipping {
                        allocation_index_correction = allocation_index;
                    } else {
                        let new_allocation_index =
                            allocation_index - allocation_index_correction - 1;
                        debug_assert!(
                            new_allocation_index <= largest_written_allocation_index + 1,
                            "{} not within bounds of {}",
                            allocation_index,
                            largest_written_allocation_index
                        );

                        output.write(&line[..1])?;
                        output.write(b" ")?;
                        write_hex(&mut output, new_allocation_index)?;
                        output.write(b"\n")?;

                        largest_written_allocation_index =
                            max(new_allocation_index, largest_written_allocation_index);
                    }
                }
            }
            b'a' => {
                if !is_skipping {
                    output.write(&line)?;
                }
            }
            _ => {
                output.write(&line)?;
            }
        }
    }

    eprintln!(
        "done. total time of profile was {}",
        current_abs_timestamp_ms
    );
    Ok(())
}

#[inline]
fn parse_hex(input: &[u8]) -> Result<u64, ()> {
    let mut rv = 0u64;
    for c in input {
        rv *= 16;
        rv |= match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => 10 + c - b'a',
            b'A'..=b'F' => 10 + c - b'A',
            _ => return Err(()),
        } as u64;
    }

    Ok(rv)
}

// do not use writeln!(), format strings or from_str_radix, those are slow and handle more
// edgecases than we need.
#[inline]
fn write_hex(mut writer: impl Write, input: u64) -> Result<(), io::Error> {
    if input == 0 {
        writer.write(b"0")?;
        return Ok(());
    }

    for byte in input.to_be_bytes() {
        for c in [(byte / 16) as u8, (byte % 16) as u8] {
            if c != 0 {
                writer.write(&[if c < 10 { b'0' + c } else { b'a' + (c - 10) }])?;
            }
        }
    }

    Ok(())
}

#[test]
fn test_hex() {
    assert_eq!(parse_hex(b"1"), 1);
    assert_eq!(parse_hex(b"a"), 10);
    assert_eq!(parse_hex(b"7d0"), 2000);
    assert_eq!(parse_hex(b"3e8"), 1000);
}

#[test]
fn basic() {
    use std::io::Cursor;

    let mut output = Vec::<u8>::new();
    run_main(
        1000,
        false,
        Cursor::new(
            b"\
+ 0
c 7d0
+ 1
+ 2
+ 3
+ 4",
        ),
        &mut output,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(output).unwrap(),
        "\
c 3e8
+ 0
+ 1
+ 2
+ 3\n"
    );
}
