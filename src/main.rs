use std::{
    cmp::Ord,
    path::{Path, PathBuf},
    process::ExitCode,
    fs, fs::File,
    io, io::{Read, Write},
};

use crossbeam_channel as mpmc;

use clap::Parser;
use wax::{Glob, Pattern};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Invocation {
    /// Files (or, with -r, directories) to crawl
    #[arg(required=true)]
    files: Vec<PathBuf>,
    /// A wax glob to exclude from the output and from crawling
    #[arg(short, long)]
    exclude: Vec<String>,
    /// A wax glob to include, even if the file matches an excluded glob
    #[arg(short, long)]
    include: Vec<String>,
    /// When a directory is encountered, descend into it, and also process its
    /// files and subdirectories.
    #[arg(short, long)]
    recursive: bool,
    /// When a symbolic link is encountered, IGNORE IT, instead of processing
    /// or descending through it.
    #[arg(long)]
    no_follow: bool,
}

fn descend(scan_tx: &mut mpmc::Sender<(PathBuf, u64)>, invocation: &Invocation, includes: &Vec<Glob>, excludes: &Vec<Glob>, path: &Path) -> Result<(), ()> {
    if excludes.iter().any(|x| x.is_match(path)) {
        if !includes.iter().any(|x| x.is_match(path)) {
            return Ok(())
        }
    }
    let res = if invocation.no_follow {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.is_symlink() { return Ok(()) }
                Ok(metadata)
            },
            Err(x) => Err(x),
        }
    } else { fs::symlink_metadata(path) };
    let metadata = match res {
        Ok(x) => x,
        Err(x) => {
            eprintln!("{:?}: {}", path, x);
            return Err(())
        },
    };
    if metadata.is_dir() {
        if invocation.recursive {
            for el in match fs::read_dir(path) {
                Ok(x) => x, Err(x) => {
                    eprintln!("{:?}: {}", path, x);
                    return Err(())
                },
            } {
                let el = match el {
                    Ok(x) => x, Err(x) => {
                        eprintln!("{:?}: {}", path, x);
                        return Err(())
                    }
                };
                descend(scan_tx, invocation, includes, excludes, &el.path())?;
            }
            Ok(())
        }
        else {
            Ok(())
        }
    }
    else {
        scan_tx.send((path.to_path_buf(), metadata.len())).map_err(|_| ())
    }
}

fn sum_file(path: &Path, meta_size: u64, result_tx: &mut mpmc::Sender<(String, [u8;32], u64)>) -> std::io::Result<()> {
    let path_as_str = match path.to_str() {
        Some(x) => x,
        None => {
            return Err(io::Error::new(io::ErrorKind::Other, "invalid Unicode in path"))
        },
    };
    let mut f = File::open(path)?;
    let mut buf = [0u8; 32768];
    let mut hasher = lsx::sha256::BufSha256::new();
    let mut read_size: u64 = 0;
    while read_size <= meta_size {
        match f.read(&mut buf[..]) {
            Ok(0) => break,
            Ok(x) => { read_size += x as u64; hasher.update(&buf[..x]) },
            Err(x) => return Err(x),
        }
    }
    if read_size != meta_size {
        return Err(io::Error::new(io::ErrorKind::Other, "metadata size and file size don't match"))
    }
    let sum = hasher.finish(&[]);
    result_tx.send((path_as_str.to_string(), sum, meta_size)).map_err(|_| {
        return io::Error::new(io::ErrorKind::Other, "channel closed unexpectedly")
    })
}

fn summer(scan_rx: mpmc::Receiver<(PathBuf, u64)>, mut result_tx: mpmc::Sender<(String, [u8;32], u64)>) -> Result<(), ()> {
    while let Ok((path, size)) = scan_rx.recv() {
        match sum_file(&path, size, &mut result_tx) {
            Ok(_) => (),
            Err(x) => {
                eprintln!("{:?}: {}", path, x);
            },
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let invocation = Invocation::parse();
    let (mut scan_tx, scan_rx) = mpmc::bounded(num_cpus::get() + 3);
    let (result_tx, result_rx) = mpmc::bounded(num_cpus::get() + 3);
    let scan_thread = std::thread::Builder::new()
    .name("scan".to_string()).spawn(move || -> Result<(), ()> {
        let includes: Vec<Glob> = match invocation.include.iter().map(|src| {
            Glob::new(src).map_err(|x| {
                eprintln!("Invalid --include glob {:?}: {}", src, x);
                ()
            })
        }).collect::<Result<Vec<Glob>, ()>>() {
            Ok(x) => x,
            Err(_) => return Err(()),
        };
        let excludes: Vec<Glob> = match invocation.exclude.iter().map(|src| {
            Glob::new(src).map_err(|x| {
                eprintln!("Invalid --exclude glob {:?}: {}", src, x);
                ()
            })
        }).collect::<Result<Vec<Glob>, ()>>() {
            Ok(x) => x,
            Err(_) => return Err(()),
        };
        for path in invocation.files.iter() {
            descend(&mut scan_tx, &invocation, &includes, &excludes, path)?;
        }
        Ok(())
    }).unwrap();
    let summers = (1 ..= num_cpus::get()).map(|n| {
        let scan_rx = scan_rx.clone();
        let result_tx = result_tx.clone();
        std::thread::Builder::new()
        .name(format!("summer {}", n)).spawn(move || {
            summer(scan_rx, result_tx)
        }).unwrap()
    }).collect::<Vec<_>>();
    drop(result_tx);
    let mut results = vec![];
    while let Ok(result) = result_rx.recv() {
        results.push(result);
    }
    for summer in summers.into_iter() {
        match summer.join() {
            Ok(Ok(())) => (),
            _ => return ExitCode::FAILURE,
        }
    }
    match scan_thread.join() {
        Ok(Ok(())) => (),
        _ => return ExitCode::FAILURE,
    }
    results.sort_by(|a, b| {
        a.0.cmp(&b.0)
    });
    let mut output = Vec::with_capacity(results.iter().fold(0, |a,x| {
        a + 43 + x.0.len()
    }));
    for result in results.iter() {
        output.extend_from_slice(result.0.as_bytes());
        output.push(b'\n');
        output.extend_from_slice(&result.1);
        output.extend_from_slice(&result.2.to_be_bytes());
        output.extend_from_slice(&[0,0]); // for future expansion
    }
    if output.len() > u32::MAX as usize {
        panic!("Catalog files cannot be longer than 4GiB before compression. (That's enough for over 50 MILLION files.)");
    }
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all(b"\xFFTCat").unwrap();
    stdout.write_all(&lsx::sha256::hash(&output[..])).unwrap();
    stdout.write_all(&(output.len() as u32).to_be_bytes()).unwrap();
    let mut encoder = flate2::write::ZlibEncoder::new(&mut stdout, flate2::Compression::best());
    encoder.write_all(&output).unwrap();
    encoder.finish().unwrap();
    ExitCode::SUCCESS
}
