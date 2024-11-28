use std::env::args;
use std::cmp::max;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::fs::File;
use std::os::fd::{FromRawFd, IntoRawFd};

use argh::FromArgs;

#[derive(FromArgs)]
struct Cli {
    #[argh(option, required)]
    skip_seconds: u64,
}

fn main() {
    let cli: Cli = argh::from_env();

    // hacks to get large stdio buffer
    let stdin = unsafe { File::from_raw_fd(0) };
    let stdout = unsafe { File::from_raw_fd(1) };
    let size = 32768;
    let mut reader = BufReader::with_capacity(size, stdin);
    let mut writer = BufWriter::with_capacity(size, stdout);

    run_main(cli.skip_seconds, &mut reader, &mut writer).unwrap();

    // do not close stdio
    let _ = reader.into_inner().into_raw_fd();
    let _ = writer.into_inner().unwrap().into_raw_fd();
}

fn run_main(skip_seconds: u64, mut input: impl BufRead, mut output: impl Write) -> Result<(), io::Error> {
    let mut allocation_index_correction = 0u64;
    let mut largest_written_allocation_index = 0u64;

    let mut line_buf = Vec::new();

    loop {
        line_buf.clear();
        let read_bytes = input.read_until(b'\n', &mut line_buf)?;
        if read_bytes == 0 {
            break;
        }

        let line = line_buf.as_slice();

        let instruction = line[0];
        match instruction {
            b'+' | b'-' => {
                let mut args = line.trim_ascii_end().split(|x| *x == b' ').skip(1);
                let allocation_index = std::str::from_utf8(args.next().unwrap()).unwrap();
                let allocation_index = u64::from_str_radix(allocation_index, 16).unwrap();
                if allocation_index > allocation_index_correction {
                    if current_line <= skip_lines {
                        allocation_index_correction = allocation_index;
                    } else {
                        output.write(&line[..1])?;
                        let new_allocation_index = allocation_index - allocation_index_correction - 1;
                        debug_assert!(new_allocation_index <= largest_written_allocation_index + 1, "{} not within bounds of {}", allocation_index, largest_written_allocation_index);
                        writeln!(output, " {:x}", new_allocation_index)?;
                        largest_written_allocation_index = max(new_allocation_index, largest_written_allocation_index);
                    }
                }
            }
            _ => {
                output.write(&line)?;
                output.write(b"\n")?;
            }
        }
    }

    Ok(())
}

#[test]
fn basic() {
    use std::io::Cursor;

    let mut output = Vec::<u8>::new();
    run_main(1, Cursor::new(b"\
+ 0
+ 1
+ 2
+ 3
+ 4"), &mut output).unwrap();

    assert_eq!(String::from_utf8(output).unwrap(), "\
+ 0
+ 1
+ 2
+ 3\n");
}
