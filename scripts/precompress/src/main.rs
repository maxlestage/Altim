//! `altim-precompress FILE...`: writes FILE.br (brotli, strongest level, 16 MiB window) and FILE.gz (gzip, level 9) next
//! to each file. The server sends them as is (`Content-Encoding`) to the browsers that accept them, instead of
//! compressing on every request at a fast level: the web app's .wasm goes over the network ~20 % smaller.
use std::io::Write;

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("usage: altim-precompress FILE...");
        std::process::exit(2);
    }
    // One thread per file: brotli at its strongest level is slow, and the machine has several cores.
    let failed = std::thread::scope(|s| {
        let jobs: Vec<_> = files.iter().map(|f| (f, s.spawn(move || compress(f)))).collect();
        jobs.into_iter().fold(false, |failed, (f, job)| match job.join() {
            Ok(Ok(())) => failed,
            Ok(Err(e)) => {
                eprintln!("{f}: {e}");
                true
            }
            Err(_) => true,
        })
    });
    if failed {
        std::process::exit(1);
    }
}

fn compress(path: &str) -> std::io::Result<()> {
    let data = std::fs::read(path)?;
    let params = brotli::enc::BrotliEncoderParams { quality: 11, lgwin: 24, size_hint: data.len(), ..Default::default() };
    let mut br = Vec::new();
    brotli::BrotliCompress(&mut data.as_slice(), &mut br, &params)?;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(&data)?;
    let gz = gz.finish()?;
    // A copy no smaller than the file (a tiny loader) is not worth a coding.
    for (copy, ext) in [(&br, "br"), (&gz, "gz")] {
        if copy.len() < data.len() {
            std::fs::write(format!("{path}.{ext}"), copy)?;
        }
    }
    println!("       {path} : {} octets, {} brotli, {} gzip", data.len(), br.len(), gz.len());
    Ok(())
}
